//! Destroying a vault on purpose: its file, the copies kept beside it, and any
//! write a crash left unfinished.
//!
//! A vault on disk is more than one file. Beside the vault itself sefy keeps
//! [copies](crate::copies) before a change from elsewhere, and a save that
//! crashed halfway leaves its temporary file behind. Each of them is the
//! sealed vault — under the current password or an earlier one — so wiping
//! the vault and leaving them would be wiping nothing. A wipe takes every one
//! of these names, and only these: they are what sefy writes beside a vault,
//! and nothing else there is sefy's.
//!
//! # What an overwrite reaches
//!
//! Each file is overwritten in place with random bytes, synced to the disk,
//! and deleted. Random rather than zeros: what is left where the vault was
//! looks like what was there — bytes with no pattern — instead of announcing
//! that something was wiped. One pass: on drives built this century a second
//! one finds nothing the first left (NIST SP 800-88).
//!
//! That reaches the blocks the file system shows for the file, and no others.
//! An SSD writes a changed block to a fresh place and keeps the old one until
//! it needs the room; a copy-on-write file system (APFS, btrfs, ZFS, ReFS)
//! does the same a level up; a snapshot, a backup or a synced folder's version
//! history holds whatever it took. What survives there is the sealed vault,
//! which still needs the password — but a wipe cannot promise it is gone.

use crate::copies;
use crate::error::{Error, Result};
use crate::vault::{SyncStamp, temporary_path};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// How much is overwritten at a time.
const CHUNK: usize = 64 * 1024;

/// What a file beside a vault is to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The vault itself.
    Vault,
    /// A copy kept before a change came in; 1 is the most recent.
    Copy(usize),
    /// A write that never finished: the sealed bytes of a save that crashed
    /// before they were put in place, of the vault or of a copy.
    Unfinished,
}

/// One file a wipe destroys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// Where it is.
    pub path: PathBuf,
    /// What it is to the vault.
    pub role: Role,
    /// How large it is, if the file system says.
    pub size: Option<u64>,
}

/// What a wipe did, file by file.
#[derive(Debug, Default)]
pub struct Wiped {
    /// Overwritten and deleted.
    pub destroyed: Vec<Part>,
    /// Gone before the wipe reached them: something else deleted them.
    pub vanished: Vec<Part>,
    /// Still on disk, each with the reason. One that could not be
    /// overwritten is left as it was, so a second wipe can still overwrite
    /// it rather than find it deleted.
    pub failed: Vec<(Part, Error)>,
    /// When the vault last reached a remote, if it ever did. The copy there
    /// is out of a wipe's reach.
    pub last_sync: Option<SyncStamp>,
}

impl Wiped {
    /// Whether every file of the vault is gone.
    pub fn complete(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Every file the vault at `vault` is on disk: the vault, its copies, and the
/// unfinished writes of either. What a wipe destroys, in the order it does.
///
/// The vault comes first, being the newest state; then the copies, newest
/// first; then what crashed saves left.
pub fn parts(vault: &Path) -> Vec<Part> {
    let mut named = vec![(vault.to_path_buf(), Role::Vault)];
    named
        .extend((1..=copies::KEPT).map(|number| (copies::path(vault, number), Role::Copy(number))));
    let unfinished: Vec<(PathBuf, Role)> = named
        .iter()
        .map(|(path, _)| (temporary_path(path), Role::Unfinished))
        .collect();
    named.extend(unfinished);

    named
        .into_iter()
        .filter_map(|(path, role)| match fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => None,
            Ok(metadata) => Some(Part {
                path,
                role,
                size: Some(metadata.len()),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            // There, but the file system will not say how large. Listed all
            // the same: a name a wipe skipped would be a copy it kept.
            Err(_) => Some(Part {
                path,
                role,
                size: None,
            }),
        })
        .collect()
}

/// Destroys every part, carrying on past one that fails.
pub(crate) fn carry_out(parts: Vec<Part>, last_sync: Option<SyncStamp>) -> Wiped {
    let mut wiped = Wiped {
        last_sync,
        ..Wiped::default()
    };
    // Past a failure rather than stopping at it: when a wipe is needed, every
    // file it does destroy is one less left behind.
    for part in parts {
        match destroy(&part.path) {
            Ok(true) => wiped.destroyed.push(part),
            Ok(false) => wiped.vanished.push(part),
            Err(error) => wiped.failed.push((part, error)),
        }
    }
    wiped
}

/// Overwrites a file in place with random bytes, syncs it, and deletes it.
///
/// Returns whether there was a file to destroy. A file that cannot be
/// overwritten is not deleted: it stays as it was, where another attempt can
/// still reach it. See the [module documentation](self) for what an overwrite
/// reaches.
pub fn destroy(path: &Path) -> Result<bool> {
    match overwrite(path) {
        Ok(()) => {}
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(false);
        }
        Err(other) => return Err(other),
    }
    fs::remove_file(path).map_err(|source| {
        Error::io(
            format!("overwrote {} but cannot delete it", path.display()),
            source,
        )
    })?;
    Ok(true)
}

/// Writes random bytes over every byte of a file, where they are.
///
/// The file is opened for writing without truncating it. Truncating first —
/// what `File::create` does — hands its blocks back to the file system, and
/// the bytes written after it land wherever the file system pleases, quite
/// possibly not on the blocks that held the secret.
fn overwrite(path: &Path) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|source| {
            Error::io(
                format!("cannot open {} to overwrite it", path.display()),
                source,
            )
        })?;
    let failed = |source| Error::io(format!("cannot overwrite {}", path.display()), source);

    let length = file.metadata().map_err(failed)?.len();
    let mut noise = vec![0u8; CHUNK];
    let mut left = length;
    while left > 0 {
        let now = usize::try_from(left).map_or(CHUNK, |left| left.min(CHUNK));
        getrandom::fill(&mut noise[..now]).map_err(|_| Error::Random)?;
        file.write_all(&noise[..now]).map_err(failed)?;
        left -= now as u64;
    }
    // Until this returns, the random bytes may be sitting in a cache with the
    // old ones still on the disk.
    file.sync_all().map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
    }

    /// Bytes that are easy to tell from noise, `length` of them.
    fn pattern(length: usize) -> Vec<u8> {
        (0..length).map(|index| (index % 251) as u8).collect()
    }

    #[test]
    fn the_parts_are_the_vault_its_copies_and_unfinished_writes_in_that_order() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");
        write(&vault, b"vault");
        write(&copies::path(&vault, 1), b"one");
        write(&copies::path(&vault, 3), b"three");
        write(&temporary_path(&vault), b"crashed save");
        write(&temporary_path(&copies::path(&vault, 2)), b"crashed copy");

        let found: Vec<(PathBuf, Role)> = parts(&vault)
            .into_iter()
            .map(|part| (part.path, part.role))
            .collect();

        assert_eq!(
            found,
            vec![
                (vault.clone(), Role::Vault),
                (copies::path(&vault, 1), Role::Copy(1)),
                (copies::path(&vault, 3), Role::Copy(3)),
                (temporary_path(&vault), Role::Unfinished),
                (temporary_path(&copies::path(&vault, 2)), Role::Unfinished),
            ]
        );
    }

    #[test]
    fn a_part_says_how_large_it_is() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");
        write(&vault, &[7; 1234]);

        assert_eq!(parts(&vault)[0].size, Some(1234));
    }

    #[test]
    fn nothing_but_sefys_own_names_is_a_part() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");
        write(&vault, b"vault");
        for neighbour in ["notes.bak.4", "notes.bak.old", "notes.bak.0", "notes.txt"] {
            write(&directory.path().join(neighbour), b"mine");
        }
        // A directory under a sefy name is not a file sefy wrote.
        fs::create_dir(copies::path(&vault, 2)).unwrap();

        let found: Vec<PathBuf> = parts(&vault).into_iter().map(|part| part.path).collect();

        assert_eq!(found, vec![vault]);
    }

    #[test]
    fn destroying_a_file_overwrites_what_it_held_before_deleting_it() {
        // A second name for the same file sees its bytes after the first name
        // is gone — which is what shows the overwrite happened in place, on
        // the data itself, rather than a delete that left it to be found.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.bak");
        let witness = directory.path().join("witness");
        // Several chunks and a ragged end, so every pass of the loop counts.
        let original = pattern(3 * CHUNK + 1001);
        write(&path, &original);
        fs::hard_link(&path, &witness).unwrap();

        assert!(destroy(&path).unwrap());

        assert!(!path.exists(), "the name is gone");
        let left = fs::read(&witness).unwrap();
        assert_eq!(left.len(), original.len(), "overwritten, not truncated");
        for (block, (now, then)) in left.chunks(4096).zip(original.chunks(4096)).enumerate() {
            assert_ne!(now, then, "block {block} still holds what it did");
        }
    }

    #[test]
    fn the_overwrite_is_noise_not_a_fill() {
        // Zeros where a vault was would say a wipe happened; noise says
        // nothing the vault's own ciphertext did not.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.bak");
        let witness = directory.path().join("witness");
        write(&path, &vec![0u8; CHUNK]);
        fs::hard_link(&path, &witness).unwrap();

        destroy(&path).unwrap();

        let left = fs::read(&witness).unwrap();
        let mut seen = [false; 256];
        for byte in &left {
            seen[usize::from(*byte)] = true;
        }
        let distinct = seen.iter().filter(|seen| **seen).count();
        assert!(distinct > 200, "only {distinct} distinct byte values");
    }

    #[test]
    fn a_file_that_is_not_there_is_reported_rather_than_failed() {
        let directory = tempfile::tempdir().unwrap();

        assert!(!destroy(&directory.path().join("gone.bak")).unwrap());
    }

    #[test]
    fn an_empty_file_is_destroyed_too() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.bak");
        write(&path, b"");

        assert!(destroy(&path).unwrap());
        assert!(!path.exists());
    }

    /// Keeps a file from being written for as long as it lives, but not from
    /// being deleted: a test then sees whether a wipe deletes what it could
    /// not overwrite, rather than the platform refusing the delete for it.
    /// A read-only attribute would do just that on Windows, and hide it.
    struct Unwritable {
        #[cfg(windows)]
        _holder: fs::File,
        #[cfg(not(windows))]
        path: PathBuf,
    }

    impl Unwritable {
        #[cfg(windows)]
        fn new(path: &Path) -> Self {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_SHARE_READ | FILE_SHARE_DELETE, and no FILE_SHARE_WRITE:
            // while this handle is open, nobody opens the file for writing.
            const READ_AND_DELETE: u32 = 0x1 | 0x4;
            let holder = fs::OpenOptions::new()
                .read(true)
                .share_mode(READ_AND_DELETE)
                .open(path)
                .unwrap();
            Self { _holder: holder }
        }

        #[cfg(not(windows))]
        fn new(path: &Path) -> Self {
            // Unix refuses to open a 0444 file for writing, and still lets it
            // be unlinked from a directory that is writable.
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o444)).unwrap();
            Self {
                path: path.to_path_buf(),
            }
        }
    }

    #[cfg(not(windows))]
    impl Drop for Unwritable {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o644));
        }
    }

    #[test]
    fn a_file_that_cannot_be_overwritten_is_left_as_it_was() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.bak");
        write(&path, b"still here");
        let _unwritable = Unwritable::new(&path);

        let outcome = destroy(&path);

        assert!(outcome.is_err(), "{outcome:?}");
        assert_eq!(fs::read(&path).unwrap(), b"still here");
    }

    #[test]
    fn a_wipe_carries_on_past_a_part_it_cannot_destroy() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");
        write(&vault, b"vault");
        for number in 1..=copies::KEPT {
            write(&copies::path(&vault, number), b"copy");
        }
        let stuck = copies::path(&vault, 2);
        let _unwritable = Unwritable::new(&stuck);

        let wiped = carry_out(parts(&vault), None);

        let destroyed: Vec<Role> = wiped.destroyed.iter().map(|part| part.role).collect();
        assert_eq!(destroyed, vec![Role::Vault, Role::Copy(1), Role::Copy(3)]);
        assert_eq!(wiped.failed.len(), 1);
        assert_eq!(wiped.failed[0].0.path, stuck);
        assert!(!wiped.complete());
        assert_eq!(fs::read(&stuck).unwrap(), b"copy");
    }

    #[test]
    #[should_panic(expected = "no file of its own")]
    fn a_copy_held_in_memory_cannot_wipe_the_vault_it_came_from() {
        // A preview's scratch copy carries the real vault's path. Wiping it
        // must stop at the assertion, before the real file is touched.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.bak");
        let vault = crate::Vault::create(&path, b"password").unwrap();
        let scratch = vault.scratch_copy().unwrap();

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scratch.wipe()));

        assert!(path.exists(), "the real vault was touched");
        std::panic::resume_unwind(outcome.expect_err("the wipe went ahead"));
    }

    #[test]
    fn a_part_deleted_before_the_wipe_reached_it_is_named_as_gone() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("notes.bak");
        write(&vault, b"vault");
        write(&copies::path(&vault, 1), b"copy");
        let listed = parts(&vault);
        fs::remove_file(copies::path(&vault, 1)).unwrap();

        let wiped = carry_out(listed, None);

        assert!(wiped.complete());
        assert_eq!(wiped.vanished.len(), 1);
        assert_eq!(wiped.vanished[0].role, Role::Copy(1));
    }
}
