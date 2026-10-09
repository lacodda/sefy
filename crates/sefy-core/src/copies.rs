//! Copies of a vault kept beside it before a change brought in from elsewhere.
//!
//! A merge, a pull, a sync and an import change many items at once, with
//! contents nobody typed on this machine. Each is built to lose nothing — what
//! a merge replaces becomes a version in the item's history — but that is a
//! property of code, and code has bugs. If one ever leaves a vault wrong, it
//! should not have cost the only copy there was.
//!
//! So before such a change reaches the disk, the file as it was is kept beside
//! the vault: `FILE.1` is the most recent, then `FILE.2` and `FILE.3`, the way
//! logrotate numbers what it rotates. Three, with no setting for it: enough to
//! reach back past a sync or two that ran after the one that went wrong, few
//! enough that old contents do not pile up.
//!
//! # What a copy is
//!
//! The sealed file, byte for byte — ciphertext under the vault's password, the
//! same thing an onlooker already finds in the vault itself. The plaintext
//! invariant is untouched. The name is the vault's own with a number after it,
//! which says "an older copy of that file" and nothing about sefy; a copy in
//! sefy's data directory would be a vault at a predictable path, the one
//! giveaway the format exists to avoid.
//!
//! Going back is replacing the vault with a copy. Nothing in sefy does that on
//! its own: which state is the right one is the person's call.

use crate::error::{Error, Result};
use crate::format;
use crate::vault::{PasswordChange, write_atomically};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How many copies are kept beside a vault.
pub const KEPT: usize = 3;

/// One copy beside a vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    /// Where it is.
    pub path: PathBuf,
    /// Its place in the rotation: 1 is the most recent.
    pub number: usize,
    /// When it was taken, if the file system says.
    pub taken: Option<SystemTime>,
}

/// Where copy `number` of the vault at `vault` lives.
pub fn path(vault: &Path, number: usize) -> PathBuf {
    let mut name = vault
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("vault"))
        .to_os_string();
    name.push(format!(".{number}"));
    vault.with_file_name(name)
}

/// The copies beside a vault, most recent first.
pub fn list(vault: &Path) -> Vec<Copy> {
    (1..=KEPT)
        .filter_map(|number| {
            let path = path(vault, number);
            let metadata = std::fs::metadata(&path).ok()?;
            metadata.is_file().then(|| Copy {
                taken: metadata.modified().ok(),
                path,
                number,
            })
        })
        .collect()
}

/// Keeps the vault's file as it is now as copy 1, moving the older copies one
/// place down and letting the oldest go.
///
/// Returns where the copy went, or nothing when there was no file to keep — a
/// vault whose file is gone has nothing on disk a change could cost.
///
/// The copy is written the way the vault is, through a temporary file and a
/// rename: a crash in the middle leaves the old copy or the new one, never half
/// of either.
pub fn keep(vault: &Path) -> Result<Option<PathBuf>> {
    let current = match std::fs::read(vault) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(Error::io(
                format!("cannot read {} to keep a copy of it", vault.display()),
                source,
            ));
        }
    };

    // Oldest first, so nothing is overwritten before it has moved on.
    for number in (1..KEPT).rev() {
        let from = path(vault, number);
        if !from.exists() {
            continue;
        }
        let to = path(vault, number + 1);
        std::fs::rename(&from, &to).map_err(|source| {
            Error::io(
                format!("cannot move {} to {}", from.display(), to.display()),
                source,
            )
        })?;
    }

    let newest = path(vault, 1);
    write_atomically(&newest, &current)?;
    Ok(Some(newest))
}

/// Seals every copy beside a vault under a new password.
///
/// A copy that does not open under `old` is left exactly as it was and named
/// in the result: it may predate an earlier change of password, or not be this
/// vault's at all, and either way it is not this function's to rewrite.
///
/// Each copy keeps the time it was taken. Re-sealing is not taking a copy, and
/// a listing that said every copy was made a moment ago would hide which state
/// each one holds.
pub(crate) fn reseal(vault: &Path, old: &[u8], new: &[u8]) -> Result<PasswordChange> {
    let mut change = PasswordChange::default();

    for copy in list(vault) {
        let sealed = std::fs::read(&copy.path)
            .map_err(|source| Error::io(format!("cannot read {}", copy.path.display()), source))?;
        let database = match format::decode(old, &sealed) {
            Ok(database) => database,
            Err(
                Error::WrongPasswordOrNotAVault | Error::TooSmall | Error::UnsupportedFormat(_),
            ) => {
                change.left.push(copy.path);
                continue;
            }
            Err(other) => return Err(other),
        };

        write_atomically(&copy.path, &format::encode(new, &database)?)?;
        if let Some(taken) = copy.taken {
            // Best effort: a file system that will not take a time back still
            // holds a correctly sealed copy, which is what matters.
            let _ = std::fs::File::options()
                .write(true)
                .open(&copy.path)
                .and_then(|file| file.set_modified(taken));
        }
        change.resealed += 1;
    }

    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn a_copy_is_named_after_the_vault_with_a_number() {
        let vault = Path::new("backups").join("notes.bak");

        assert_eq!(path(&vault, 1), Path::new("backups").join("notes.bak.1"));
        assert_eq!(path(&vault, 3), Path::new("backups").join("notes.bak.3"));
    }

    #[test]
    fn the_name_of_a_copy_says_nothing_about_what_wrote_it() {
        let vault = Path::new("notes.bak");

        for number in 1..=KEPT {
            let name = path(vault, number).display().to_string().to_lowercase();
            assert!(!name.contains("sefy"), "{name}");
        }
    }

    #[test]
    fn the_first_copy_is_the_file_byte_for_byte() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");
        std::fs::write(&vault, [0u8, 255, 7, 13, 10]).unwrap();

        let kept = keep(&vault).unwrap().expect("there was a file to keep");

        assert_eq!(kept, path(&vault, 1));
        assert_eq!(std::fs::read(&kept).unwrap(), vec![0u8, 255, 7, 13, 10]);
        assert_eq!(
            std::fs::read(&vault).unwrap(),
            vec![0u8, 255, 7, 13, 10],
            "keeping a copy leaves the vault alone"
        );
    }

    #[test]
    fn copies_rotate_and_the_oldest_goes() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");

        for state in ["first", "second", "third", "fourth", "fifth"] {
            write(&vault, state);
            keep(&vault).unwrap();
        }

        assert_eq!(read(&path(&vault, 1)), "fifth");
        assert_eq!(read(&path(&vault, 2)), "fourth");
        assert_eq!(read(&path(&vault, 3)), "third");
        assert!(
            !path(&vault, KEPT + 1).exists(),
            "no copy is kept past the last place"
        );
    }

    #[test]
    fn a_gap_in_the_rotation_is_not_filled_with_a_wrong_copy() {
        // Someone deleted the second copy by hand. The first moves down into
        // its place, and the third stays the third: a number is a promise
        // about order, not a slot to fill with whatever is at hand.
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");
        write(&path(&vault, 1), "older");
        write(&path(&vault, 3), "oldest");
        write(&vault, "now");

        keep(&vault).unwrap();

        assert_eq!(read(&path(&vault, 1)), "now");
        assert_eq!(read(&path(&vault, 2)), "older");
        assert_eq!(read(&path(&vault, 3)), "oldest");
    }

    #[test]
    fn a_vault_with_no_file_has_nothing_to_keep() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("gone.bak");

        assert_eq!(keep(&vault).unwrap(), None);
        assert!(list(&vault).is_empty());
    }

    #[test]
    fn the_listing_runs_most_recent_first_and_skips_what_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");
        write(&path(&vault, 3), "oldest");
        write(&path(&vault, 1), "newest");

        let numbers: Vec<usize> = list(&vault).iter().map(|copy| copy.number).collect();

        assert_eq!(numbers, vec![1, 3]);
    }

    #[test]
    fn nothing_but_the_numbered_copies_is_touched() {
        // A file beside the vault that only looks related must survive any
        // number of rotations: sefy owns three names here and no others.
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");
        let neighbour = directory.path().join("notes.bak.old");
        write(&neighbour, "mine");

        for state in ["a", "b", "c", "d"] {
            write(&vault, state);
            keep(&vault).unwrap();
        }

        assert_eq!(read(&neighbour), "mine");
    }
}
