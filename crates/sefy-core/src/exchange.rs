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
//!     { "title": "mail", "kind": "credential", "login": "…", "password": "…" },
//!     { "title": "key",  "kind": "file", "filename": "id_ed25519",
//!       "bytes_base64": "…" }
//!   ]
//! }
//! ```

use crate::error::{Error, Result};
use crate::model::{Credential, ItemKind, NewItem, Payload};
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

    /// Login of a credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
    /// Password of a credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// URL of a credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// TOTP secret of a credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp: Option<String>,
    /// Notes of a credential.
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
            Payload::Credential(credential) => {
                exported.login = Some(credential.login);
                exported.password = Some(credential.password);
                exported.url = credential.url;
                exported.totp = credential.totp;
                exported.notes = credential.notes;
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
        ItemKind::Credential => Payload::Credential(Credential {
            login: item.login.clone().ok_or_else(|| Error::MalformedExport {
                index,
                reason: "a credential needs a \"login\" field".to_owned(),
            })?,
            password: item
                .password
                .clone()
                .ok_or_else(|| Error::MalformedExport {
                    index,
                    reason: "a credential needs a \"password\" field".to_owned(),
                })?,
            url: item.url.clone(),
            totp: item.totp.clone(),
            notes: item.notes.clone(),
        }),
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

/// Renders an export as indented JSON.
pub fn to_json(export: &Export) -> Result<String> {
    serde_json::to_string_pretty(export).map_err(|error| Error::UnreadableExport(error.to_string()))
}

/// Parses an export from JSON.
pub fn from_json(json: &str) -> Result<Export> {
    serde_json::from_str(json).map_err(|error| Error::UnreadableExport(error.to_string()))
}
