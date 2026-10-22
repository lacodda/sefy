//! The one vault the window has open, and how it gets there.
//!
//! The vault lives here, in this process, for as long as it is unlocked; the
//! page is told what it is (`VaultStatus`), never handed the database. Locking
//! drops it, which wipes the password the core holds and lets the in-memory
//! database go.

use crate::wire::{Failure, Opening, VaultStatus};
use sefy_core::Vault;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The vault the window has open, if any.
#[derive(Default)]
pub struct Session {
    vault: Mutex<Option<Vault>>,
}

impl Session {
    /// Puts an opened vault in the window, in place of whatever was open.
    pub fn hold(&self, vault: Vault) -> Result<VaultStatus, Failure> {
        let status = VaultStatus::of(&vault.status()?);
        *self.slot() = Some(vault);
        Ok(status)
    }

    /// Closes the open vault, if there is one.
    pub fn lock(&self) {
        // Taken out and dropped here rather than left to the next unlock, so
        // the password and the database are gone the moment the window says
        // the vault is locked.
        drop(self.slot().take());
    }

    /// What and where the open vault is.
    pub fn status(&self) -> Result<VaultStatus, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        Ok(VaultStatus::of(&vault.status()?))
    }

    /// Whether a vault is open.
    pub fn is_open(&self) -> bool {
        self.slot().is_some()
    }

    /// The slot, even after a panic elsewhere held the lock: what is in it is
    /// a vault or nothing, and neither is left half-changed by a panic.
    fn slot(&self) -> MutexGuard<'_, Option<Vault>> {
        self.vault.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Opens the vault at `path` — the slow part of unlocking, which stretches
/// the password on purpose and so belongs off the window's thread.
pub fn open(path: &Path, password: &[u8]) -> Result<Vault, Failure> {
    Vault::open(path, password).map_err(Failure::from)
}

/// The file the window was started on: the first argument that is not a flag,
/// as "Open with" and a terminal pass it, or else `SEFY_VAULT`, the one the
/// command line uses.
///
/// Nothing else, and nothing remembered: sefy keeps no list of the vaults it
/// has opened. A list like that, in a predictable place under the user's
/// profile, would name exactly the files the format spends all its effort
/// making unremarkable — to anyone who looks there, and to any scanner that
/// knows where sefy keeps its things.
pub fn opening(args: impl IntoIterator<Item = OsString>, vault_env: Option<OsString>) -> Opening {
    let argument = args
        .into_iter()
        .skip(1)
        .find(|arg| !arg.to_string_lossy().starts_with('-'));
    let file = argument
        .or(vault_env)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        // A path relative to wherever the launcher stood would mean another
        // file once the dialog moves elsewhere.
        .map(|path| std::path::absolute(&path).unwrap_or(path))
        .map(|path| path.display().to_string());
    Opening { file }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::FailureKind;

    const PASSWORD: &[u8] = b"correct horse battery staple";

    fn vault_in(directory: &tempfile::TempDir) -> PathBuf {
        let path = directory.path().join("notes.bak");
        let mut vault = Vault::create(&path, PASSWORD).unwrap();
        vault
            .add(sefy_core::NewItem::new(
                "bank",
                sefy_core::Payload::Note {
                    text: "1234".into(),
                },
            ))
            .unwrap();
        vault.save().unwrap();
        path
    }

    #[test]
    fn an_unlocked_vault_answers_until_it_is_locked() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();

        let status = session.hold(open(&path, PASSWORD).unwrap()).unwrap();
        assert_eq!(status.name, "notes.bak");
        assert_eq!(status.items, 1);
        assert!(session.is_open());
        assert_eq!(session.status().unwrap(), status);

        session.lock();
        assert!(!session.is_open());
        assert_eq!(session.status().unwrap_err().kind, FailureKind::Locked);
    }

    #[test]
    fn a_wrong_password_is_refused_and_leaves_nothing_open() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();

        let failure = open(&path, b"not it").unwrap_err();
        assert_eq!(failure.kind, FailureKind::Refused);
        assert!(!session.is_open());
    }

    #[test]
    fn any_other_file_is_refused_the_same_way() {
        // A file that is not a vault and a vault under another password are
        // one answer: there is nothing in either to tell them apart.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("holiday.jpg");
        std::fs::write(&path, vec![7u8; 4096]).unwrap();
        assert_eq!(
            open(&path, PASSWORD).unwrap_err().kind,
            FailureKind::Refused
        );

        let tiny = directory.path().join("tiny");
        std::fs::write(&tiny, b"x").unwrap();
        assert_eq!(
            open(&tiny, PASSWORD).unwrap_err().kind,
            FailureKind::Refused
        );
    }

    #[test]
    fn a_file_that_is_not_there_is_missing_rather_than_refused() {
        let directory = tempfile::tempdir().unwrap();
        let failure = open(&directory.path().join("gone.bak"), PASSWORD).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Missing);
    }

    #[test]
    fn unlocking_another_vault_closes_the_first() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let session = Session::default();

        session
            .hold(open(&vault_in(&first), PASSWORD).unwrap())
            .unwrap();
        let other = second.path().join("archive.dat");
        Vault::create(&other, PASSWORD).unwrap();
        session.hold(open(&other, PASSWORD).unwrap()).unwrap();

        let status = session.status().unwrap();
        assert_eq!(status.name, "archive.dat");
        assert_eq!(status.items, 0);
    }

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_file_named_at_launch_comes_before_the_environment() {
        let opening = opening(
            args(&["sefy-gui", "--some-flag", "notes.bak"]),
            Some("elsewhere.dat".into()),
        );
        let file = opening.file.unwrap();
        assert!(file.ends_with("notes.bak"), "{file}");
        assert!(
            Path::new(&file).is_absolute(),
            "{file} would mean another file once the dialog moves"
        );
    }

    #[test]
    fn without_an_argument_the_window_opens_on_sefy_vault() {
        let opening = opening(args(&["sefy-gui"]), Some("elsewhere.dat".into()));
        assert!(opening.file.unwrap().ends_with("elsewhere.dat"));
    }

    #[test]
    fn with_neither_the_person_picks_the_file() {
        assert_eq!(opening(args(&["sefy-gui"]), None).file, None);
        assert_eq!(opening(args(&["sefy-gui"]), Some("".into())).file, None);
    }
}
