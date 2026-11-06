//! What crosses between the window and the vault.
//!
//! Every value the page receives is one of these, and each is built from what
//! `sefy-core` already answers — the page draws, it does not work anything
//! out. The TypeScript side is generated from them (`tests/bindings.rs`), so
//! the wire has one shape and not two that drift.
//!
//! None of them carries a secret but [`Revealed`]. The vault stays in this
//! process; what the window is told about an item is what `sefy show` prints:
//! titles, tags, public fields, a note's text, with every secret field named
//! and never filled in. A secret reaches the page only when someone
//! asks to see that one field, and reaches the clipboard without passing
//! through the page at all.

use serde::{Deserialize, Serialize};
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

/// Which items the list shows: what is typed in the search box, narrowed to
/// one kind or one tag picked in the rail.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
pub struct Filter {
    /// Text to look for in titles, notes and public fields; never in a
    /// secret, the same rule as `sefy find`.
    pub text: Option<String>,
    /// One kind, by the name it is stored under.
    pub kind: Option<String>,
    /// One tag.
    pub tag: Option<String>,
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ItemRow {
    /// The item's id in this vault: what the window asks about it by.
    #[ts(type = "number")]
    pub id: i64,
    /// What it is called.
    pub title: String,
    /// Its kind, as stored.
    pub kind: String,
    /// Whether this build knows the kind; an item a newer sefy wrote is
    /// listed, and cannot be opened.
    pub known: bool,
    /// Its tags, sorted.
    pub tags: Vec<String>,
    /// When it last changed, in Unix seconds.
    #[ts(type = "number")]
    pub updated_at: i64,
}

impl ItemRow {
    /// The row for an item the core listed.
    pub fn of(summary: &sefy_core::ItemSummary) -> Self {
        Self {
            id: summary.id,
            title: summary.title.clone(),
            kind: summary.kind.as_str().to_owned(),
            known: summary.kind.is_known(),
            tags: summary.tags.clone(),
            updated_at: summary.updated_at,
        }
    }
}

/// A tag and how many items carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct TagCount {
    /// The tag.
    pub tag: String,
    /// How many items carry it.
    #[ts(type = "number")]
    pub count: usize,
}

/// An item opened in the card: everything `sefy show` prints, and no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ItemCard {
    /// The item's id in this vault.
    #[ts(type = "number")]
    pub id: i64,
    /// Identity across vaults, the one merging matches on.
    pub uuid: String,
    /// What it is called.
    pub title: String,
    /// Its kind, as stored.
    pub kind: String,
    /// Its tags, sorted.
    pub tags: Vec<String>,
    /// When it was made, in Unix seconds.
    #[ts(type = "number")]
    pub created_at: i64,
    /// When it last changed, in Unix seconds.
    #[ts(type = "number")]
    pub updated_at: i64,
    /// How many earlier versions of it are kept.
    #[ts(type = "number")]
    pub earlier_versions: usize,
    /// What it holds.
    pub body: CardBody,
}

/// What an item holds, in the shape its kind gives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "shape", rename_all = "kebab-case")]
pub enum CardBody {
    /// A note: its text, which `sefy show` prints too.
    Note {
        /// The text.
        text: String,
    },
    /// A record made of fields.
    Record {
        /// Its fields, in the order they are meant to be read.
        fields: Vec<CardField>,
    },
    /// A file kept in the vault.
    File {
        /// The name it had.
        filename: String,
        /// Its size in bytes.
        #[ts(type = "number")]
        size: usize,
    },
    /// A kind this build does not know: listed, kept, and not opened.
    Unknown,
}

/// One field of a record, as the card draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "shape", rename_all = "kebab-case")]
pub enum CardField {
    /// A public value, shown as it is.
    Plain {
        /// The field's name.
        name: String,
        /// Its value.
        value: String,
        /// For the date a record stops working: days until then, negative
        /// once it has passed.
        #[serde(rename = "daysLeft")]
        #[ts(rename = "daysLeft", type = "number | null")]
        days_left: Option<i64>,
    },
    /// A secret: named, never sent. Shown when asked for, copied without
    /// passing through the page.
    Secret {
        /// The field's name.
        name: String,
    },
    /// A one-time password key. What it makes is a code, and the code is
    /// what is shown and copied; the key itself never leaves.
    Code {
        /// The field's name.
        name: String,
        /// How many seconds a code lives.
        #[ts(type = "number")]
        period: u64,
    },
}

impl ItemCard {
    /// The card for an item, as of `now` (Unix seconds).
    pub fn of(item: &sefy_core::Item, earlier_versions: usize, now: i64) -> Self {
        let summary = &item.summary;
        Self {
            id: summary.id,
            uuid: summary.uuid.clone(),
            title: summary.title.clone(),
            kind: summary.kind.as_str().to_owned(),
            tags: summary.tags.clone(),
            created_at: summary.created_at,
            updated_at: summary.updated_at,
            earlier_versions,
            body: CardBody::of(&item.payload, now),
        }
    }
}

impl CardBody {
    /// What a payload holds, for the card: as of `now` (Unix seconds), every
    /// secret named and none filled in.
    pub fn of(payload: &sefy_core::Payload, now: i64) -> Self {
        use sefy_core::Payload;
        match payload {
            Payload::Note { text } => Self::Note { text: text.clone() },
            Payload::Fields { fields, .. } => Self::Record {
                fields: fields
                    .iter()
                    .map(|field| CardField::of(field, now))
                    .collect(),
            },
            Payload::File { filename, bytes } => Self::File {
                filename: filename.clone(),
                size: bytes.len(),
            },
            Payload::Unknown { .. } => Self::Unknown,
        }
    }
}

impl CardField {
    fn of(field: &sefy_core::Field, now: i64) -> Self {
        let name = field.name.clone();
        if field.name == sefy_core::otp::FIELD
            && let Ok(totp) = sefy_core::Totp::parse(&field.value)
        {
            return Self::Code {
                name,
                period: totp.period,
            };
        }
        // A key that cannot make a code is still a secret, and stays one.
        if field.secret {
            return Self::Secret { name };
        }
        let days_left = (field.name == sefy_core::expiry::FIELD)
            .then(|| sefy_core::Expiry::parse(&field.value))
            .flatten()
            .map(|date| date.days_left(now));
        Self::Plain {
            name,
            value: field.value.clone(),
            days_left,
        }
    }
}

/// A secret field, shown because someone asked to see it.
///
/// The one wire value that carries a secret. It crosses only on that request
/// and is dropped from the page when the card is closed, hidden again, or the
/// vault locked. For a one-time password key it is the current code, never
/// the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Revealed {
    /// What the field holds.
    pub value: String,
}

/// What a Copy put on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Copied {
    /// Which field it was, or `text` for a note, or `code` for a one-time
    /// password.
    pub what: String,
    /// Seconds until it is taken back off.
    #[ts(type = "number")]
    pub seconds: u64,
}

/// How this window treats what leaves the vault: what the status line says and
/// what the settings offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    /// Seconds a copy stays on the clipboard: what the open vault says, which
    /// the command line obeys too.
    #[ts(type = "number")]
    pub clipboard_seconds: u64,
    /// Seconds the vault stays unlocked with nobody using it: what the open
    /// vault says, which `sefy agent` obeys too.
    #[ts(type = "number")]
    pub lock_after_seconds: u64,
    /// What the clipboard's time can be set to.
    #[ts(type = "number[]")]
    pub clipboard_choices: Vec<u64>,
    /// What the vault's time can be set to.
    #[ts(type = "number[]")]
    pub lock_after_choices: Vec<u64>,
    /// The quick search shortcut.
    pub quick: Shortcut,
}

/// The quick search shortcut, and whether it is in force.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Shortcut {
    /// The keys, as a person presses them: `Ctrl+Alt+Space`.
    pub keys: String,
    /// Whether the system gave it to sefy; another program can hold it.
    pub active: bool,
}

/// One version of an item's contents, as `sefy history` lists it: named
/// secrets, never filled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct VersionRow {
    /// Its number, the one `sefy history` and `sefy restore` go by.
    #[ts(type = "number")]
    pub number: usize,
    /// Its identity, which is how the window asks about it.
    pub uuid: String,
    /// When these contents were written, in Unix seconds.
    #[ts(type = "number")]
    pub made_at: i64,
    /// The machine they were written on, when it was known.
    pub device: Option<String>,
    /// Whether this version lost a conflict in a merge and was kept.
    pub conflict: bool,
    /// Whether these are the item's contents now.
    pub current: bool,
    /// The fields this version changed from the one before it. Names only.
    pub changed: Vec<String>,
    /// What it held.
    pub body: CardBody,
}

/// The transports on this machine, and what sefy goes by when using them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Transports {
    /// Everything found, usable or not.
    pub found: Vec<Transport>,
    /// The directory a transport is put in, when the system has one.
    pub directory: Option<String>,
    /// What the remote copy is called.
    pub remote: String,
}

/// One transport, whether or not it can be used. A broken one is listed with
/// the reason: silence would read as one that was never installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Transport {
    /// Its short name, the one `--transport` goes by.
    pub name: String,
    /// Its own version, when it could say.
    pub version: Option<String>,
    /// The protocol it speaks, when it could say.
    #[ts(type = "number | null")]
    pub protocol: Option<u32>,
    /// What it can do: `push`, `pull`.
    pub operations: Vec<String>,
    /// Why it cannot be used, if it cannot.
    pub problem: Option<String>,
}

impl Transport {
    /// The window's view of a plugin the core found.
    pub fn of(plugin: &sefy_core::Plugin) -> Self {
        let manifest = plugin.manifest.as_ref();
        Self {
            name: plugin.name().to_owned(),
            version: manifest.map(|manifest| manifest.version.clone()),
            protocol: manifest.map(|manifest| manifest.protocol_version),
            operations: manifest
                .map(|manifest| {
                    manifest
                        .operations
                        .iter()
                        .map(|operation| operation.as_str().to_owned())
                        .collect()
                })
                .unwrap_or_default(),
            problem: (!plugin.usable).then(|| {
                plugin
                    .reason
                    .clone()
                    .unwrap_or_else(|| "it is not usable".to_owned())
            }),
        }
    }
}

/// What the last transfer in this window did. Kept in memory until the vault
/// locks and nowhere else: it has titles in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    /// When it happened, in Unix seconds.
    #[ts(type = "number")]
    pub at: i64,
    /// Through which transport.
    pub transport: String,
    /// `sync` or `push`.
    pub operation: String,
    /// Items the other copy had and this one did not.
    pub added: Vec<MergedRow>,
    /// Items whose contents were taken from the other copy.
    pub updated: Vec<MergedRow>,
    /// Items changed on both sides: both versions are kept, in the item's
    /// history.
    pub conflicts: Vec<ConflictRow>,
    /// Items that were already the same, or already ahead here.
    #[ts(type = "number")]
    pub unchanged: usize,
    /// Earlier versions brought across into items' histories.
    #[ts(type = "number")]
    pub versions: usize,
    /// Items of a kind this build does not know, left where they were.
    #[ts(type = "number")]
    pub unsupported: usize,
    /// Where the vault as it was is kept, when the merge changed anything.
    pub kept_copy: Option<String>,
    /// What the transport itself had to say.
    pub said: Vec<String>,
    /// Why the result could not be sent back up, if it could not: what came
    /// down is in the vault all the same.
    pub send_failure: Option<String>,
}

impl Transfer {
    /// A transfer that moved nothing into the vault, yet.
    pub fn new(at: i64, transport: &str, operation: &str) -> Self {
        Self {
            at,
            transport: transport.to_owned(),
            operation: operation.to_owned(),
            added: Vec::new(),
            updated: Vec::new(),
            conflicts: Vec::new(),
            unchanged: 0,
            versions: 0,
            unsupported: 0,
            kept_copy: None,
            said: Vec::new(),
            send_failure: None,
        }
    }

    /// Takes in what a merge did.
    pub fn with_merge(mut self, report: &sefy_core::MergeReport) -> Self {
        let rows = |items: &[sefy_core::MergedItem]| {
            items
                .iter()
                .map(|item| MergedRow {
                    id: item.id,
                    title: item.title.clone(),
                })
                .collect::<Vec<_>>()
        };
        self.added = rows(&report.added);
        self.updated = rows(&report.updated);
        self.conflicts = report
            .conflicts
            .iter()
            .map(|conflict| ConflictRow {
                id: conflict.id,
                title: conflict.title.clone(),
                other_is_current: conflict.current == sefy_core::Side::There,
            })
            .collect();
        self.unchanged = report.unchanged;
        self.versions = report.versions;
        self.unsupported = report.unsupported;
        self.kept_copy = report
            .kept_copy
            .as_ref()
            .map(|path| path.display().to_string());
        self
    }
}

/// An item a merge brought across or changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MergedRow {
    /// The item, in this vault.
    #[ts(type = "number")]
    pub id: i64,
    /// What it is called now.
    pub title: String,
}

/// An item changed on both sides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConflictRow {
    /// The item, in this vault.
    #[ts(type = "number")]
    pub id: i64,
    /// What it is called now.
    pub title: String,
    /// Whether the other copy's contents are the current ones, and this one's
    /// are what went to the history.
    pub other_is_current: bool,
}

/// How hard a new password would be to guess, on zxcvbn's scale: what the
/// master password dialog draws. Worked out here, offline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct PasswordStrength {
    /// From 0, guessable in moments, to 4.
    pub score: u8,
    /// What makes it weak, when something can be named.
    pub warning: Option<String>,
}

/// What changing the master password did beyond the vault itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PasswordChanged {
    /// Copies beside the vault, sealed under the new password as well.
    #[ts(type = "number")]
    pub resealed: usize,
    /// Copies the old password did not open, left as they were.
    pub left: Vec<String>,
    /// Whether an agent holding the old password was locked.
    pub agent_locked: bool,
}

/// What a wipe would destroy, for the person about to ask for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct WipePreview {
    /// The vault's own name, which is what has to be typed back.
    pub name: String,
    /// The directory they are all in.
    pub directory: String,
    /// Every file, the vault first.
    pub parts: Vec<WipePart>,
    /// The transport the vault last went through: the copy there is out of
    /// reach.
    pub remote: Option<String>,
}

/// One file a wipe destroys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct WipePart {
    /// Its name; the directory is said once.
    pub name: String,
    /// What it is to the vault, in words.
    pub role: String,
    /// How large it is, when the file system says.
    #[ts(type = "number | null")]
    pub size: Option<u64>,
}

impl WipePart {
    /// The window's view of a part the core listed.
    pub fn of(part: &sefy_core::wipe::Part) -> Self {
        use sefy_core::wipe::Role;
        Self {
            name: file_name(&part.path),
            role: match part.role {
                Role::Vault => "the vault".to_owned(),
                Role::Copy(1) => "the most recent copy".to_owned(),
                Role::Copy(number) => format!("copy {number}"),
                Role::Unfinished => "a write that never finished".to_owned(),
            },
            size: part.size,
        }
    }
}

/// What a wipe did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct WipeOutcome {
    /// Files overwritten and deleted.
    #[ts(type = "number")]
    pub destroyed: usize,
    /// Files something else deleted before the wipe reached them.
    #[ts(type = "number")]
    pub vanished: usize,
    /// Files still on disk, each with the reason.
    pub failed: Vec<String>,
    /// The transport the vault last went through, whose copy was not touched.
    pub remote: Option<String>,
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
    /// The password did not open the copy at the remote, which can be under
    /// another one: the page asks for it.
    RemoteRefused,
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
