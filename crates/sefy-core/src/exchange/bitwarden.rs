//! Bitwarden's JSON export: "File → Export vault → .json" in the apps, or
//! `bw export --format json`, from a personal vault or an organization's.
//!
//! Each item has a numeric type and an object of that type's fields. Logins,
//! notes, cards, SSH keys and bank accounts have sefy kinds and map onto them;
//! an identity, a driver's license and a passport have none, and become a note
//! listing every field, so nothing they hold is left behind. Folders and
//! collections become tags.
//!
//! Bitwarden keeps the last few passwords of an item, and the last values of
//! its hidden fields, as `passwordHistory`. They arrive as the item's earlier
//! versions, each differing from the one after it in that one value.

use super::{Entry, Lineage, Outcome, Reading, Record, UNREADABLE_TOTP, normalize_uuid, title_or};
use crate::error::{Error, Result};
use crate::model::{ItemKind, NewItem, Payload};
use crate::time;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

const FORMAT: &str = "a Bitwarden JSON export";

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    encrypted: bool,
    #[serde(default, rename = "encKeyValidation_DO_NOT_EDIT")]
    key_check: Option<Value>,
    #[serde(default)]
    folders: Vec<Group>,
    #[serde(default)]
    collections: Vec<Group>,
    #[serde(default)]
    items: Vec<Item>,
}

/// A folder or a collection: an id and a name.
#[derive(Deserialize)]
struct Group {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<u32>,
    name: Option<String>,
    notes: Option<String>,
    folder_id: Option<String>,
    collection_ids: Option<Vec<String>>,
    fields: Option<Vec<CustomField>>,
    login: Option<Login>,
    card: Option<Map<String, Value>>,
    identity: Option<Map<String, Value>>,
    ssh_key: Option<Map<String, Value>>,
    bank_account: Option<Map<String, Value>>,
    drivers_license: Option<Map<String, Value>>,
    passport: Option<Map<String, Value>>,
    password_history: Option<Vec<PastPassword>>,
    creation_date: Option<String>,
    revision_date: Option<String>,
    deleted_date: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Login {
    uris: Option<Vec<Uri>>,
    username: Option<String>,
    password: Option<String>,
    totp: Option<String>,
    fido2_credentials: Option<Vec<Value>>,
}

#[derive(Deserialize)]
struct Uri {
    uri: Option<String>,
}

#[derive(Deserialize)]
struct CustomField {
    name: Option<String>,
    value: Option<String>,
    #[serde(rename = "type")]
    kind: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PastPassword {
    password: Option<String>,
    last_used_date: Option<String>,
}

/// Custom field types, as Bitwarden numbers them.
const FIELD_TEXT: u32 = 0;
const FIELD_HIDDEN: u32 = 1;
const FIELD_BOOLEAN: u32 = 2;
const FIELD_LINKED: u32 = 3;

/// Reads a Bitwarden export into entries.
pub(super) fn read(body: &str) -> Result<Reading> {
    let file: File = serde_json::from_str(body).map_err(|error| Error::UnreadableImport {
        format: FORMAT,
        reason: error.to_string(),
    })?;
    if file.encrypted || file.key_check.is_some() {
        return Err(Error::SealedImport {
            format: "Bitwarden export",
            advice: "export the vault again choosing the plain .json format, not \
                     .json (Encrypted)",
        });
    }

    let groups: HashMap<String, String> = file
        .folders
        .into_iter()
        .chain(file.collections)
        .filter_map(|group| Some((group.id?, group.name?)))
        .collect();

    let mut reading = Reading::default();
    for item in file.items {
        read_item(item, &groups, &mut reading);
    }
    Ok(reading)
}

fn read_item(item: Item, groups: &HashMap<String, String>, reading: &mut Reading) {
    let login = item.login.as_ref();
    let url = login
        .and_then(|login| login.uris.as_ref())
        .and_then(|uris| uris.first())
        .and_then(|uri| uri.uri.as_deref())
        .unwrap_or("");
    let username = login
        .and_then(|login| login.username.as_deref())
        .unwrap_or("");
    let title = title_or(item.name.as_deref().unwrap_or(""), url, username);

    if item
        .deleted_date
        .as_deref()
        .is_some_and(|date| !date.is_empty())
    {
        reading.notice(&title, Outcome::NotImported, "in the trash");
        return;
    }

    let tags: Vec<String> = item
        .folder_id
        .iter()
        .chain(item.collection_ids.iter().flatten())
        .filter_map(|id| groups.get(id).cloned())
        .collect();
    let notes = item.notes.as_deref().unwrap_or("");
    let custom = item.fields.as_deref().unwrap_or(&[]);

    let payload = match item.kind {
        Some(1) => {
            let mut record = Record::new(ItemKind::Login);
            if let Some(login) = login {
                record.put("login", username, None);
                record.put("password", login.password.as_deref().unwrap_or(""), None);
                for uri in login.uris.iter().flatten() {
                    record.put("url", uri.uri.as_deref().unwrap_or(""), None);
                }
                if !record.put_totp(login.totp.as_deref().unwrap_or("")) {
                    reading.notice(&title, Outcome::Reshaped, UNREADABLE_TOTP);
                }
                if login
                    .fido2_credentials
                    .as_ref()
                    .is_some_and(|keys| !keys.is_empty())
                {
                    reading.notice(
                        &title,
                        Outcome::InPart,
                        "its passkey stays in Bitwarden: a passkey cannot be exported",
                    );
                }
            }
            finish(record, notes, custom, &title, reading)
        }
        Some(2) => Some(note(notes, custom, &title, reading)),
        Some(3) => {
            let card = item.card.unwrap_or_default();
            let mut record = Record::new(ItemKind::Card);
            record.put("number", &text(&card, "number"), None);
            record.put("holder", &text(&card, "cardholderName"), None);
            record.put("expiry", &expiry(&card), None);
            record.put("cvv", &text(&card, "code"), None);
            record.put("brand", &text(&card, "brand"), Some(false));
            finish(record, notes, custom, &title, reading)
        }
        Some(5) => {
            let key = item.ssh_key.unwrap_or_default();
            let mut record = Record::new(ItemKind::SshKey);
            record.put("private-key", &text(&key, "privateKey"), None);
            record.put("public-key", &text(&key, "publicKey"), None);
            record.put("fingerprint", &text(&key, "keyFingerprint"), Some(false));
            finish(record, notes, custom, &title, reading)
        }
        Some(6) => {
            let bank = item.bank_account.unwrap_or_default();
            let mut record = Record::new(ItemKind::Bank);
            let number = text(&bank, "accountNumber");
            let iban = text(&bank, "iban");
            // The template has one place for the account and one for the
            // code that routes to it; whichever is filled takes it, and the
            // other keeps its own name.
            if number.trim().is_empty() {
                record.put("account", &iban, None);
            } else {
                record.put("account", &number, None);
                record.put("iban", &iban, Some(true));
            }
            let routing = text(&bank, "routingNumber");
            let swift = text(&bank, "swiftCode");
            if routing.trim().is_empty() {
                record.put("routing", &swift, None);
            } else {
                record.put("routing", &routing, None);
                record.put("swift", &swift, Some(false));
            }
            record.put("holder", &text(&bank, "nameOnAccount"), None);
            record.put("bank", &text(&bank, "bankName"), None);
            record.put("branch", &text(&bank, "branchNumber"), Some(false));
            record.put("account-type", &text(&bank, "accountType"), Some(false));
            record.put("pin", &text(&bank, "pin"), Some(true));
            record.put("phone", &text(&bank, "bankContactPhone"), Some(false));
            finish(record, notes, custom, &title, reading)
        }
        Some(number @ (4 | 7 | 8)) => {
            let (fields, what) = match number {
                4 => (item.identity.as_ref(), "an identity"),
                7 => (item.drivers_license.as_ref(), "a driver's license"),
                _ => (item.passport.as_ref(), "a passport"),
            };
            reading.notice(
                &title,
                Outcome::Reshaped,
                format!("{what}, which sefy has no kind for; kept as a note listing its fields"),
            );
            let mut text = String::new();
            for (key, value) in fields.into_iter().flatten() {
                if let Some(value) = scalar(value) {
                    text.push_str(&format!("{}: {value}\n", words(key)));
                }
            }
            if !notes.trim().is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(notes);
                text.push('\n');
            }
            Some(note(text.trim_end(), custom, &title, reading))
        }
        other => {
            let reason = match other {
                Some(number) => {
                    format!("a Bitwarden item of type {number}, which sefy does not know")
                }
                None => "a Bitwarden item with no type".to_owned(),
            };
            reading.notice(&title, Outcome::NotImported, reason);
            None
        }
    };

    let Some(payload) = payload else {
        if matches!(item.kind, Some(1 | 3 | 5 | 6)) {
            reading.notice(&title, Outcome::NotImported, "nothing in it but a name");
        }
        return;
    };

    let earlier = earlier(
        &payload,
        item.password_history.as_deref().unwrap_or(&[]),
        custom,
        item.creation_date.as_deref(),
        &title,
        reading,
    );
    reading.push(Entry {
        uuid: item.id.as_deref().and_then(normalize_uuid),
        item: NewItem::new(title, payload).with_tags(tags),
        created_at: item.creation_date.as_deref().and_then(time::parse_rfc3339),
        updated_at: item.revision_date.as_deref().and_then(time::parse_rfc3339),
        lineage: Lineage::Earlier(earlier),
        notices: Vec::new(),
    });
}

/// Adds the notes and custom fields every record carries, and makes it a
/// payload.
fn finish(
    mut record: Record,
    notes: &str,
    custom: &[CustomField],
    title: &str,
    reading: &mut Reading,
) -> Option<Payload> {
    record.put("notes", notes, None);
    for field in custom {
        let name = field.name.as_deref().unwrap_or("");
        let value = field.value.as_deref().unwrap_or("");
        match field.kind.unwrap_or(FIELD_TEXT) {
            FIELD_HIDDEN => record.put(name, value, Some(true)),
            FIELD_TEXT | FIELD_BOOLEAN => record.put(name, value, Some(false)),
            FIELD_LINKED => reading.notice(
                title,
                Outcome::InPart,
                format!("its linked field {name:?} is left out: it only points at another field"),
            ),
            _ => record.put(name, value, None),
        }
    }
    if record.is_empty() {
        None
    } else {
        record.into_payload()
    }
}

/// A note, with any custom fields written into its text: a sefy note is text
/// and nothing else, and dropping the fields would lose them.
fn note(text: &str, custom: &[CustomField], title: &str, reading: &mut Reading) -> Payload {
    let mut text = text.to_owned();
    let mut folded = false;
    for field in custom {
        if field.kind == Some(FIELD_LINKED) {
            continue;
        }
        let value = field.value.as_deref().unwrap_or("");
        if value.trim().is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!(
            "{}: {value}",
            field.name.as_deref().unwrap_or("field")
        ));
        folded = true;
    }
    if folded {
        reading.notice(
            title,
            Outcome::Reshaped,
            "a note with fields of its own; they are added to its text",
        );
    }
    Payload::Note { text }
}

/// The item's earlier contents, rebuilt from the values Bitwarden kept.
///
/// Bitwarden keeps old values, not old states: each entry says what a password
/// — or a hidden field, written as `name: value` — was until a moment. Walking
/// back from the current contents and putting each old value back gives the
/// state before every change; each is dated by the change that ended the one
/// before it, the oldest by the item's creation.
fn earlier(
    current: &Payload,
    history: &[PastPassword],
    custom: &[CustomField],
    created: Option<&str>,
    title: &str,
    reading: &mut Reading,
) -> Vec<(i64, Payload)> {
    let Payload::Fields { kind, fields } = current else {
        return Vec::new();
    };
    let hidden: Vec<&str> = custom
        .iter()
        .filter(|field| field.kind == Some(FIELD_HIDDEN))
        .filter_map(|field| field.name.as_deref())
        .collect();

    let mut dated: Vec<(i64, &str)> = history
        .iter()
        .filter_map(|past| {
            Some((
                past.last_used_date
                    .as_deref()
                    .and_then(time::parse_rfc3339)
                    .unwrap_or(0),
                past.password.as_deref()?,
            ))
        })
        .collect();
    // Newest first, the way the walk back goes.
    dated.sort_by_key(|(at, _)| std::cmp::Reverse(*at));

    let mut state = fields.clone();
    let mut versions: Vec<(i64, Payload)> = Vec::new();
    let mut unplaced = 0;
    for (index, (_, old)) in dated.iter().enumerate() {
        let (name, value) = match hidden.iter().find_map(|name| {
            old.strip_prefix(&format!("{name}: "))
                .map(|value| (*name, value))
        }) {
            Some(hit) => hit,
            None if state.iter().any(|field| field.name == "password") => ("password", *old),
            None => {
                unplaced += 1;
                continue;
            }
        };
        match state.iter_mut().find(|field| field.name == name) {
            Some(field) => field.value = value.to_owned(),
            None => state.push(crate::model::Field::secret(name, value)),
        }
        // This state lasted from the change before it, or from the start.
        let since = dated
            .get(index + 1)
            .map(|(at, _)| *at)
            .or_else(|| created.and_then(time::parse_rfc3339))
            .unwrap_or(0);
        versions.push((
            since,
            Payload::Fields {
                kind: kind.clone(),
                fields: state.clone(),
            },
        ));
    }
    if unplaced > 0 {
        reading.notice(
            title,
            Outcome::InPart,
            format!(
                "{unplaced} earlier value{} in its history belong{} to no field it has, left out",
                if unplaced == 1 { "" } else { "s" },
                if unplaced == 1 { "s" } else { "" }
            ),
        );
    }
    versions.reverse();
    versions
}

/// A field of a type's object as text; empty when absent or null.
fn text(object: &Map<String, Value>, key: &str) -> String {
    object.get(key).and_then(scalar).unwrap_or_default()
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// A card's expiry as it is printed on one: `MM/YYYY`, or whichever half is
/// there.
fn expiry(card: &Map<String, Value>) -> String {
    let month = text(card, "expMonth");
    let year = text(card, "expYear");
    let month = match month.trim().parse::<u32>() {
        Ok(number) if (1..=12).contains(&number) => format!("{number:02}"),
        _ => month.trim().to_owned(),
    };
    match (month.is_empty(), year.trim().is_empty()) {
        (false, false) => format!("{month}/{}", year.trim()),
        (false, true) => month,
        (true, false) => year.trim().to_owned(),
        (true, true) => String::new(),
    }
}

/// `firstName` as `first name`, for a field written into a note.
fn words(key: &str) -> String {
    let mut words = String::with_capacity(key.len() + 4);
    for character in key.chars() {
        if character.is_uppercase() {
            if !words.is_empty() {
                words.push(' ');
            }
            words.extend(character.to_lowercase());
        } else {
            words.push(character);
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_name_reads_as_words() {
        assert_eq!(words("firstName"), "first name");
        assert_eq!(words("ssn"), "ssn");
        assert_eq!(words("passportNumber"), "passport number");
    }

    #[test]
    fn an_expiry_comes_out_as_printed() {
        let card: Map<String, Value> =
            serde_json::from_str(r#"{"expMonth":"3","expYear":"2029"}"#).unwrap();
        assert_eq!(expiry(&card), "03/2029");
        let card: Map<String, Value> = serde_json::from_str(r#"{"expYear":"2029"}"#).unwrap();
        assert_eq!(expiry(&card), "2029");
    }
}
