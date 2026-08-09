//! What can go wrong, phrased so that no message carries secret material.

use std::path::PathBuf;

/// Result alias used across the core library.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong while working with a vault.
///
/// Error messages never contain secret material: no passwords, no item
/// contents, no attachment bytes.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The password did not decrypt the vault, or the file is not a vault at
    /// all. These two cases are indistinguishable by design: an unauthenticated
    /// blob carries nothing to tell them apart.
    #[error("wrong password or not a vault file")]
    WrongPasswordOrNotAVault,

    /// The file is too short to hold a salt, a nonce and an authentication tag.
    #[error("file is too small to be a vault")]
    TooSmall,

    /// The decrypted payload declares a format version this build cannot read.
    #[error("unsupported vault format version {0}; upgrade sefy")]
    UnsupportedFormat(u8),

    /// The vault file exists and would be overwritten.
    #[error("vault already exists: {0}")]
    AlreadyExists(PathBuf),

    /// The requested item is not in the vault.
    #[error("no item with id {0}")]
    ItemNotFound(i64),

    /// Nothing in the vault matches what the user asked for.
    #[error("nothing matches {0:?}")]
    NotFound(String),

    /// More than one item matches, so the caller has to narrow it down.
    ///
    /// The candidates travel with the error to be shown to the user; they carry
    /// item titles, so treat them as vault contents rather than log material.
    #[error("{} items match {reference:?}", candidates.len())]
    Ambiguous {
        /// What the user typed.
        reference: String,
        /// Every item it could have meant.
        candidates: Vec<crate::model::ItemSummary>,
    },

    /// The requested item does not hold the kind of payload asked for.
    #[error("item {id} is a {actual}, not a {expected}")]
    ItemKindMismatch {
        /// Item that was addressed.
        id: i64,
        /// Kind the item actually has.
        actual: &'static str,
        /// Kind the caller asked for.
        expected: &'static str,
    },

    /// The export file declares a version this build cannot read.
    #[error("unsupported export format version {0}; upgrade sefy")]
    UnsupportedExport(u32),

    /// The export file is not the JSON an export is supposed to be.
    #[error("this is not a sefy export: {0}")]
    UnreadableExport(String),

    /// An entry in an export is missing a field or holds something unusable.
    #[error("item {index} in the export is malformed: {reason}")]
    MalformedExport {
        /// Position of the offending entry, counting from zero.
        index: usize,
        /// What is wrong with it, without quoting the item's contents.
        reason: String,
    },

    /// Key derivation failed (only on absurd parameters or allocation failure).
    #[error("key derivation failed")]
    KeyDerivation,

    /// The operating system refused to provide random bytes.
    #[error("system random number generator unavailable")]
    Random,

    /// A filesystem operation failed.
    #[error("{context}: {source}")]
    Io {
        /// What was being attempted.
        context: String,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// The embedded database rejected an operation.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

impl Error {
    /// Attaches a human-readable context to an I/O failure.
    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
