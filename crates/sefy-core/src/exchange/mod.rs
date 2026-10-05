//! Moving a vault's contents in and out: sefy's own JSON, and the files other
//! password managers and browsers read and write.
//!
//! This is the one place in sefy that deliberately handles plaintext in bulk.
//! It exists so a vault is never a trap, in either direction: what another
//! tool holds can come in, and what sefy holds can leave for another tool. An
//! export is exactly as sensitive as the vault it came from, and so is the file
//! an import reads; nothing here pretends otherwise, and callers are expected
//! to say so to the user.
//!
//! Every format is read into the same shape before anything touches the vault:
//! a list of entries, each a [`NewItem`] with the identity, times and earlier
//! contents the source knew, and a list of notices about what could not come
//! across as it was. The whole file is read and checked first, so a broken
//! entry halfway down cannot leave a half-imported vault behind.

mod bitwarden;
mod csv;
mod keepass;
mod sefy;
mod xml;

pub use sefy::{EXPORT_VERSION, Export, ExportField, ExportItem, ExportStamp, ExportVersion};

use crate::db::{self, Stamp};
use crate::error::{Error, Result};
use crate::history::Version;
use crate::model::{Field, ItemKind, ItemSummary, NewItem, Payload};
use crate::otp;
use crate::vault::Vault;
use zeroize::Zeroizing;

/// A kind of file [`import`] reads, as told by what is in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// sefy's own JSON, written by [`export`].
    Sefy,
    /// The XML KeePass 2 and KeePassXC write when exporting a database.
    KeePass,
    /// The unencrypted JSON Bitwarden writes when exporting a vault.
    Bitwarden,
    /// Rows of passwords under a header, as browsers and password managers
    /// write them.
    Csv,
}

impl Format {
    /// What the file is, in a phrase that fits "imported 3 items from …".
    pub fn describe(self) -> &'static str {
        match self {
            Self::Sefy => "a sefy export",
            Self::KeePass => "a KeePass XML export",
            Self::Bitwarden => "a Bitwarden JSON export",
            Self::Csv => "a CSV of passwords",
        }
    }
}

/// What [`export`] writes.
///
/// Earlier versions travel only where the format has a place for them, and
/// only when asked: a file of every password an account has ever had, in the
/// clear, is a larger exposure than moving the current ones calls for. A CSV
/// has no place for them, so it has no way to ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// sefy's own JSON: every kind, every field, nothing lost.
    Sefy {
        /// Whether each item's earlier versions go along.
        history: bool,
    },
    /// KeePass XML, which KeePass 2 and KeePassXC import.
    KeePass {
        /// Whether each item's earlier versions go along, as the entry's
        /// history.
        history: bool,
    },
    /// Logins as rows, the CSV browsers import.
    Csv,
}

impl Target {
    /// The format's name, as in "cannot be exported as KeePass XML".
    pub fn name(self) -> &'static str {
        match self {
            Self::Sefy { .. } => "a sefy export",
            Self::KeePass { .. } => "KeePass XML",
            Self::Csv => "CSV",
        }
    }
}

/// Tells which format a file is in, from its contents alone.
///
/// The file says unambiguously what it is: sefy's JSON names itself, Bitwarden's
/// carries `items` beside `encrypted` or `folders`, KeePass's XML has a
/// `KeePassFile` root, and anything else is taken for a CSV and has to prove it
/// with a header naming a password, a login or an address.
pub fn detect(text: &str) -> Result<Format> {
    let body = body(text);
    match body.as_bytes().first() {
        Some(b'{') => {
            let value: serde_json::Value =
                serde_json::from_str(body).map_err(|error| Error::UnreadableImport {
                    format: "a JSON file",
                    reason: error.to_string(),
                })?;
            let object = value.as_object().ok_or(Error::UnrecognizedImport)?;
            if object.contains_key("sefy_export") {
                Ok(Format::Sefy)
            } else if object.contains_key("encKeyValidation_DO_NOT_EDIT")
                || (object.contains_key("items")
                    && ["encrypted", "folders", "collections"]
                        .iter()
                        .any(|key| object.contains_key(*key)))
            {
                Ok(Format::Bitwarden)
            } else {
                Err(Error::UnrecognizedImport)
            }
        }
        Some(b'<') if body.contains("<KeePassFile") => Ok(Format::KeePass),
        Some(b'<') | None => Err(Error::UnrecognizedImport),
        Some(_) => Ok(Format::Csv),
    }
}

/// Adds what a file holds to a vault, whichever format it is in.
///
/// An entry whose identity the vault already holds is **skipped**, not merged
/// and not duplicated: importing the same file twice leaves the vault as it was
/// after the first time. Bringing newer contents across is what
/// [`crate::merge`] is for — an import is not the place to overwrite a secret,
/// because the file may well be older than what is here.
///
/// Identity comes from the source where the source has one — sefy's own, a
/// KeePass entry's UUID, a Bitwarden item's id, Firefox's guid. An entry
/// without one is always added: there is nothing to recognise it by, and
/// guessing from titles would silently collapse two accounts that share a name.
///
/// Earlier contents the source kept come in as the item's history, so a
/// password changed last spring is one `restore` away here too.
pub fn import(vault: &mut Vault, text: &str) -> Result<ImportReport> {
    let format = detect(text)?;
    let body = body(text);
    let reading = match format {
        Format::Sefy => sefy::read(body)?,
        Format::KeePass => keepass::read(body)?,
        Format::Bitwarden => bitwarden::read(body)?,
        Format::Csv => csv::read(body)?,
    };
    apply(vault, format, reading)
}

/// Writes a vault's contents out in the chosen format.
pub fn export(vault: &Vault, target: Target) -> Result<Exported> {
    match target {
        Target::Sefy { history } => sefy::write(vault, history),
        Target::KeePass { history } => keepass::write(vault, history),
        Target::Csv => csv::write(vault),
    }
}

/// The text of an export, and what went into it.
pub struct Exported {
    /// The file's contents, every secret in them in the clear.
    pub text: Zeroizing<String>,
    /// What was written and what was not.
    pub report: ExportReport,
}

/// What an export wrote, and what the format had no place for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// Items written.
    pub written: usize,
    /// Earlier versions written along with them.
    pub versions: usize,
    /// Items of each kind the format has no place for, left out.
    pub left_out: Vec<(ItemKind, usize)>,
    /// Items written without some of their fields, because the format has no
    /// column for them.
    pub trimmed: usize,
    /// Items of a kind this build does not know: written without their
    /// contents in sefy's own format, left out of any other.
    pub unreadable: usize,
}

/// What an import did, entry by entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    /// What the file was.
    pub format: Format,
    /// Items added, by kind, in the order kinds are listed everywhere else.
    pub added: Vec<(ItemKind, usize)>,
    /// Entries whose identity the vault already held, left untouched.
    pub skipped: usize,
    /// Earlier versions that came in with the added items.
    pub versions: usize,
    /// Entries that did not come across as they were: left behind, brought
    /// in part, or brought in another shape. Each names its entry and why.
    pub notices: Vec<Notice>,
    /// Columns of a CSV that were recognised as a browser's own bookkeeping
    /// and not imported.
    pub columns_left_out: Vec<String>,
}

impl ImportReport {
    /// How many items were added, of every kind.
    pub fn added_total(&self) -> usize {
        self.added.iter().map(|(_, count)| count).sum()
    }

    /// How many entries were not imported at all.
    pub fn not_imported(&self) -> usize {
        self.notices
            .iter()
            .filter(|notice| notice.outcome == Outcome::NotImported)
            .count()
    }

    fn count(&mut self, kind: ItemKind) {
        match self.added.iter_mut().find(|(known, _)| *known == kind) {
            Some((_, count)) => *count += 1,
            None => self.added.push((kind, 1)),
        }
    }
}

/// One entry that did not come across exactly as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// The entry's title, as it would be or is in the vault.
    pub title: String,
    /// What became of it.
    pub outcome: Outcome,
    /// Why, in a phrase that follows the title.
    pub reason: String,
}

/// What became of an entry a [`Notice`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Left behind entirely: in a trash, of a kind this build cannot store, or
    /// with nothing in it.
    NotImported,
    /// Imported, without something it carried that cannot move to sefy.
    InPart,
    /// Imported whole, in a shape other than the one it had: an identity as a
    /// note, an attachment as a file of its own.
    Reshaped,
}

/// A file read into entries, before any of it reaches the vault.
#[derive(Default)]
pub(crate) struct Reading {
    pub entries: Vec<Entry>,
    /// Entries left behind, reported whatever the vault holds.
    pub notices: Vec<Notice>,
    pub columns_left_out: Vec<String>,
    /// Notices about the entry being read, until it is pushed.
    pending: Vec<Notice>,
}

impl Reading {
    /// Says something about the entry being read.
    ///
    /// An entry left behind is reported as it is. Anything else belongs to
    /// the entry the reader pushes next, and is reported only if that entry
    /// lands: on a second import of the same file, "its passkey stays behind"
    /// about an item skipped as already here would describe nothing that
    /// happened.
    pub fn notice(&mut self, title: &str, outcome: Outcome, reason: impl Into<String>) {
        let notice = Notice {
            title: title.to_owned(),
            outcome,
            reason: reason.into(),
        };
        if outcome == Outcome::NotImported {
            // Whatever was said about it on the way is moot now.
            self.pending.clear();
            self.notices.push(notice);
        } else {
            self.pending.push(notice);
        }
    }

    /// Adds an entry, with what was said about it while it was read.
    pub fn push(&mut self, mut entry: Entry) {
        entry.notices.append(&mut self.pending);
        self.entries.push(entry);
    }
}

/// One item on its way in, with what its source knew about it.
pub(crate) struct Entry {
    /// Identity in the source, when it had one.
    pub uuid: Option<String>,
    pub item: NewItem,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub lineage: Lineage,
    /// What did not come across as it was; reported if the entry lands.
    pub notices: Vec<Notice>,
}

impl Entry {
    /// An entry with no times and no earlier contents.
    pub fn new(uuid: Option<String>, item: NewItem) -> Self {
        Self {
            uuid,
            item,
            created_at: None,
            updated_at: None,
            lineage: Lineage::Earlier(Vec::new()),
            notices: Vec::new(),
        }
    }
}

/// How an entry's earlier contents are known.
pub(crate) enum Lineage {
    /// As another sefy recorded them: each version with its identity, its
    /// place in the line and its machine, and the current one's stamp.
    Recorded {
        stamp: Option<Stamp>,
        past: Vec<Version>,
    },
    /// As another tool kept them: contents and when they were written, oldest
    /// first. Identities and places are given here.
    Earlier(Vec<(i64, Payload)>),
}

fn apply(vault: &mut Vault, format: Format, reading: Reading) -> Result<ImportReport> {
    let mut report = ImportReport {
        format,
        added: Vec::new(),
        skipped: 0,
        versions: 0,
        notices: reading.notices,
        columns_left_out: reading.columns_left_out,
    };

    let now = crate::vault::now();
    for entry in reading.entries {
        let uuid = match entry.uuid {
            Some(uuid) => uuid,
            None => db::new_uuid()?,
        };
        if vault.find_by_uuid(&uuid)?.is_some() {
            report.skipped += 1;
            continue;
        }
        report.notices.extend(entry.notices);

        let created_at = entry.created_at.unwrap_or(now);
        let updated_at = entry.updated_at.unwrap_or(created_at);
        let kind = entry.item.payload.kind();
        let (stamp, past) = settle(entry.lineage, &entry.item.payload, updated_at)?;
        let (_, kept) =
            vault.add_travelled(entry.item, &uuid, (created_at, updated_at), &stamp, &past)?;
        report.count(kind);
        report.versions += kept;
    }

    let known = ItemKind::known();
    report.added.sort_by_key(|(kind, _)| {
        known
            .iter()
            .position(|candidate| candidate == kind)
            .unwrap_or(known.len())
    });
    vault.save()?;
    Ok(report)
}

/// The stamp an entry's current contents get, and its earlier versions.
fn settle(lineage: Lineage, current: &Payload, updated_at: i64) -> Result<(Stamp, Vec<Version>)> {
    match lineage {
        Lineage::Recorded { stamp, past } => {
            let stamp = match stamp {
                Some(stamp) => stamp,
                None => {
                    let after = past.iter().map(|version| version.seq).max().unwrap_or(0);
                    Stamp::new(after + 1, updated_at, None)?
                }
            };
            Ok((stamp, past))
        }
        Lineage::Earlier(earlier) => {
            // A snapshot identical to the one before it is no version: KeePass
            // keeps one for a renamed title, Bitwarden for a password saved
            // unchanged, and neither changed the contents. The first of a run
            // is kept, because its time is when those contents were written;
            // a run that reaches the current contents is the current version.
            let mut kept: Vec<(i64, Payload)> = Vec::new();
            for (made_at, payload) in earlier {
                if kept.last().is_none_or(|(_, last)| *last != payload) {
                    kept.push((made_at, payload));
                }
            }
            if kept.last().is_some_and(|(_, last)| last == current) {
                kept.pop();
            }

            let mut past = Vec::with_capacity(kept.len());
            for (position, (made_at, payload)) in kept.into_iter().enumerate() {
                past.push(Version {
                    uuid: db::new_uuid()?,
                    seq: position as i64 + 1,
                    made_at,
                    device: None,
                    conflict: false,
                    current: false,
                    payload,
                });
            }
            let stamp = Stamp::new(past.len() as i64 + 1, updated_at, None)?;
            Ok((stamp, past))
        }
    }
}

/// One item on its way out, with what the formats may want of it.
pub(crate) struct Outgoing {
    pub summary: ItemSummary,
    pub payload: Payload,
    pub stamp: Stamp,
    /// Earlier versions, oldest first; empty unless they were asked for.
    pub past: Vec<Version>,
}

/// Every item in a vault, in listing order, ready for a writer.
pub(crate) fn gather(vault: &Vault, history: bool) -> Result<Vec<Outgoing>> {
    let mut items = Vec::new();
    for summary in vault.list()? {
        let item = vault.get(summary.id)?;
        let past = if history && !matches!(item.payload, Payload::Unknown { .. }) {
            vault.past_versions(summary.id)?
        } else {
            Vec::new()
        };
        items.push(Outgoing {
            stamp: vault.stamp(summary.id)?,
            summary: item.summary,
            payload: item.payload,
            past,
        });
    }
    Ok(items)
}

/// Counts an item of `kind` that a format had no place for.
pub(crate) fn leave_out(report: &mut ExportReport, kind: &ItemKind) {
    match report.left_out.iter_mut().find(|(known, _)| known == kind) {
        Some((_, count)) => *count += 1,
        None => report.left_out.push((kind.clone(), 1)),
    }
}

/// A record being assembled from another tool's fields.
///
/// Every source names its fields its own way and says about secrecy what it
/// happens to know. This is where that becomes a sefy record: a field the
/// kind's template knows takes the template's secrecy, any other the source's
/// word for it, and a field nobody said anything about is taken as secret —
/// for a value of unknown meaning, hiding it is the mistake that can be undone.
pub(crate) struct Record {
    kind: ItemKind,
    fields: Vec<Field>,
}

impl Record {
    pub fn new(kind: ItemKind) -> Self {
        Self {
            kind,
            fields: Vec::new(),
        }
    }

    /// Adds a field, unless its value is empty.
    ///
    /// A name already taken gets a number — `url-2` — rather than replacing
    /// what is there: two values under one name are two values.
    pub fn put(&mut self, name: &str, value: &str, secret: Option<bool>) {
        if value.trim().is_empty() {
            return;
        }
        let name = match name.trim() {
            "" => "field",
            name => name,
        };
        let secret = self
            .kind
            .template()
            .and_then(|template| template.field(name))
            .map(|spec| spec.secret)
            .or(secret)
            .unwrap_or(true);

        let mut unique = name.to_owned();
        let mut counter = 2;
        while self.has(&unique) {
            unique = format!("{name}-{counter}");
            counter += 1;
        }
        self.fields.push(Field {
            name: unique,
            value: value.to_owned(),
            secret,
        });
    }

    /// Adds a one-time password key, normalized the way `sefy otp --set` would
    /// store it.
    ///
    /// A key sefy cannot read — a Steam key, a counter-based one — is kept all
    /// the same, as a secret field named `otp`, so nothing is lost; the return
    /// value says which happened.
    pub fn put_totp(&mut self, value: &str) -> bool {
        if value.trim().is_empty() {
            return true;
        }
        match otp::normalize(value) {
            Ok(key) => {
                self.put(otp::FIELD, &key, Some(true));
                true
            }
            Err(_) => {
                self.put("otp", value.trim(), Some(true));
                false
            }
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.fields.iter().any(|field| field.name == name)
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// The record as a payload, its fields in the template's order and any
    /// others after them in the order they came; `None` when it holds nothing.
    pub fn into_payload(self) -> Option<Payload> {
        if self.fields.is_empty() {
            return None;
        }
        let template = self.kind.template();
        let mut fields = self.fields;
        fields.sort_by_key(|field| {
            template
                .and_then(|template| {
                    template
                        .fields
                        .iter()
                        .position(|spec| spec.name == field.name)
                })
                .unwrap_or(usize::MAX)
        });
        Some(Payload::Fields {
            kind: self.kind,
            fields,
        })
    }
}

/// Why an unreadable one-time password key is kept under another name.
pub(crate) const UNREADABLE_TOTP: &str =
    "its one-time password key is not one sefy can read, kept as the field \"otp\"";

/// A title for an entry that came without one: the site it is for, else the
/// account, else a placeholder that says so.
pub(crate) fn title_or(title: &str, url: &str, login: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_owned();
    }
    let host = host_of(url);
    if !host.is_empty() {
        return host.to_owned();
    }
    match login.trim() {
        "" => "untitled".to_owned(),
        login => login.to_owned(),
    }
}

/// The host part of an address: `accounts.example.com` out of
/// `https://www.accounts.example.com:443/sign-in?next=/`.
fn host_of(url: &str) -> &str {
    let url = url.trim();
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    host.strip_prefix("www.").unwrap_or(host)
}

/// The text past a byte-order mark and leading blank space.
fn body(text: &str) -> &str {
    text.trim_start_matches('\u{feff}').trim_start()
}

/// The hyphenated form of a sixteen-byte identity.
pub(crate) fn uuid_from_bytes(bytes: &[u8]) -> Option<String> {
    let bytes: &[u8; 16] = bytes.try_into().ok()?;
    Some(db::hyphenated(bytes))
}

/// The sixteen bytes behind a hyphenated identity, or `None` for one that is
/// not a UUID — an identity hand-written into an export may be anything.
pub(crate) fn uuid_to_bytes(uuid: &str) -> Option<[u8; 16]> {
    let hex: String = uuid.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

/// An identity from a source that writes UUIDs its own way — braced,
/// upper-case — in the form sefy keeps, or `None` when it is not one.
pub(crate) fn normalize_uuid(text: &str) -> Option<String> {
    let text = text.trim().trim_start_matches('{').trim_end_matches('}');
    uuid_to_bytes(text).map(|bytes| db::hyphenated(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_format_is_told_by_its_contents() {
        assert_eq!(
            detect(r#"{"sefy_export":1,"items":[]}"#).unwrap(),
            Format::Sefy
        );
        assert_eq!(
            detect(r#"{"encrypted":false,"folders":[],"items":[]}"#).unwrap(),
            Format::Bitwarden
        );
        assert_eq!(
            detect(r#"{"encrypted":false,"collections":[],"items":[]}"#).unwrap(),
            Format::Bitwarden
        );
        assert_eq!(
            detect("\u{feff}<?xml version=\"1.0\"?>\n<KeePassFile><Root/></KeePassFile>").unwrap(),
            Format::KeePass
        );
        assert_eq!(detect("name,url,username,password\n").unwrap(), Format::Csv);
    }

    #[test]
    fn json_and_xml_that_are_neither_are_refused_by_name() {
        assert!(matches!(
            detect(r#"{"items": []}"#),
            Err(Error::UnrecognizedImport)
        ));
        assert!(matches!(detect("[1, 2]"), Ok(Format::Csv)));
        assert!(matches!(
            detect("<html></html>"),
            Err(Error::UnrecognizedImport)
        ));
        assert!(matches!(detect("   "), Err(Error::UnrecognizedImport)));
        assert!(matches!(
            detect("{ not json"),
            Err(Error::UnreadableImport { .. })
        ));
    }

    #[test]
    fn a_field_name_already_taken_gets_a_number_instead_of_replacing() {
        let mut record = Record::new(ItemKind::Login);
        record.put("url", "https://one.example", None);
        record.put("url", "https://two.example", None);
        record.put("url", "https://three.example", None);
        let payload = record.into_payload().unwrap();
        assert_eq!(payload.field("url").unwrap().value, "https://one.example");
        assert_eq!(payload.field("url-2").unwrap().value, "https://two.example");
        assert_eq!(
            payload.field("url-3").unwrap().value,
            "https://three.example"
        );
    }

    #[test]
    fn secrecy_comes_from_the_template_then_the_source_then_caution() {
        let mut record = Record::new(ItemKind::Login);
        // The template says a login's password is secret, whatever the
        // source claimed.
        record.put("password", "p", Some(false));
        record.put("login", "ada", Some(true));
        record.put("pin hint", "birthday", Some(false));
        record.put("mystery", "?", None);
        let payload = record.into_payload().unwrap();
        assert!(payload.field("password").unwrap().secret);
        assert!(!payload.field("login").unwrap().secret);
        assert!(!payload.field("pin hint").unwrap().secret);
        assert!(payload.field("mystery").unwrap().secret);
    }

    #[test]
    fn fields_settle_in_the_templates_order_with_strangers_after() {
        let mut record = Record::new(ItemKind::Login);
        record.put("notes", "n", None);
        record.put("extra", "e", None);
        record.put("password", "p", None);
        record.put("login", "l", None);
        let Payload::Fields { fields, .. } = record.into_payload().unwrap() else {
            unreachable!()
        };
        let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
        assert_eq!(names, ["login", "password", "notes", "extra"]);
    }

    #[test]
    fn an_empty_value_is_no_field_and_an_empty_record_is_no_payload() {
        let mut record = Record::new(ItemKind::Login);
        record.put("login", "", None);
        record.put("password", "   ", None);
        assert!(record.into_payload().is_none());
    }

    #[test]
    fn a_title_falls_back_to_the_site_then_the_account() {
        assert_eq!(title_or(" mail ", "https://x.example", "ada"), "mail");
        assert_eq!(
            title_or("", "https://www.accounts.example.com:443/in?next=/", "ada"),
            "accounts.example.com"
        );
        assert_eq!(
            title_or("", "ftp://user@files.example/", ""),
            "files.example"
        );
        assert_eq!(title_or("", "", "ada"), "ada");
        assert_eq!(title_or("", "", ""), "untitled");
    }

    #[test]
    fn identities_written_other_ways_come_out_the_same() {
        let canonical = "3f2b8c1e-9a4d-4e6f-8b2a-1c0d9e8f7a6b";
        assert_eq!(normalize_uuid(canonical).unwrap(), canonical);
        assert_eq!(
            normalize_uuid("{3F2B8C1E-9A4D-4E6F-8B2A-1C0D9E8F7A6B}").unwrap(),
            canonical
        );
        assert_eq!(
            uuid_from_bytes(&uuid_to_bytes(canonical).unwrap()).unwrap(),
            canonical
        );
        assert!(normalize_uuid("not-a-uuid").is_none());
        assert!(uuid_to_bytes("3f2b8c1e").is_none());
    }

    fn text(value: &str) -> Payload {
        Payload::Note {
            text: value.to_owned(),
        }
    }

    #[test]
    fn earlier_contents_settle_into_a_numbered_line_without_repeats() {
        // A title change makes KeePass keep a second snapshot of the same
        // contents; the last snapshot can equal what is current. Neither is a
        // version, and the first of a run keeps its time.
        let earlier = vec![
            (10, text("a")),
            (20, text("a")),
            (30, text("b")),
            (40, text("a")),
            (50, text("c")),
            (60, text("c")),
        ];
        let (stamp, past) = settle(Lineage::Earlier(earlier), &text("c"), 70).unwrap();
        let line: Vec<(i64, i64, Payload)> = past
            .iter()
            .map(|version| (version.seq, version.made_at, version.payload.clone()))
            .collect();
        assert_eq!(
            line,
            [(1, 10, text("a")), (2, 30, text("b")), (3, 40, text("a"))]
        );
        assert_eq!((stamp.seq, stamp.made_at), (4, 70));
        assert!(past.iter().all(|version| version.device.is_none()));
    }

    #[test]
    fn a_recorded_line_without_a_stamp_continues_after_its_last_version() {
        let past = vec![Version {
            uuid: "kept".to_owned(),
            seq: 7,
            made_at: 1,
            device: Some("laptop".to_owned()),
            conflict: true,
            current: false,
            payload: text("old"),
        }];
        let (stamp, kept) = settle(
            Lineage::Recorded {
                stamp: None,
                past: past.clone(),
            },
            &text("new"),
            9,
        )
        .unwrap();
        assert_eq!(kept, past, "another sefy's versions arrive as they were");
        assert_eq!((stamp.seq, stamp.made_at), (8, 9));
    }
}
