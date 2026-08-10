//! On-disk layout of a vault file.
//!
//! ```text
//! salt (16 B) ‖ nonce (24 B) ‖ ciphertext (SQLite bytes + Poly1305 tag)
//! ```
//!
//! There is no magic number, no header and no extension convention: salt and
//! nonce are uniformly random, and everything after them is indistinguishable
//! from random to anyone without the password. The format version lives inside
//! the ciphertext — a version byte in the clear would be a signature.
//!
//! The plaintext under the seal is `version (1 B) ‖ serialized SQLite database`.

use crate::crypto::{DerivedKey, NONCE_LEN, SALT_LEN, TAG_LEN};
use crate::error::{Error, Result};
use zeroize::Zeroizing;

/// Format version this build writes.
pub const FORMAT_VERSION: u8 = 1;

/// Smallest possible vault file: salt, nonce, tag and a version byte.
const MIN_FILE_LEN: usize = SALT_LEN + NONCE_LEN + TAG_LEN + 1;

/// Encrypts a serialized database into the bytes of a vault file.
pub fn encode(password: &[u8], database: &[u8]) -> Result<Vec<u8>> {
    let salt = crate::crypto::random_bytes::<SALT_LEN>()?;
    let key = DerivedKey::derive(password, &salt)?;

    let mut plaintext = Zeroizing::new(Vec::with_capacity(database.len() + 1));
    plaintext.push(FORMAT_VERSION);
    plaintext.extend_from_slice(database);

    let (nonce, ciphertext) = key.seal(&plaintext)?;

    let mut file = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    file.extend_from_slice(&salt);
    file.extend_from_slice(&nonce);
    file.extend_from_slice(&ciphertext);
    Ok(file)
}

/// Decrypts the bytes of a vault file back into a serialized database.
pub fn decode(password: &[u8], file: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if file.len() < MIN_FILE_LEN {
        return Err(Error::TooSmall);
    }

    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&file[..SALT_LEN]);
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&file[SALT_LEN..SALT_LEN + NONCE_LEN]);
    let ciphertext = &file[SALT_LEN + NONCE_LEN..];

    let key = DerivedKey::derive(password, &salt)?;
    let plaintext = key.open(&nonce, ciphertext)?;

    match plaintext.first() {
        Some(&FORMAT_VERSION) => {}
        Some(&other) => return Err(Error::UnsupportedFormat(other)),
        // An empty plaintext cannot come out of `encode`, but a valid tag over
        // zero bytes is still conceivable input.
        None => return Err(Error::WrongPasswordOrNotAVault),
    }

    Ok(Zeroizing::new(plaintext[1..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let file = encode(b"pw", b"SQLite format 3\0rest").unwrap();
        let database = decode(b"pw", &file).unwrap();
        assert_eq!(database.as_slice(), b"SQLite format 3\0rest");
    }

    #[test]
    fn wrong_password_is_rejected() {
        let file = encode(b"pw", b"payload").unwrap();
        assert!(matches!(
            decode(b"other", &file),
            Err(Error::WrongPasswordOrNotAVault)
        ));
    }

    #[test]
    fn truncated_file_is_rejected() {
        let file = encode(b"pw", b"payload").unwrap();
        assert!(matches!(
            decode(b"pw", &file[..MIN_FILE_LEN - 1]),
            Err(Error::TooSmall)
        ));
        assert!(matches!(
            decode(b"pw", &file[..file.len() - 1]),
            Err(Error::WrongPasswordOrNotAVault)
        ));
    }

    #[test]
    fn corrupted_file_is_rejected() {
        let mut file = encode(b"pw", b"payload").unwrap();
        let last = file.len() - 1;
        file[last] ^= 0xff;
        assert!(matches!(
            decode(b"pw", &file),
            Err(Error::WrongPasswordOrNotAVault)
        ));
    }

    #[test]
    fn file_starts_with_no_recognizable_signature() {
        // Two vaults with identical content and password must share no prefix:
        // salt and nonce are freshly random every write.
        let first = encode(b"pw", b"payload").unwrap();
        let second = encode(b"pw", b"payload").unwrap();
        assert_ne!(
            first[..SALT_LEN + NONCE_LEN],
            second[..SALT_LEN + NONCE_LEN]
        );
        assert_ne!(first, second);
    }

    #[test]
    fn plaintext_database_bytes_are_not_visible_in_the_file() {
        let database = b"SQLite format 3\0this must never appear in the open";
        let file = encode(b"pw", database).unwrap();
        assert!(
            !file
                .windows(database.len())
                .any(|window| window == database.as_slice())
        );
    }
}
