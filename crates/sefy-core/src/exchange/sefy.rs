//! sefy's own exchange format: the whole vault as plain JSON.
//!
//! The shape is deliberately plain and stable:
//!
//! ```json
//! {
//!   "sefy_export": 1,
//!   "items": [
//!     { "uuid": "…", "title": "bank", "kind": "note", "tags": ["money"],
//!       "created_at": 1709164800, "updated_at": 1709251200,
//!       "version": { "uuid": "…", "seq": 2, "made_at": 1709251200 },
//!       "text": "…" },
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
//!
//! Everything past `title`, `kind` and the contents is optional on the way in,
//! so a file written by hand or by another program needs nothing it cannot
//! know. What sefy writes beyond that — identity, times, the version the
//! contents are, and on request the earlier ones — is what lets an export
//! imported elsewhere be merged with the vault it came from later without
//! either side mistaking the other's versions for strangers.

use super::{Entry, ExportReport, Exported, Lineage, Outcome, Reading, gather};
use crate::db::Stamp;
use crate::error::{Error, Result};
use crate::history::Version;
use crate::model::{Field, ItemKind, NewItem, Payload};
use crate::vault::Vault;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Version of the exchange format written by this build.
///
/// Still 1: everything added since is optional on the way in and ignored by a
/// reader that does not know it, so a file written now imports into an older
/// sefy, which takes the current contents and leaves the rest.
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
#[derive(Debug, Default, Serialize, Deserialize)]
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

    /// When the item was created, seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    /// When it last changed, seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    /// The version its contents are.
    ///
    /// Carried so the contents keep their identity in the vault they are
    /// imported into: given a new one there, a later merge with the vault they
    /// came from would take one set of contents for two and call it a conflict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<ExportStamp>,
    /// Earlier versions of its contents, oldest first.
    ///
    /// Written only when asked for: by default an export is a snapshot of what
    /// each item holds now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<ExportVersion>,

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

/// Which version an item's contents are.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportStamp {
    /// Identity of the version, the same in every vault that holds it.
    pub uuid: String,
    /// Place in the item's line of edits.
    pub seq: i64,
    /// When the contents were written, seconds since the Unix epoch.
    pub made_at: i64,
    /// Name of the machine they were written on, when it was known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

/// One earlier version of an item's contents.
///
/// The contents take the same keys as the item's own — `text` for a note,
/// `fields` for a record, `filename` and `bytes_base64` for a file — and the
/// kind is the item's.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportVersion {
    /// Identity of the version.
    pub uuid: String,
    /// Place in the item's line of edits.
    pub seq: i64,
    /// When the contents were written, seconds since the Unix epoch.
    pub made_at: i64,
    /// Name of the machine they were written on, when it was known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Whether this version lost a conflict in a merge and was kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub conflict: bool,
    /// Body of a note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Fields of a record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ExportField>,
    /// Name of a stored file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Contents of a stored file, base64-encoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
}

/// The contents part of an entry, whether an item's or an earlier version's.
struct Contents<'a> {
    text: &'a Option<String>,
    fields: &'a [ExportField],
    flat: [(&'static str, &'a Option<String>); 5],
    filename: &'a Option<String>,
    bytes_base64: &'a Option<String>,
}

const NONE: Option<String> = None;

impl ExportItem {
    fn contents(&self) -> Contents<'_> {
        Contents {
            text: &self.text,
            fields: &self.fields,
            flat: [
                ("login", &self.login),
                ("password", &self.password),
                ("url", &self.url),
                ("totp", &self.totp),
                ("notes", &self.notes),
            ],
            filename: &self.filename,
            bytes_base64: &self.bytes_base64,
        }
    }
}

impl ExportVersion {
    fn contents(&self) -> Contents<'_> {
        Contents {
            text: &self.text,
            fields: &self.fields,
            flat: [
                ("login", &NONE),
                ("password", &NONE),
                ("url", &NONE),
                ("totp", &NONE),
                ("notes", &NONE),
            ],
            filename: &self.filename,
            bytes_base64: &self.bytes_base64,
        }
    }
}

/// Writes a vault as sefy's JSON.
pub(super) fn write(vault: &Vault, history: bool) -> Result<Exported> {
    let mut report = ExportReport::default();
    let mut items = Vec::new();
    for outgoing in gather(vault, history)? {
        let summary = outgoing.summary;
        let mut exported = ExportItem {
            uuid: Some(summary.uuid),
            title: summary.title,
            kind: summary.kind.as_str().to_owned(),
            tags: summary.tags,
            created_at: Some(summary.created_at),
            updated_at: Some(summary.updated_at),
            version: Some(ExportStamp {
                uuid: outgoing.stamp.uuid,
                seq: outgoing.stamp.seq,
                made_at: outgoing.stamp.made_at,
                device: outgoing.stamp.device,
            }),
            ..ExportItem::default()
        };

        match outgoing.payload {
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
                exported.fields = export_fields(fields);
            }
            // Its contents stay behind: this build cannot read them, and an
            // export that quietly dropped the item would turn "sefy can always
            // get your data out" into a promise that holds only until someone
            // uses a newer version. The entry says what it is and that it came
            // out incomplete, so a reader is never misled into thinking an
            // empty item is all there was.
            Payload::Unknown { .. } => {
                exported.contents_not_exported = true;
                exported.version = None;
                report.unreadable += 1;
            }
            Payload::File { filename, bytes } => {
                exported.filename = Some(filename);
                exported.bytes_base64 = Some(BASE64.encode(&bytes));
            }
        }

        for version in outgoing.past {
            let mut written = ExportVersion {
                uuid: version.uuid,
                seq: version.seq,
                made_at: version.made_at,
                device: version.device,
                conflict: version.conflict,
                ..ExportVersion::default()
            };
            match version.payload {
                Payload::Note { text } => written.text = Some(text),
                Payload::Fields { fields, .. } => written.fields = export_fields(fields),
                Payload::File { filename, bytes } => {
                    written.filename = Some(filename);
                    written.bytes_base64 = Some(BASE64.encode(&bytes));
                }
                Payload::Unknown { .. } => continue,
            }
            exported.history.push(written);
            report.versions += 1;
        }

        report.written += 1;
        items.push(exported);
    }

    let export = Export {
        version: EXPORT_VERSION,
        items,
    };
    let text = serde_json::to_string_pretty(&export).map_err(|error| Error::Unexportable {
        title: String::new(),
        format: "a sefy export",
        reason: error.to_string(),
    })?;
    Ok(Exported {
        text: Zeroizing::new(text),
        report,
    })
}

fn export_fields(fields: Vec<Field>) -> Vec<ExportField> {
    fields
        .into_iter()
        .map(|field| ExportField {
            name: field.name,
            value: field.value,
            secret: field.secret,
        })
        .collect()
}

/// Reads sefy's JSON into entries.
///
/// The whole file is checked before anything is returned, so a malformed
/// entry halfway down cannot leave a half-imported vault behind. An entry of a
/// kind this build does not know, or one whose contents a newer sefy left out,
/// is named in a notice instead: one entry from a newer version should not
/// stop the other nine hundred from arriving.
pub(super) fn read(body: &str) -> Result<Reading> {
    let export: Export = serde_json::from_str(body).map_err(|error| Error::UnreadableImport {
        format: super::Format::Sefy.describe(),
        reason: error.to_string(),
    })?;
    if export.version != EXPORT_VERSION {
        return Err(Error::UnsupportedExport(export.version));
    }

    let mut reading = Reading::default();
    for (index, item) in export.items.iter().enumerate() {
        let kind = ItemKind::parse(&item.kind);
        if item.contents_not_exported {
            reading.notice(
                &item.title,
                Outcome::NotImported,
                "a newer sefy wrote it, and its contents were left out of the export",
            );
            continue;
        }
        if !kind.is_known() {
            reading.notice(
                &item.title,
                Outcome::NotImported,
                format!(
                    "a {kind}, a kind this version of sefy does not know; upgrade and import again"
                ),
            );
            continue;
        }

        let payload = payload_of(index, &kind, &item.contents())?;
        let mut past = Vec::with_capacity(item.history.len());
        for version in &item.history {
            past.push(Version {
                uuid: version.uuid.clone(),
                seq: version.seq,
                made_at: version.made_at,
                device: version.device.clone(),
                conflict: version.conflict,
                current: false,
                payload: payload_of(index, &kind, &version.contents())?,
            });
        }

        reading.push(Entry {
            uuid: item.uuid.clone(),
            item: NewItem::new(item.title.clone(), payload).with_tags(item.tags.clone()),
            created_at: item.created_at,
            updated_at: item.updated_at,
            lineage: Lineage::Recorded {
                stamp: item.version.as_ref().map(|stamp| Stamp {
                    uuid: stamp.uuid.clone(),
                    seq: stamp.seq,
                    made_at: stamp.made_at,
                    device: stamp.device.clone(),
                }),
                past,
            },
            notices: Vec::new(),
        });
    }
    Ok(reading)
}

/// Turns one entry's contents into something the vault will accept.
fn payload_of(index: usize, kind: &ItemKind, contents: &Contents<'_>) -> Result<Payload> {
    let malformed = |reason: String| Error::MalformedExport { index, reason };
    Ok(match kind {
        ItemKind::Note => Payload::Note {
            text: contents
                .text
                .clone()
                .ok_or_else(|| malformed("a note needs a \"text\" field".to_owned()))?,
        },
        kind @ (ItemKind::Login
        | ItemKind::Card
        | ItemKind::SshKey
        | ItemKind::Wifi
        | ItemKind::ApiToken
        | ItemKind::Bank) => {
            let fields = read_fields(index, kind, contents)?;
            if fields.is_empty() {
                return Err(malformed(format!("a {kind} needs at least one field")));
            }
            Payload::Fields {
                kind: kind.clone(),
                fields,
            }
        }
        ItemKind::File => {
            let encoded = contents
                .bytes_base64
                .as_deref()
                .ok_or_else(|| malformed("a file needs a \"bytes_base64\" field".to_owned()))?;
            Payload::File {
                filename: contents
                    .filename
                    .clone()
                    .ok_or_else(|| malformed("a file needs a \"filename\" field".to_owned()))?,
                bytes: BASE64.decode(encoded).map_err(|error| {
                    malformed(format!("bytes_base64 is not valid base64: {error}"))
                })?,
            }
        }
        // Filtered out by `read` before it gets here: an unknown kind has no
        // shape to build. Spelled out rather than left to a catch-all, so that
        // adding a kind is a compile error here until it is handled.
        ItemKind::Unknown(name) => {
            return Err(malformed(format!(
                "kind {name:?} is not known to this build"
            )));
        }
    })
}

/// Reads a record's fields out of an entry.
///
/// `fields` is the authoritative form and wins outright. Only when it is
/// absent — an export written by 0.6.0, or one produced by another tool — are
/// the flat keys read instead, in template order and with the template's idea
/// of what is secret. Both forms are never merged: an entry carrying `fields`
/// has already said everything it holds, and folding stale flat keys in would
/// resurrect a value the writer had dropped.
fn read_fields(index: usize, kind: &ItemKind, contents: &Contents<'_>) -> Result<Vec<Field>> {
    if !contents.fields.is_empty() {
        let mut seen: Vec<&str> = Vec::new();
        for field in contents.fields {
            if seen.contains(&field.name.as_str()) {
                return Err(Error::MalformedExport {
                    index,
                    reason: format!("field {:?} appears twice", field.name),
                });
            }
            seen.push(&field.name);
        }
        return Ok(contents
            .fields
            .iter()
            .map(|field| Field {
                name: field.name.clone(),
                value: field.value.clone(),
                secret: field.secret,
            })
            .collect());
    }

    let template = kind.template();
    Ok(contents
        .flat
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
