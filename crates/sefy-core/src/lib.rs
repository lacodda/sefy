//! Core library behind [sefy](https://github.com/lacodda/sefy): a personal
//! secret store that lives in one unremarkable file.
//!
//! A vault is a SQLite database sealed whole with XChaCha20-Poly1305 under a
//! key derived from a master password with Argon2id. The file on disk is
//! `salt ‖ nonce ‖ ciphertext` — no magic bytes, no header, no extension
//! convention, nothing to mark it as a secret store.
//!
//! # Threat model
//!
//! sefy is **inconspicuous, not deniable**. It hides secrets from a passing
//! glance, a curious file listing and a cloud-side scanner looking for known
//! formats. It does not claim to withstand forensic analysis — a
//! high-entropy headerless file is recognizable as *some* container to an
//! examiner — nor to protect anyone compelled to give up a password.
//!
//! # Example
//!
//! ```
//! use sefy_core::{ItemKind, NewItem, Payload, Query, Vault};
//! # let directory = tempfile::tempdir().unwrap();
//! # let path = directory.path().join("notes.bak");
//!
//! let mut vault = Vault::create(&path, b"master password")?;
//! let id = vault.add(
//!     NewItem::new("bank", Payload::Note { text: "vault code 1234".into() })
//!         .with_tags(["money"]),
//! )?;
//! vault.save()?;
//!
//! let reopened = Vault::open(&path, b"master password")?;
//! let found = reopened.search(&Query::all().text("bank").kind(ItemKind::Note))?;
//! assert_eq!(found[0].id, id);
//! # Ok::<(), sefy_core::Error>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod crypto;
pub mod db;
pub mod error;
pub mod exchange;
pub mod format;
pub mod model;
pub mod vault;

pub use error::{Error, Result};
pub use exchange::{EXPORT_VERSION, Export, ExportItem};
pub use format::FORMAT_VERSION;
pub use model::{Credential, Item, ItemKind, ItemSummary, NewItem, Payload, Query};
pub use vault::Vault;
