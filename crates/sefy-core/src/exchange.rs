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
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
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
}

/// Collects everything in a vault into an [`Export`].
pub fn export(vault: &Vault) -> Result<Export> {
    let mut items = Vec::new();
    for summary in vault.list()? {
        let item = vault.get(summary.id)?;
        let mut exported = ExportItem {
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

/// Adds every item of an export to a vault, returning how many arrived.
///
/// Items are appended, never matched against what is already there: an import
/// into a non-empty vault duplicates rather than merges. Merging would need an
/// identity for items that the format does not have, and silently overwriting
/// someone's secrets is worse than a visible duplicate.
///
/// The whole export is validated before anything is inserted, so a malformed
/// entry halfway down the file cannot leave a half-imported vault behind.
pub fn import(vault: &mut Vault, export: &Export) -> Result<usize> {
    if export.version != EXPORT_VERSION {
        return Err(Error::UnsupportedExport(export.version));
    }

    let prepared: Vec<NewItem> = export
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| to_new_item(index, item))
        .collect::<Result<_>>()?;

    let count = prepared.len();
    for item in prepared {
        vault.add(item)?;
    }
    vault.save()?;
    Ok(count)
}

/// Turns one exported entry into something the vault will accept.
fn to_new_item(index: usize, item: &ExportItem) -> Result<NewItem> {
    let kind = ItemKind::parse(&item.kind).ok_or_else(|| Error::MalformedExport {
        index,
        reason: format!("unknown kind {:?}", item.kind),
    })?;

    let payload = match kind {
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
