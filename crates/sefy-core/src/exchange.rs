//! Moving a vault's contents in and out as plain JSON.
//!
//! This is the one place in sefy that deliberately produces plaintext. It
//! exists so a vault is never a trap — contents can be migrated, backed up in
//! another form, or moved to a different tool — but an export is exactly as
//! sensitive as the vault it came from, and nothing here pretends otherwise.
//! Callers are expected to make that explicit to the user before writing one.
//!
//! The shape is deliberately plain and stable:
//!
//! ```json
//! {
//!   "sefy_export": 1,
//!   "items": [
//!     { "title": "bank", "kind": "note", "tags": ["money"], "text": "…" },
//!     { "title": "mail", "kind": "login", "login": "…", "password": "…",
//!       "fields": [ { "name": "login", "value": "…" },
//!                   { "name": "password", "value": "…", "secret": true } ] },
//!     { "title": "key",  "kind": "file", "filename": "id_ed25519",
//!       "bytes_base64": "…" }
//!   ]
//! }
//! ```
//!
//! A record made of fields is written twice over: once as `fields`, which is
//! the whole truth, and once as the flat keys a login used to have. The flat
//! copy is what another tool — or a reader's eye — finds where it expects it;
//! `fields` is what carries a card, an SSH key, and any field a template never
//! heard of. Reading prefers `fields` and falls back to the flat keys, so an
//! export written by 0.6.0 still imports.

use crate::error::{Error, Result};
use crate::model::{Field, ItemKind, NewItem, Payload};
use crate::vault::Vault;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

/// Version of the exchange format written by this build.
pub const EXPORT_VERSION: u32 = 1;

/// A whole vault's contents, ready to be serialized.
#[derive(Debug, Serialize, Deserialize)]
pub struct Export {
    /// Format version, so a future reader knows what it is holding.
    #[serde(rename = "sefy_export")]
    pub version: u32,
    /// Every item, in listing order.
    pub items: Vec<ExportItem>,
}

/// One item in an export.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportItem {
    /// Identity the item carried in the vault it came from.
    ///
    /// Optional on the way in: exports written by 0.1.x have none, and one
    /// hand-written by another tool need not invent one. An import uses it to
    /// recognise an item it already holds instead of duplicating it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// What the item is called.
    pub title: String,
    /// Which of the fields below carry its payload.
    pub kind: String,
    /// Tags attached to it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Body of a note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Fields of a record: a login, a card, an SSH key.
    ///
    /// The authoritative form. The flat keys below repeat a login's fields
    /// under the names it carried up to 0.6.0, so an older sefy and anything
    /// written against it still find them; they say nothing about a card or an
    /// SSH key, which had no flat form to begin with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ExportField>,

    /// Login of a record, repeated from its fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    /// Password of a record, repeated from its fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// URL of a record, repeated from its fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// TOTP secret of a record, repeated from its fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp: Option<String>,
    /// Notes of a record, repeated from its fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    /// Name of a stored file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Contents of a stored file, base64-encoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,

    /// Set when the vault held this item but the exporting build could not read
    /// its contents, because a newer sefy wrote it.
    ///
    /// The entry then carries the item's identity, title and tags and nothing
    /// else. Importing it is refused rather than done partially: an item that
    /// silently arrived empty would be worse than one that was never imported.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contents_not_exported: bool,
}

/// One field of a record in an export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportField {
    /// What the field is called.
    pub name: String,
    /// What it holds.
    pub value: String,
    /// Whether the value is a secret.
    ///
    /// Absent means public, so an export stays readable when written by hand.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
}

/// Collects everything in a vault into an [`Export`].
pub fn export(vault: &Vault) -> Result<Export> {
    let mut items = Vec::new();
    for summary in vault.list()? {
        let item = vault.get(summary.id)?;
        let mut exported = ExportItem {
            uuid: Some(item.summary.uuid),
            title: item.summary.title,
            kind: item.summary.kind.as_str().to_owned(),
            tags: item.summary.tags,
            text: None,
            fields: Vec::new(),
            login: None,
            password: None,
            url: None,
            totp: None,
            notes: None,
            filename: None,
            bytes_base64: None,
            contents_not_exported: false,
        };

        match item.payload {
            Payload::Note { text } => exported.text = Some(text),
            Payload::Fields { fields, .. } => {
                // The flat keys are filled from whatever the record happens to
                // carry under those names — a login always, a card or an SSH
                // key only where it genuinely has a "notes" of its own. A field
                // outside them travels in `fields` alone, which is why that is
                // the form a reader should prefer.
                for field in &fields {
                    let flat = match field.name.as_str() {
                        "login" => &mut exported.login,
                        "password" => &mut exported.password,
                        "url" => &mut exported.url,
                        "totp" => &mut exported.totp,
                        "notes" => &mut exported.notes,
                        _ => continue,
                    };
                    *flat = Some(field.value.clone());
                }
                exported.fields = fields
                    .into_iter()
                    .map(|field| ExportField {
                        name: field.name,
                        value: field.value,
                        secret: field.secret,
                    })
                    .collect();
            }
            // Its contents stay behind: this build cannot read them, and an
            // export that quietly dropped the item would turn "sefy can always
            // get your data out" into a promise that holds only until someone
            // uses a newer version. The entry says what it is and that it came
            // out incomplete, so a reader is never misled into thinking an
            // empty item is all there was.
            Payload::Unknown { .. } => exported.contents_not_exported = true,
            Payload::File { filename, bytes } => {
                exported.filename = Some(filename);
                exported.bytes_base64 = Some(BASE64.encode(&bytes));
            }
        }
        items.push(exported);
    }

    Ok(Export {
        version: EXPORT_VERSION,
        items,
    })
}

/// What an import did, item by item.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Items the vault did not hold, now added.
    pub added: usize,
    /// Items already present under the same identity, left untouched.
    pub skipped: usize,
    /// Entries this build cannot store: a kind it does not know, or an entry
    /// whose contents were left out of the export that produced it.
    ///
    /// They are counted rather than silently dropped, and rather than failing
    /// the whole import: one entry from a newer sefy should not stop the other
    /// nine hundred from arriving.
    pub unsupported: usize,
}

impl ImportReport {
    /// How many entries the export carried in total.
    pub fn total(&self) -> usize {
        self.added + self.skipped + self.unsupported
    }
}

/// Adds the items of an export to a vault, skipping ones it already holds.
///
/// An entry carrying a `uuid` the vault already has is **skipped**, not merged
/// and not duplicated: re-importing the same export twice leaves the vault as
/// it was after the first time. Bringing newer contents across is what
/// [`crate::merge`] is for — an import is not the place to overwrite a secret,
/// because the export may well be the older of the two.
///
/// Entries without a `uuid` — exports written by 0.1.x, or JSON produced by
/// another tool — are always added. There is nothing to recognise them by, and
/// guessing from titles would silently collapse two accounts that share a name.
///
/// The whole export is validated before anything is inserted, so a malformed
/// entry halfway down the file cannot leave a half-imported vault behind.
pub fn import(vault: &mut Vault, export: &Export) -> Result<ImportReport> {
    if export.version != EXPORT_VERSION {
        return Err(Error::UnsupportedExport(export.version));
    }

    let mut report = ImportReport::default();

    // Entries this build cannot represent are counted here and left out of the
    // batch below, so validation of the rest — and the all-or-nothing guarantee
    // that goes with it — is unaffected by their presence.
    let mut prepared: Vec<(Option<&str>, NewItem)> = Vec::new();
    for (index, item) in export.items.iter().enumerate() {
        if item.contents_not_exported || !ItemKind::parse(&item.kind).is_known() {
            report.unsupported += 1;
            continue;
        }
        prepared.push((item.uuid.as_deref(), to_new_item(index, item)?));
    }

    for (uuid, item) in prepared {
        match uuid {
            Some(uuid) if vault.find_by_uuid(uuid)?.is_some() => report.skipped += 1,
            Some(uuid) => {
                let now = crate::vault::now();
                vault.add_existing(item, uuid, now, now)?;
                report.added += 1;
            }
            None => {
                vault.add(item)?;
                report.added += 1;
            }
        }
    }
    vault.save()?;
    Ok(report)
}

/// Turns one exported entry into something the vault will accept.
fn to_new_item(index: usize, item: &ExportItem) -> Result<NewItem> {
    let payload = match ItemKind::parse(&item.kind) {
        ItemKind::Note => Payload::Note {
            text: item.text.clone().ok_or_else(|| Error::MalformedExport {
                index,
                reason: "a note needs a \"text\" field".to_owned(),
            })?,
        },
        kind @ (ItemKind::Login
        | ItemKind::Card
        | ItemKind::SshKey
        | ItemKind::Wifi
        | ItemKind::ApiToken
        | ItemKind::Bank) => {
            let fields = read_fields(index, &kind, item)?;
            if fields.is_empty() {
                return Err(Error::MalformedExport {
                    index,
                    reason: format!("a {kind} needs at least one field"),
                });
            }
            Payload::Fields { kind, fields }
        }
        ItemKind::File => {
            let encoded = item
                .bytes_base64
                .as_deref()
                .ok_or_else(|| Error::MalformedExport {
                    index,
                    reason: "a file needs a \"bytes_base64\" field".to_owned(),
                })?;
            Payload::File {
                filename: item
                    .filename
                    .clone()
                    .ok_or_else(|| Error::MalformedExport {
                        index,
                        reason: "a file needs a \"filename\" field".to_owned(),
                    })?,
                bytes: BASE64
                    .decode(encoded)
                    .map_err(|error| Error::MalformedExport {
                        index,
                        reason: format!("bytes_base64 is not valid base64: {error}"),
                    })?,
            }
        }
        // Filtered out by `import` before it gets here: an unknown kind has no
        // shape to build. Spelled out rather than left to a catch-all, so that
        // adding a kind is a compile error here until it is handled.
        ItemKind::Unknown(name) => {
            return Err(Error::MalformedExport {
                index,
                reason: format!("kind {name:?} is not known to this build"),
            });
        }
    };

    Ok(NewItem::new(item.title.clone(), payload).with_tags(item.tags.clone()))
}

/// Reads a record's fields out of an exported entry.
///
/// `fields` is the authoritative form and wins outright. Only when it is
/// absent — an export written by 0.6.0, or one produced by another tool — are
/// the flat keys read instead, in template order and with the template's idea
/// of what is secret. Both forms are never merged: an entry carrying `fields`
/// has already said everything it holds, and folding stale flat keys in would
/// resurrect a value the writer had dropped.
fn read_fields(index: usize, kind: &ItemKind, item: &ExportItem) -> Result<Vec<Field>> {
    if !item.fields.is_empty() {
        let mut seen: Vec<&str> = Vec::new();
        for field in &item.fields {
            if seen.contains(&field.name.as_str()) {
                return Err(Error::MalformedExport {
                    index,
                    reason: format!("field {:?} appears twice", field.name),
                });
            }
            seen.push(&field.name);
        }
        return Ok(item
            .fields
            .iter()
            .map(|field| Field {
                name: field.name.clone(),
                value: field.value.clone(),
                secret: field.secret,
            })
            .collect());
    }

    let flat = [
        ("login", &item.login),
        ("password", &item.password),
        ("url", &item.url),
        ("totp", &item.totp),
        ("notes", &item.notes),
    ];
    let template = kind.template();
    Ok(flat
        .into_iter()
        .filter_map(|(name, value)| {
            value.as_ref().map(|value| Field {
                name: name.to_owned(),
                value: value.clone(),
                // A name the template does not list is taken as secret: for a
                // value of unknown meaning, hiding it is the mistake that can
                // be undone.
                secret: template
                    .and_then(|template| template.field(name))
                    .is_none_or(|field| field.secret),
            })
        })
        .collect())
}

/// Renders an export as indented JSON.
pub fn to_json(export: &Export) -> Result<String> {
    serde_json::to_string_pretty(export).map_err(|error| Error::UnreadableExport(error.to_string()))
}

/// Parses an export from JSON.
pub fn from_json(json: &str) -> Result<Export> {
    serde_json::from_str(json).map_err(|error| Error::UnreadableExport(error.to_string()))
}
