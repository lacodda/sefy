//! Password-based authenticated encryption for the vault blob.
//!
//! A master password is stretched with Argon2id into a 256-bit key, which
//! XChaCha20-Poly1305 uses to seal the serialized database. Nothing here is
//! home-grown: the primitives come from audited RustCrypto crates, and every
//! salt and nonce is freshly sampled from the operating system.

use crate::error::{Error, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use zeroize::{Zeroize, Zeroizing};

/// Length of the Argon2id salt stored in front of the ciphertext.
pub const SALT_LEN: usize = 16;
/// Length of the XChaCha20-Poly1305 nonce stored after the salt.
pub const NONCE_LEN: usize = 24;
/// Length of the Poly1305 authentication tag appended to the ciphertext.
pub const TAG_LEN: usize = 16;
/// Length of the derived symmetric key.
const KEY_LEN: usize = 32;

/// Argon2id cost parameters for format version 1.
///
/// They are compiled in rather than stored in the file: a parameter block in
/// the clear would be exactly the kind of recognizable header the format exists
/// to avoid. Changing these means minting a new format version.
const ARGON2_MEMORY_KIB: u32 = 64 * 1024; // 64 MiB
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_PARALLELISM: u32 = 4;

/// A derived encryption key, wiped from memory when dropped.
pub struct DerivedKey(Zeroizing<[u8; KEY_LEN]>);

impl DerivedKey {
    /// Stretches `password` into a key using the salt of an existing vault.
    pub fn derive(password: &[u8], salt: &[u8; SALT_LEN]) -> Result<Self> {
        let params = Params::new(
            ARGON2_MEMORY_KIB,
            ARGON2_ITERATIONS,
            ARGON2_PARALLELISM,
            Some(KEY_LEN),
        )
        .map_err(|_| Error::KeyDerivation)?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        argon2
            .hash_password_into(password, salt, key.as_mut())
            .map_err(|_| Error::KeyDerivation)?;
        Ok(Self(key))
    }

    fn cipher(&self) -> Result<XChaCha20Poly1305> {
        let key = Key::try_from(self.0.as_ref()).map_err(|_| Error::KeyDerivation)?;
        Ok(XChaCha20Poly1305::new(&key))
    }

    /// Seals `plaintext` under a freshly sampled nonce.
    ///
    /// Returns the nonce alongside the ciphertext; the caller stores both.
    pub fn seal(&self, plaintext: &[u8]) -> Result<([u8; NONCE_LEN], Vec<u8>)> {
        let nonce_bytes = random_bytes::<NONCE_LEN>()?;
        let nonce = XNonce::try_from(&nonce_bytes[..]).map_err(|_| Error::KeyDerivation)?;
        let ciphertext = self
            .cipher()?
            .encrypt(&nonce, plaintext)
            .map_err(|_| Error::KeyDerivation)?;
        Ok((nonce_bytes, ciphertext))
    }

    /// Opens a sealed blob, verifying its authentication tag.
    ///
    /// A wrong password and a corrupted or unrelated file both fail here, and
    /// both surface as [`Error::WrongPasswordOrNotAVault`] — the tag check
    /// cannot tell them apart, and neither should the error.
    pub fn open(
        &self,
        nonce_bytes: &[u8; NONCE_LEN],
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>> {
        let nonce = XNonce::try_from(&nonce_bytes[..]).map_err(|_| Error::KeyDerivation)?;
        let plaintext = self
            .cipher()?
            .decrypt(&nonce, ciphertext)
            .map_err(|_| Error::WrongPasswordOrNotAVault)?;
        Ok(Zeroizing::new(plaintext))
    }
}

impl std::fmt::Debug for DerivedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DerivedKey(<redacted>)")
    }
}

/// Samples `N` bytes from the operating system random source.
pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    if getrandom::fill(&mut buf).is_err() {
        buf.zeroize();
        return Err(Error::Random);
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salt() -> [u8; SALT_LEN] {
        [9u8; SALT_LEN]
    }

    #[test]
    fn seal_open_roundtrip() {
        let key = DerivedKey::derive(b"correct horse", &salt()).unwrap();
        let (nonce, ciphertext) = key.seal(b"attack at dawn").unwrap();
        let opened = key.open(&nonce, &ciphertext).unwrap();
        assert_eq!(opened.as_slice(), b"attack at dawn");
    }

    #[test]
    fn wrong_password_fails_to_open() {
        let key = DerivedKey::derive(b"correct horse", &salt()).unwrap();
        let (nonce, ciphertext) = key.seal(b"attack at dawn").unwrap();

        let other = DerivedKey::derive(b"wrong horse", &salt()).unwrap();
        assert!(matches!(
            other.open(&nonce, &ciphertext),
            Err(Error::WrongPasswordOrNotAVault)
        ));
    }

    #[test]
    fn tampered_ciphertext_fails_to_open() {
        let key = DerivedKey::derive(b"pw", &salt()).unwrap();
        let (nonce, mut ciphertext) = key.seal(b"payload").unwrap();
        ciphertext[0] ^= 0x01;
        assert!(matches!(
            key.open(&nonce, &ciphertext),
            Err(Error::WrongPasswordOrNotAVault)
        ));
    }

    #[test]
    fn same_plaintext_seals_to_different_ciphertexts() {
        let key = DerivedKey::derive(b"pw", &salt()).unwrap();
        let (first_nonce, first) = key.seal(b"payload").unwrap();
        let (second_nonce, second) = key.seal(b"payload").unwrap();
        assert_ne!(first_nonce, second_nonce);
        assert_ne!(first, second);
    }

    #[test]
    fn ciphertext_carries_the_authentication_tag() {
        let key = DerivedKey::derive(b"pw", &salt()).unwrap();
        let (_, ciphertext) = key.seal(b"payload").unwrap();
        assert_eq!(ciphertext.len(), b"payload".len() + TAG_LEN);
    }
}
