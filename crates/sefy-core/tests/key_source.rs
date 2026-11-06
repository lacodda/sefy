//! A vault opened with keys from something else that holds the password.
//!
//! The rules these hold: every write still gets a salt of its own, the file
//! stays one the password alone opens, and nothing is sealed under a key from
//! a source that cannot prove it holds the password the vault was opened with.

use sefy_core::crypto::{DerivedKey, SALT_LEN, random_bytes};
use sefy_core::{Error, FreshKey, KeySource, NewItem, Opener, Payload, Vault};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PASSWORD: &[u8] = b"correct horse battery staple";

/// A source holding a password that can be swapped or taken away, as an agent
/// locking or being replaced would.
struct Source(Mutex<Option<Vec<u8>>>);

impl Source {
    fn holding(password: &[u8]) -> Arc<Self> {
        Arc::new(Self(Mutex::new(Some(password.to_vec()))))
    }

    fn set(&self, password: Option<&[u8]>) {
        *self.0.lock().unwrap() = password.map(<[u8]>::to_vec);
    }

    fn password(&self) -> sefy_core::Result<Vec<u8>> {
        self.0.lock().unwrap().clone().ok_or(Error::KeysGone)
    }
}

impl KeySource for Source {
    fn key_for(&self, salt: &[u8; SALT_LEN]) -> sefy_core::Result<DerivedKey> {
        DerivedKey::derive(&self.password()?, salt)
    }

    fn fresh(&self, opened_with: &[u8; SALT_LEN]) -> sefy_core::Result<FreshKey> {
        let password = self.password()?;
        let salt = random_bytes::<SALT_LEN>()?;
        Ok(FreshKey {
            salt,
            key: DerivedKey::derive(&password, &salt)?,
            proof: DerivedKey::derive(&password, opened_with)?,
        })
    }
}

fn note(title: &str) -> NewItem {
    NewItem::new(
        title,
        Payload::Note {
            text: "synthetic".into(),
        },
    )
}

fn vault_in(directory: &Path) -> PathBuf {
    let path = directory.join("notes.bak");
    let mut vault = Vault::create(&path, PASSWORD).unwrap();
    vault.add(note("first")).unwrap();
    vault.save().unwrap();
    path
}

fn titles(vault: &Vault) -> Vec<String> {
    vault
        .list()
        .unwrap()
        .into_iter()
        .map(|item| item.title)
        .collect()
}

#[test]
fn what_is_written_through_a_source_is_a_file_the_password_opens() {
    let directory = tempfile::tempdir().unwrap();
    let path = vault_in(directory.path());

    let mut vault = Vault::open_through(&path, Source::holding(PASSWORD)).unwrap();
    assert!(!vault.holds_password());
    assert_eq!(titles(&vault), ["first"]);
    vault.add(note("second")).unwrap();
    vault.save().unwrap();

    // Nothing about the file says how it was written.
    let reopened = Vault::open(&path, PASSWORD).unwrap();
    assert!(reopened.holds_password());
    assert_eq!(titles(&reopened), ["second", "first"]);
}

#[test]
fn every_write_through_a_source_gets_a_salt_of_its_own() {
    // ADR-0001: two versions of a file share no prefix. A source that kept
    // handing out one key would be the easy way to lose that, and sixteen
    // bytes every version has in common are a signature to anyone holding two
    // of them - which a git remote is.
    let directory = tempfile::tempdir().unwrap();
    let path = vault_in(directory.path());
    let vault = Vault::open_through(&path, Source::holding(PASSWORD)).unwrap();

    let mut seen = vec![std::fs::read(&path).unwrap()[..SALT_LEN].to_vec()];
    for _ in 0..3 {
        vault.save().unwrap();
        let salt = std::fs::read(&path).unwrap()[..SALT_LEN].to_vec();
        assert!(!seen.contains(&salt), "a salt came back: {salt:?}");
        seen.push(salt);
    }
}

#[test]
fn a_source_with_another_password_opens_nothing_and_looks_like_a_wrong_password() {
    let directory = tempfile::tempdir().unwrap();
    let path = vault_in(directory.path());

    assert!(matches!(
        Vault::open_through(&path, Source::holding(b"another")),
        Err(Error::WrongPasswordOrNotAVault)
    ));
}

#[test]
fn a_source_with_nothing_opens_nothing_and_says_so() {
    let directory = tempfile::tempdir().unwrap();
    let path = vault_in(directory.path());
    let source = Source::holding(PASSWORD);
    source.set(None);

    assert!(matches!(
        Vault::open_through(&path, source),
        Err(Error::KeysGone)
    ));
}

#[test]
fn a_source_that_changed_its_password_does_not_get_to_seal_the_vault() {
    // The dangerous one. The vault was opened under one password, the agent
    // locked, another agent came up with another vault's password while a
    // command sat waiting, and the command saves. Sealed under what that one
    // gives, the vault would quietly open under a password its owner is not
    // thinking of, and no longer under its own.
    let directory = tempfile::tempdir().unwrap();
    let path = vault_in(directory.path());
    let source = Source::holding(PASSWORD);
    let mut vault = Vault::open_through(&path, source.clone()).unwrap();
    vault.add(note("second")).unwrap();
    let before = std::fs::read(&path).unwrap();

    source.set(Some(b"another"));

    assert!(matches!(vault.save(), Err(Error::KeysChanged)));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "the file was touched"
    );
    assert_eq!(titles(&Vault::open(&path, PASSWORD).unwrap()), ["first"]);
}

#[test]
fn a_source_that_went_away_leaves_the_file_as_it_was() {
    let directory = tempfile::tempdir().unwrap();
    let path = vault_in(directory.path());
    let source = Source::holding(PASSWORD);
    let mut vault = Vault::open_through(&path, source.clone()).unwrap();
    vault.add(note("second")).unwrap();
    let before = std::fs::read(&path).unwrap();

    source.set(None);

    assert!(matches!(vault.save(), Err(Error::KeysGone)));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(vault.writes(), 0);
}

#[test]
fn changing_the_password_takes_having_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = vault_in(directory.path());
    let mut vault = Vault::open_through(&path, Source::holding(PASSWORD)).unwrap();

    assert!(matches!(
        vault.change_password(b"new"),
        Err(Error::PasswordNeeded)
    ));
    // Refused before anything moved.
    assert_eq!(titles(&Vault::open(&path, PASSWORD).unwrap()), ["first"]);
}

#[test]
fn an_opener_says_what_it_opens() {
    let directory = tempfile::tempdir().unwrap();
    let path = vault_in(directory.path());
    let right = Source::holding(PASSWORD);
    let wrong = Source::holding(b"another");

    assert!(Opener::Password(PASSWORD).opens(&path));
    assert!(!Opener::Password(b"another").opens(&path));
    assert!(Opener::Source(right.as_ref()).opens(&path));
    assert!(!Opener::Source(wrong.as_ref()).opens(&path));
    assert!(!Opener::Source(right.as_ref()).opens(&directory.path().join("gone")));
}

#[test]
fn a_vault_held_through_a_source_catches_up_with_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = vault_in(directory.path());
    let source = Source::holding(PASSWORD);

    let mut mine = Vault::open_through(&path, source.clone()).unwrap();
    assert!(!mine.reload().unwrap());

    let mut theirs = Vault::open(&path, PASSWORD).unwrap();
    theirs.add(note("theirs")).unwrap();
    theirs.save().unwrap();
    assert!(matches!(mine.save(), Err(Error::ChangedOnDisk(_))));

    assert!(mine.reload().unwrap());
    mine.add(note("mine")).unwrap();
    // Sealing proves the source still holds the password that opened this, which
    // is now the version just read rather than the first one.
    mine.save().unwrap();

    let reopened = Vault::open(&path, PASSWORD).unwrap();
    let titles: Vec<String> = reopened
        .list()
        .unwrap()
        .into_iter()
        .map(|item| item.title)
        .collect();
    assert_eq!(titles.len(), 3, "{titles:?}");
}
