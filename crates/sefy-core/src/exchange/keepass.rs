//! KeePass XML: what KeePass 2 writes under "Export → KeePass XML (2.x)" and
//! KeePassXC under `keepassxc-cli export --format xml`, and what both import.
//!
//! A KeePass entry is a bag of named strings, five of them standard (`Title`,
//! `UserName`, `Password`, `URL`, `Notes`), plus attachments and a history of
//! whole earlier snapshots. The mapping is the same for every kind: a
//! record's `login`, `password`, `url` and `notes` take the standard places,
//! every other field becomes a string of its own, protected when it is
//! secret. The sefy kind travels in the entry's custom data, which KeePass
//! keeps without showing, so a round trip through KeePass comes back a card
//! and not a login with odd fields; an entry without it — one KeePass made —
//! is a note when all it has is notes, and a login otherwise.
//!
//! Groups have no sefy counterpart, tags do: an entry's group path becomes a
//! tag (`Internet/Shops`), and its own tags stay tags. A one-time password key
//! is written the way the two programs keep them — an `otpauth://` link as
//! KeePassXC's `otp`, a bare key as KeePass's `TimeOtp-Secret-Base32` — and
//! both are read.

use super::xml::{self, Element};
use super::{
    Entry, ExportReport, Exported, Lineage, Outcome, Reading, Record, UNREADABLE_TOTP, gather,
    title_or, uuid_from_bytes, uuid_to_bytes,
};
use crate::db;
use crate::error::{Error, Result};
use crate::history::Version;
use crate::model::{ItemKind, NewItem, Payload};
use crate::otp::{self, Algorithm, Totp};
use crate::time;
use crate::vault::Vault;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use std::collections::HashMap;
use std::io::Read;
use zeroize::Zeroizing;

const FORMAT: &str = "a KeePass XML export";
const FORMAT_NAME: &str = "KeePass XML";

/// The custom-data key the sefy kind of an entry is kept under.
const KIND_KEY: &str = "sefy-kind";

/// The standard strings and the fields they stand for.
const STANDARD: [(&str, &str); 4] = [
    ("UserName", "login"),
    ("Password", "password"),
    ("URL", "url"),
    ("Notes", "notes"),
];

/// Where KeePassXC keeps a one-time password key: an `otpauth://` link.
const OTP_LINK: &str = "otp";

/// Where KeePass 2 keeps one, in parts; the secret under one of four names
/// depending on how it was typed in.
const TIME_OTP_BASE32: &str = "TimeOtp-Secret-Base32";
const TIME_OTP_HEX: &str = "TimeOtp-Secret-Hex";
const TIME_OTP_BASE64: &str = "TimeOtp-Secret-Base64";
const TIME_OTP_TEXT: &str = "TimeOtp-Secret";
const TIME_OTP_ALGORITHM: &str = "TimeOtp-Algorithm";
const TIME_OTP_LENGTH: &str = "TimeOtp-Length";
const TIME_OTP_PERIOD: &str = "TimeOtp-Period";
const TIME_OTP: [&str; 7] = [
    TIME_OTP_BASE32,
    TIME_OTP_HEX,
    TIME_OTP_BASE64,
    TIME_OTP_TEXT,
    TIME_OTP_ALGORITHM,
    TIME_OTP_LENGTH,
    TIME_OTP_PERIOD,
];

/// Why an attachment an entry names did not arrive.
const MISSING_ATTACHMENT: &str = "is not in the file: KeePassXC's XML export leaves \
     attachments out; save it with `keepassxc-cli attachment-export`";

/// Seconds between 0001-01-01, where KDBX 4 counts its binary times from, and
/// the Unix epoch.
const DOTNET_EPOCH_OFFSET: i64 = 62_135_596_800;

fn unreadable(reason: impl Into<String>) -> Error {
    Error::UnreadableImport {
        format: FORMAT,
        reason: reason.into(),
    }
}

/// One string of an entry.
struct Text {
    key: String,
    value: String,
    protected: bool,
}

/// What the whole file shares: attachments by id, and which group is the
/// recycle bin.
struct Context {
    binaries: HashMap<String, Vec<u8>>,
    recycle_bin: Option<String>,
}

/// Reads KeePass XML into entries.
pub(super) fn read(body: &str) -> Result<Reading> {
    let root = xml::parse(body).map_err(unreadable)?;
    if root.name != "KeePassFile" {
        return Err(unreadable("the document is not a KeePassFile"));
    }

    let meta = root.child("Meta");
    let mut binaries = HashMap::new();
    if let Some(pool) = meta.and_then(|meta| meta.child("Binaries")) {
        for binary in pool.children("Binary") {
            let id = binary
                .attribute("ID")
                .ok_or_else(|| unreadable("an attachment in the file has no ID"))?;
            binaries.insert(id.to_owned(), binary_bytes(binary)?);
        }
    }
    let recycle_bin = meta
        .filter(|meta| meta.text_of("RecycleBinEnabled").map(str::trim) != Some("False"))
        .and_then(|meta| meta.text_of("RecycleBinUUID"))
        .map(|uuid| uuid.trim().to_owned())
        .filter(|uuid| !uuid.is_empty() && *uuid != "AAAAAAAAAAAAAAAAAAAAAA==");
    let context = Context {
        binaries,
        recycle_bin,
    };

    let top = root
        .child("Root")
        .and_then(|root| root.child("Group"))
        .ok_or_else(|| unreadable("the file has no root group"))?;

    let mut reading = Reading::default();
    walk(top, &[], false, &context, &mut reading)?;
    Ok(reading)
}

/// Reads a group's entries, then its groups, depth first.
fn walk(
    group: &Element,
    path: &[String],
    in_bin: bool,
    context: &Context,
    reading: &mut Reading,
) -> Result<()> {
    for entry in group.children("Entry") {
        read_entry(entry, path, in_bin, context, reading)?;
    }
    for child in group.children("Group") {
        let name = child.text_of("Name").unwrap_or("").trim().to_owned();
        let bin = in_bin || context.recycle_bin.as_deref() == child.text_of("UUID").map(str::trim);
        let mut child_path = path.to_vec();
        child_path.push(name);
        walk(child, &child_path, bin, context, reading)?;
    }
    Ok(())
}

fn read_entry(
    entry: &Element,
    path: &[String],
    in_bin: bool,
    context: &Context,
    reading: &mut Reading,
) -> Result<()> {
    let texts = strings_of(entry)?;
    let title_text = value(&texts, "Title");
    let title = title_or(title_text, value(&texts, "URL"), value(&texts, "UserName"));
    if in_bin {
        reading.notice(&title, Outcome::NotImported, "in the recycle bin");
        return Ok(());
    }

    let uuid = entry
        .text_of("UUID")
        .and_then(|uuid| BASE64.decode(uuid.trim()).ok())
        .and_then(|bytes| uuid_from_bytes(&bytes));
    let times = entry.child("Times");
    let created_at = times
        .and_then(|times| times.text_of("CreationTime"))
        .and_then(parse_time);
    let updated_at = times
        .and_then(|times| times.text_of("LastModificationTime"))
        .and_then(parse_time);

    let mut tags: Vec<String> = entry
        .text_of("Tags")
        .unwrap_or("")
        .split([';', ','])
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty())
        .collect();
    let group: Vec<&str> = path
        .iter()
        .map(|name| name.as_str())
        .filter(|name| !name.is_empty())
        .collect();
    if !group.is_empty() {
        tags.insert(0, group.join("/"));
    }

    let marked = entry
        .child("CustomData")
        .into_iter()
        .flat_map(|data| data.children("Item"))
        .find(|item| item.text_of("Key") == Some(KIND_KEY))
        .and_then(|item| item.text_of("Value"))
        .map(ItemKind::parse)
        .filter(ItemKind::is_known);

    let mut attachments = Vec::new();
    let mut missing = Vec::new();
    for binary in entry.children("Binary") {
        let name = binary.text_of("Key").unwrap_or("").to_owned();
        let value = binary
            .child("Value")
            .ok_or_else(|| unreadable(format!("an attachment of {title:?} has no value")))?;
        match value.attribute("Ref") {
            Some(id) => match context.binaries.get(id) {
                Some(bytes) => attachments.push((name, bytes.clone())),
                // KeePassXC's XML export names an entry's attachments and
                // leaves their bytes out. The rest of the entry is still
                // whole, and the report says what stayed behind.
                None => missing.push(name),
            },
            None => attachments.push((name, binary_bytes(value)?)),
        }
    }

    let kind = marked.clone().unwrap_or_else(|| infer(&texts));
    let username = value(&texts, "UserName");

    // A file entry is its one attachment; anything else attached to it, or to
    // any other entry, becomes a file of its own beside it.
    let mut attachments = attachments.into_iter();
    let converted = if kind == ItemKind::File {
        attachments
            .next()
            .map(|(filename, bytes)| Converted::whole(Payload::File { filename, bytes }))
    } else if marked.is_none() && is_empty(&texts) && attachments.len() == 1 {
        attachments
            .next()
            .map(|(filename, bytes)| Converted::whole(Payload::File { filename, bytes }))
    } else {
        contents(&kind, &texts, title_text, username)
    };
    let attachments: Vec<(String, Vec<u8>)> = attachments.collect();

    let entry_uuid = uuid.clone();
    match converted {
        Some(converted) => {
            if !converted.totp_read {
                reading.notice(&title, Outcome::Reshaped, UNREADABLE_TOTP);
            }
            if converted.folded {
                reading.notice(
                    &title,
                    Outcome::Reshaped,
                    "a note with fields of its own; they are added to its text",
                );
            }
            for name in &missing {
                reading.notice(
                    &title,
                    Outcome::InPart,
                    format!("its attachment {name:?} {MISSING_ATTACHMENT}"),
                );
            }

            let mut earlier = Vec::new();
            if !matches!(converted.payload, Payload::File { .. })
                && let Some(history) = entry.child("History")
            {
                let current_kind = converted.payload.kind();
                for snapshot in history.children("Entry") {
                    let past_texts = strings_of(snapshot)?;
                    let made_at = snapshot
                        .child("Times")
                        .and_then(|times| times.text_of("LastModificationTime"))
                        .and_then(parse_time)
                        .unwrap_or(0);
                    if let Some(past) = contents(
                        &current_kind,
                        &past_texts,
                        value(&past_texts, "Title"),
                        value(&past_texts, "UserName"),
                    ) {
                        earlier.push((made_at, past.payload));
                    }
                }
                earlier.sort_by_key(|(made_at, _)| *made_at);
            }

            reading.push(Entry {
                uuid,
                item: NewItem::new(title.clone(), converted.payload).with_tags(tags.clone()),
                created_at,
                updated_at,
                lineage: Lineage::Earlier(earlier),
                notices: Vec::new(),
            });
        }
        None if attachments.is_empty() && missing.is_empty() => {
            reading.notice(&title, Outcome::NotImported, "nothing in it but a title");
        }
        None if attachments.is_empty() => {
            let names: Vec<String> = missing.iter().map(|name| format!("{name:?}")).collect();
            reading.notice(
                &title,
                Outcome::NotImported,
                format!("its attachment {} {MISSING_ATTACHMENT}", names.join(", ")),
            );
        }
        // The entry was its attachments and nothing else: they arrive below,
        // and the empty entry around them would add nothing.
        None => {}
    }

    for (filename, bytes) in attachments {
        reading.notice(
            &title,
            Outcome::Reshaped,
            format!("its attachment {filename:?} is a file item of its own"),
        );
        // Derived from the entry and the name, so importing the same file
        // again recognises the attachment as well as the entry.
        let uuid = entry_uuid
            .as_deref()
            .map(|uuid| db::derived_uuid(&format!("sefy: attachment {filename} of {uuid}")));
        let mut file = Entry::new(
            uuid,
            NewItem::new(
                format!("{title} - {filename}"),
                Payload::File { filename, bytes },
            )
            .with_tags(tags.clone()),
        );
        file.created_at = created_at;
        file.updated_at = updated_at;
        reading.push(file);
    }
    Ok(())
}

/// The strings of an entry.
fn strings_of(entry: &Element) -> Result<Vec<Text>> {
    let mut texts = Vec::new();
    for string in entry.children("String") {
        let key = string.text_of("Key").unwrap_or("").to_owned();
        let Some(value) = string.child("Value") else {
            continue;
        };
        if value.attribute("Protected").is_some_and(is_true) {
            return Err(Error::SealedImport {
                format: "KeePass XML export",
                advice: "its protected values are still sealed with the database's inner key; \
                         export it again from KeePass or KeePassXC as plain XML",
            });
        }
        texts.push(Text {
            key,
            value: value.text.clone(),
            protected: value.attribute("ProtectInMemory").is_some_and(is_true),
        });
    }
    Ok(texts)
}

fn value<'a>(texts: &'a [Text], key: &str) -> &'a str {
    texts
        .iter()
        .find(|text| text.key == key)
        .map_or("", |text| text.value.as_str())
}

fn is_true(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("true")
}

/// Whether an entry holds nothing but its title.
fn is_empty(texts: &[Text]) -> bool {
    texts
        .iter()
        .all(|text| text.key == "Title" || text.value.trim().is_empty())
}

/// The kind of an entry KeePass made: a note when notes are all it holds, a
/// login otherwise.
fn infer(texts: &[Text]) -> ItemKind {
    let only_notes = texts
        .iter()
        .filter(|text| !text.value.trim().is_empty())
        .all(|text| text.key == "Title" || text.key == "Notes");
    if only_notes && !value(texts, "Notes").trim().is_empty() {
        ItemKind::Note
    } else {
        ItemKind::Login
    }
}

/// An entry's strings, made into contents of `kind`.
struct Converted {
    payload: Payload,
    /// False when a one-time password key was kept under another name.
    totp_read: bool,
    /// True when a note's extra strings were folded into its text.
    folded: bool,
}

impl Converted {
    fn whole(payload: Payload) -> Self {
        Self {
            payload,
            totp_read: true,
            folded: false,
        }
    }
}

fn contents(kind: &ItemKind, texts: &[Text], title: &str, username: &str) -> Option<Converted> {
    if *kind == ItemKind::Note {
        let mut text = value(texts, "Notes").to_owned();
        let mut folded = false;
        for extra in texts.iter().filter(|text| {
            !matches!(text.key.as_str(), "Title" | "Notes") && !text.value.trim().is_empty()
        }) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&format!("{}: {}", extra.key, extra.value));
            folded = true;
        }
        return Some(Converted {
            payload: Payload::Note { text },
            totp_read: true,
            folded,
        });
    }

    let mut record = Record::new(kind.clone());
    for (key, field) in STANDARD {
        let text = texts.iter().find(|text| text.key == key);
        if let Some(text) = text {
            record.put(field, &text.value, Some(text.protected));
        }
    }

    let link = value(texts, OTP_LINK);
    let mut totp_read = true;
    let mut consumed: Vec<&str> = Vec::new();
    if !link.trim().is_empty() {
        totp_read = record.put_totp(link);
        consumed.push(OTP_LINK);
    } else if let Some(stored) = time_otp(texts, title, username) {
        match stored {
            Some(stored) => {
                record.put(otp::FIELD, &stored, Some(true));
                consumed.extend(TIME_OTP);
            }
            // A key in KeePass's parts that sefy cannot make codes from: the
            // parts stay as they are, below, and the notice says so.
            None => totp_read = false,
        }
    }

    for text in texts {
        let key = text.key.as_str();
        if key == "Title" || STANDARD.iter().any(|(standard, _)| *standard == key) {
            continue;
        }
        if consumed.contains(&key) {
            continue;
        }
        record.put(key, &text.value, Some(text.protected));
    }

    record.into_payload().map(|payload| Converted {
        payload,
        totp_read,
        folded: false,
    })
}

/// A key kept in KeePass 2's parts, as sefy stores one.
///
/// `None` when the entry has no such key; `Some(None)` when it has one sefy
/// cannot read.
fn time_otp(texts: &[Text], title: &str, username: &str) -> Option<Option<String>> {
    let secret = [
        TIME_OTP_BASE32,
        TIME_OTP_HEX,
        TIME_OTP_BASE64,
        TIME_OTP_TEXT,
    ]
    .into_iter()
    .find_map(|key| {
        let value = value(texts, key).trim();
        (!value.is_empty()).then_some((key, value))
    })?;

    let key: Option<Zeroizing<Vec<u8>>> = match secret {
        (TIME_OTP_BASE32, value) => otp::decode_base32(value).ok(),
        (TIME_OTP_HEX, value) => decode_hex(value).map(Zeroizing::new),
        (TIME_OTP_BASE64, value) => BASE64.decode(value).ok().map(Zeroizing::new),
        (_, value) => Some(Zeroizing::new(value.as_bytes().to_vec())),
    };
    let algorithm = match value(texts, TIME_OTP_ALGORITHM).trim() {
        "" | "HMAC-SHA-1" => Some(Algorithm::Sha1),
        "HMAC-SHA-256" => Some(Algorithm::Sha256),
        "HMAC-SHA-512" => Some(Algorithm::Sha512),
        _ => None,
    };
    let digits = match value(texts, TIME_OTP_LENGTH).trim() {
        "" => Some(6),
        digits => digits.parse().ok(),
    };
    let period = match value(texts, TIME_OTP_PERIOD).trim() {
        "" => Some(30),
        period => period.parse().ok(),
    };

    let (Some(key), Some(algorithm), Some(digits), Some(period)) = (key, algorithm, digits, period)
    else {
        return Some(None);
    };
    let username = (!username.trim().is_empty()).then_some(username);
    Some(
        Totp::from_parts(&key, algorithm, digits, period)
            .ok()
            .map(|totp| totp.stored(title, username)),
    )
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}

/// The bytes of an attachment, decompressed when the file says it is.
fn binary_bytes(element: &Element) -> Result<Vec<u8>> {
    let encoded: String = element
        .text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| unreadable("an attachment is not valid base64"))?;
    if element.attribute("Compressed").is_some_and(is_true) {
        let mut inflated = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .read_to_end(&mut inflated)
            .map_err(|_| unreadable("a compressed attachment does not decompress"))?;
        Ok(inflated)
    } else {
        Ok(bytes)
    }
}

/// A KeePass time: text in an XML export, base64 seconds since year one in a
/// database's own XML.
fn parse_time(text: &str) -> Option<i64> {
    let text = text.trim();
    time::parse_rfc3339(text).or_else(|| {
        let bytes: [u8; 8] = BASE64.decode(text).ok()?.try_into().ok()?;
        Some(i64::from_le_bytes(bytes) - DOTNET_EPOCH_OFFSET)
    })
}

/// Writes a vault as KeePass XML.
pub(super) fn write(vault: &Vault, history: bool) -> Result<Exported> {
    let mut report = ExportReport::default();
    let mut pool: Vec<Vec<u8>> = Vec::new();
    let mut entries = String::new();

    for outgoing in gather(vault, history)? {
        if matches!(outgoing.payload, Payload::Unknown { .. }) {
            report.unreadable += 1;
            continue;
        }
        let summary = &outgoing.summary;
        entries.push_str(&entry(
            Snapshot {
                uuid: &summary.uuid,
                title: &summary.title,
                tags: &summary.tags,
                created_at: summary.created_at,
                updated_at: summary.updated_at,
                payload: &outgoing.payload,
            },
            &outgoing.past,
            &mut pool,
            3,
        )?);
        report.written += 1;
        report.versions += outgoing.past.len();
    }

    let group_uuid = BASE64.encode(
        uuid_to_bytes(&db::derived_uuid(
            "sefy: the group a KeePass export is written into",
        ))
        .expect("a derived identity is a UUID"),
    );
    let mut document = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n<KeePassFile>\n\
         \t<Meta>\n\t\t<Generator>sefy</Generator>\n\t\t<DatabaseName>sefy</DatabaseName>\n",
    );
    if !pool.is_empty() {
        document.push_str("\t\t<Binaries>\n");
        for (id, bytes) in pool.iter().enumerate() {
            document.push_str(&format!(
                "\t\t\t<Binary ID=\"{id}\" Compressed=\"False\">{}</Binary>\n",
                BASE64.encode(bytes)
            ));
        }
        document.push_str("\t\t</Binaries>\n");
    }
    document.push_str(&format!(
        "\t</Meta>\n\t<Root>\n\t\t<Group>\n\t\t\t<UUID>{group_uuid}</UUID>\n\
         \t\t\t<Name>sefy</Name>\n\t\t\t<IconID>48</IconID>\n\t\t\t<IsExpanded>True</IsExpanded>\n"
    ));
    document.push_str(&entries);
    document.push_str("\t\t</Group>\n\t\t<DeletedObjects />\n\t</Root>\n</KeePassFile>\n");

    Ok(Exported {
        text: Zeroizing::new(document),
        report,
    })
}

/// What one entry, or one snapshot in its history, is written from.
struct Snapshot<'a> {
    uuid: &'a str,
    title: &'a str,
    tags: &'a [String],
    created_at: i64,
    updated_at: i64,
    payload: &'a Payload,
}

/// One `<Entry>`, its history inside it.
fn entry(
    snapshot: Snapshot<'_>,
    past: &[Version],
    pool: &mut Vec<Vec<u8>>,
    depth: usize,
) -> Result<String> {
    let indent = "\t".repeat(depth);
    let escape = |text: &str| {
        xml::escape(text).map_err(|character| Error::Unexportable {
            title: snapshot.title.to_owned(),
            format: FORMAT_NAME,
            reason: format!(
                "it holds the control character U+{:04X}, which XML cannot carry",
                u32::from(character)
            ),
        })
    };

    // An identity hand-written into an earlier export need not be a UUID;
    // KeePass needs one, and the same text always gives the same one.
    let uuid = uuid_to_bytes(snapshot.uuid)
        .or_else(|| uuid_to_bytes(&db::derived_uuid(snapshot.uuid)))
        .expect("a derived identity is a UUID");

    let mut out = format!(
        "{indent}<Entry>\n{indent}\t<UUID>{}</UUID>\n{indent}\t<IconID>0</IconID>\n",
        BASE64.encode(uuid)
    );
    if !snapshot.tags.is_empty() {
        out.push_str(&format!(
            "{indent}\t<Tags>{}</Tags>\n",
            escape(&snapshot.tags.join(";"))?
        ));
    }
    let created = time::format_rfc3339(snapshot.created_at);
    let updated = time::format_rfc3339(snapshot.updated_at);
    out.push_str(&format!(
        "{indent}\t<Times>\n\
         {indent}\t\t<CreationTime>{created}</CreationTime>\n\
         {indent}\t\t<LastModificationTime>{updated}</LastModificationTime>\n\
         {indent}\t\t<LastAccessTime>{updated}</LastAccessTime>\n\
         {indent}\t\t<ExpiryTime>{updated}</ExpiryTime>\n\
         {indent}\t\t<Expires>False</Expires>\n\
         {indent}\t\t<UsageCount>0</UsageCount>\n\
         {indent}\t\t<LocationChanged>{updated}</LocationChanged>\n\
         {indent}\t</Times>\n"
    ));

    for (key, value, protect) in strings(snapshot.title, snapshot.payload) {
        let attribute = if protect {
            " ProtectInMemory=\"True\""
        } else {
            ""
        };
        out.push_str(&format!(
            "{indent}\t<String>\n{indent}\t\t<Key>{}</Key>\n{indent}\t\t<Value{attribute}>{}</Value>\n{indent}\t</String>\n",
            escape(&key)?,
            escape(&value)?
        ));
    }

    if let Payload::File { filename, bytes } = snapshot.payload {
        out.push_str(&format!(
            "{indent}\t<Binary>\n{indent}\t\t<Key>{}</Key>\n{indent}\t\t<Value Ref=\"{}\" />\n{indent}\t</Binary>\n",
            escape(filename)?,
            pool.len()
        ));
        pool.push(bytes.clone());
    }

    out.push_str(&format!(
        "{indent}\t<CustomData>\n{indent}\t\t<Item>\n{indent}\t\t\t<Key>{KIND_KEY}</Key>\n\
         {indent}\t\t\t<Value>{}</Value>\n{indent}\t\t</Item>\n{indent}\t</CustomData>\n",
        escape(snapshot.payload.kind().as_str())?
    ));

    if !past.is_empty() {
        out.push_str(&format!("{indent}\t<History>\n"));
        for version in past {
            out.push_str(&entry(
                Snapshot {
                    uuid: snapshot.uuid,
                    title: snapshot.title,
                    tags: snapshot.tags,
                    created_at: snapshot.created_at,
                    updated_at: version.made_at,
                    payload: &version.payload,
                },
                &[],
                pool,
                depth + 2,
            )?);
        }
        out.push_str(&format!("{indent}\t</History>\n"));
    }
    out.push_str(&format!("{indent}</Entry>\n"));
    Ok(out)
}

/// The strings an entry is written with: the five standard ones always, as
/// KeePass writes them, then a string for every other field.
fn strings(title: &str, payload: &Payload) -> Vec<(String, String, bool)> {
    let mut standard: Vec<(String, String, bool)> = vec![
        ("Title".to_owned(), title.to_owned(), false),
        ("UserName".to_owned(), String::new(), false),
        ("Password".to_owned(), String::new(), true),
        ("URL".to_owned(), String::new(), false),
        ("Notes".to_owned(), String::new(), false),
    ];
    let mut others: Vec<(String, String, bool)> = Vec::new();

    match payload {
        Payload::Note { text } => standard[4].1 = text.clone(),
        Payload::Fields { fields, .. } => {
            for field in fields {
                let place = STANDARD
                    .iter()
                    .find(|(_, name)| *name == field.name)
                    .map(|(key, _)| *key);
                if let Some(key) = place
                    && let Some(slot) = standard.iter_mut().find(|(name, _, _)| name == key)
                {
                    slot.1 = field.value.clone();
                    slot.2 = slot.2 || field.secret;
                    continue;
                }
                let key = if field.name == otp::FIELD {
                    if otp::is_link(&field.value) {
                        OTP_LINK
                    } else {
                        TIME_OTP_BASE32
                    }
                } else {
                    field.name.as_str()
                };
                others.push((key.to_owned(), field.value.clone(), field.secret));
            }
        }
        Payload::File { .. } | Payload::Unknown { .. } => {}
    }

    // Two strings of an entry cannot share a key; a field that happens to be
    // called `Title` or `otp` gets a number rather than overwriting.
    for (key, value, protect) in others {
        let mut unique = key.clone();
        let mut counter = 2;
        while standard.iter().any(|(taken, _, _)| *taken == unique) {
            unique = format!("{key} {counter}");
            counter += 1;
        }
        standard.push((unique, value, protect));
    }
    standard
}
