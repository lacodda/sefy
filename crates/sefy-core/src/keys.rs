//! Who turns a salt into a key.
//!
//! A vault's key is the password stretched with the salt at the front of its
//! file, and every write samples a new salt. So a key is good for one version
//! of one file, and anything that keeps a vault usable across writes needs the
//! password, not a key. [`KeySource`] is that: something holding the password
//! elsewhere, which hands over keys and never the password.
//!
//! An agent is one ([`crate::agent`]). The file checks its answers: a key that
//! does not fit fails the authentication tag, and [`KeySource::fresh`] has to
//! prove it still holds the password that opened the vault before anything is
//! sealed under what it gives.

use crate::crypto::{DerivedKey, SALT_LEN};
use crate::error::Result;
use crate::format;
use std::path::Path;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Something that holds a master password and gives out keys derived from it.
pub trait KeySource: Send + Sync {
    /// The key for a file with this salt, or [`crate::Error::KeysGone`] if
    /// there is no longer a password to derive it from.
    fn key_for(&self, salt: &[u8; SALT_LEN]) -> Result<DerivedKey>;

    /// A key for a salt nothing has used yet, to seal a new version of a file
    /// with; `opened_with` is the salt of the version it was read from, and
    /// the answer carries the key for that salt as well.
    fn fresh(&self, opened_with: &[u8; SALT_LEN]) -> Result<FreshKey>;
}

impl<T: KeySource + ?Sized> KeySource for Arc<T> {
    fn key_for(&self, salt: &[u8; SALT_LEN]) -> Result<DerivedKey> {
        (**self).key_for(salt)
    }

    fn fresh(&self, opened_with: &[u8; SALT_LEN]) -> Result<FreshKey> {
        (**self).fresh(opened_with)
    }
}

/// What [`KeySource::fresh`] answers.
pub struct FreshKey {
    /// A salt sampled for this write.
    pub salt: [u8; SALT_LEN],
    /// The key for it.
    pub key: DerivedKey,
    /// The key for the salt the vault was opened with. The same bytes as the
    /// key that opened it, if and only if this is still the same password;
    /// asked for in one answer, so another password cannot come between.
    pub proof: DerivedKey,
}

/// What opens a sealed file: a password in hand, or a source that holds one.
#[derive(Clone, Copy)]
pub enum Opener<'a> {
    /// The password itself.
    Password(&'a [u8]),
    /// Whoever holds it.
    Source(&'a dyn KeySource),
}

impl Opener<'_> {
    /// Opens the bytes of a vault file.
    pub(crate) fn open(&self, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        match self {
            Self::Password(password) => format::decode(password, sealed),
            Self::Source(source) => {
                let key = source.key_for(&format::salt_of(sealed)?)?;
                format::open_under(&key, sealed)
            }
        }
    }

    /// Whether this opens the file at `path`.
    pub fn opens(&self, path: &Path) -> bool {
        std::fs::read(path)
            .map(|sealed| self.open(&sealed).is_ok())
            .unwrap_or(false)
    }
}

impl<'a> From<&'a [u8]> for Opener<'a> {
    fn from(password: &'a [u8]) -> Self {
        Self::Password(password)
    }
}

impl<'a, const N: usize> From<&'a [u8; N]> for Opener<'a> {
    fn from(password: &'a [u8; N]) -> Self {
        Self::Password(password)
    }
}

impl<'a> From<&'a dyn KeySource> for Opener<'a> {
    fn from(source: &'a dyn KeySource) -> Self {
        Self::Source(source)
    }
}
