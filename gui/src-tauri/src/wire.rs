//! What crosses between the window and the vault.
//!
//! Every value the page receives is one of these, and each is built from what
//! `sefy-core` already answers — the page draws, it does not work anything
//! out. The TypeScript side is generated from them (`tests/bindings.rs`), so
//! the wire has one shape and not two that drift.
//!
//! None of them carries a secret. The vault stays in this process; what the
//! window is told about it is shape — counts, the file, how it is sealed —
//! until a later version reveals one field because someone asked for it.

use serde::Serialize;
use std::path::Path;
use ts_rs::TS;

/// The file the window was asked to open, before anything is unlocked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Opening {
    /// Path of the file, as the launcher or `SEFY_VAULT` gave it; `None`
    /// when nothing named one and the person picks it.
    pub file: Option<String>,
}

/// What and where the open vault is; `sefy_core::Status` for the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// The file's own name, the way the window names the open vault.
    pub name: String,
    /// The whole path, for the line that says where it is.
    pub path: String,
    /// Bytes on disk, when the file system says.
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    /// Items in total.
    #[ts(type = "number")]
    pub items: usize,
    /// How many of each kind, in the order the kinds are declared.
    pub kinds: Vec<KindCount>,
    /// Distinct tags in use.
    #[ts(type = "number")]
    pub tags: usize,
    /// Schema version of the database inside the blob.
    #[ts(type = "number")]
    pub schema: i64,
    /// Whether a newer sefy last wrote it.
    pub written_by_newer: bool,
    /// When it last reached a remote.
    pub last_sync: Option<SyncSeen>,
    /// How many copies are kept beside it.
    #[ts(type = "number")]
    pub copies: usize,
    /// When the newest of them was taken, in Unix seconds.
    #[ts(type = "number | null")]
    pub newest_copy: Option<i64>,
    /// How it is sealed.
    pub sealing: Sealing,
}

/// One kind and how many items of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct KindCount {
    /// The kind's name as stored, known to this build or not.
    pub kind: String,
    /// How many.
    #[ts(type = "number")]
    pub count: usize,
}

/// The last time the vault reached a remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct SyncSeen {
    /// Unix seconds.
    #[ts(type = "number")]
    pub at: i64,
    /// The transport's short name.
    pub transport: String,
    /// `push`, `pull` or `sync`.
    pub operation: String,
}

/// `sefy_core::Sealing`, for the passport of the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Sealing {
    /// Format version.
    pub format: u8,
    /// Key derivation function.
    pub kdf: String,
    /// Its memory, in KiB.
    pub memory_kib: u32,
    /// Its passes.
    pub passes: u32,
    /// Its lanes.
    pub lanes: u32,
    /// The authenticated cipher.
    pub cipher: String,
    /// Salt bytes at the front of the file.
    #[ts(type = "number")]
    pub salt_len: usize,
    /// Nonce bytes after it.
    #[ts(type = "number")]
    pub nonce_len: usize,
    /// Tag bytes at the end.
    #[ts(type = "number")]
    pub tag_len: usize,
}

impl VaultStatus {
    /// The window's view of what the core reports.
    pub fn of(status: &sefy_core::Status) -> Self {
        let stats = &status.stats;
        Self {
            name: file_name(&status.path),
            path: status.path.display().to_string(),
            size: status.size,
            items: stats.items,
            kinds: stats
                .by_kind
                .iter()
                .map(|(kind, count)| KindCount {
                    kind: kind.clone(),
                    count: *count,
                })
                .collect(),
            tags: stats.tags,
            schema: stats.schema,
            written_by_newer: stats.written_by_newer(),
            last_sync: stats.last_sync.as_ref().map(|stamp| SyncSeen {
                at: stamp.at,
                transport: stamp.transport.clone(),
                operation: stamp.operation.clone(),
            }),
            copies: status.copies.len(),
            newest_copy: status
                .copies
                .first()
                .and_then(|copy| copy.taken)
                .and_then(|taken| taken.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_secs() as i64),
            sealing: Sealing::of(&status.sealing),
        }
    }
}

impl Sealing {
    fn of(sealing: &sefy_core::Sealing) -> Self {
        Self {
            format: sealing.format,
            kdf: sealing.kdf.to_owned(),
            memory_kib: sealing.memory_kib,
            passes: sealing.passes,
            lanes: sealing.lanes,
            cipher: sealing.cipher.to_owned(),
            salt_len: sealing.salt_len,
            nonce_len: sealing.nonce_len,
            tag_len: sealing.tag_len,
        }
    }
}

/// The last component of a path, or the whole of it when there is none.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Why something the window asked for did not happen.
///
/// The message is the core's own, which never carries secret material; the
/// kind is what the page acts on — a refused password puts the cursor back in
/// the password field, a missing file in the file field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Failure {
    /// What went wrong, for the page to act on.
    pub kind: FailureKind,
    /// What went wrong, for the person to read.
    pub message: String,
}

/// The kinds of failure the page treats differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    /// The password did not open the file, or the file is not a vault. One
    /// answer for both, as in the core: the file carries nothing to tell them
    /// apart.
    Refused,
    /// The vault was written in a format this build does not read.
    NewerFormat,
    /// Nothing is at that path.
    Missing,
    /// Something is there and cannot be read.
    Unreadable,
    /// The window asked about a vault while none was open.
    Locked,
    /// Anything else.
    Other,
}

impl From<sefy_core::Error> for Failure {
    fn from(error: sefy_core::Error) -> Self {
        use sefy_core::Error;
        let kind = match &error {
            Error::WrongPasswordOrNotAVault | Error::TooSmall => FailureKind::Refused,
            Error::UnsupportedFormat(_) => FailureKind::NewerFormat,
            Error::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
                FailureKind::Missing
            }
            Error::Io { .. } => FailureKind::Unreadable,
            _ => FailureKind::Other,
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
}

impl Failure {
    /// The answer to a question about a vault while none is open.
    pub fn locked() -> Self {
        Self {
            kind: FailureKind::Locked,
            message: "no vault is open".to_owned(),
        }
    }
}
