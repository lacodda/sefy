//! Wiping a real vault: every file of it goes, and what a second name for it
//! still sees opens under no password.

use sefy_core::wipe::{self, Role};
use sefy_core::{Error, NewItem, Payload, Vault, copies};
use std::fs;
use std::path::{Path, PathBuf};

const PASSWORD: &[u8] = b"correct horse battery staple";

fn vault_with_a_secret(path: &Path) -> Vault {
    let mut vault = Vault::create(path, PASSWORD).unwrap();
    vault
        .add(NewItem::new(
            "shed",
            Payload::Note {
                text: "combination 4815".to_owned(),
            },
        ))
        .unwrap();
    vault.save().unwrap();
    vault
}

/// A second name for the same file, which outlives the first.
fn witness(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap().to_os_string();
    name.push(".witness");
    let witness = path.with_file_name(name);
    fs::hard_link(path, &witness).unwrap();
    witness
}

#[test]
fn a_wipe_leaves_nothing_of_the_vault_that_opens() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");
    let vault = vault_with_a_secret(&path);
    // Two copies, taken the way a merge takes them, and the file a crashed
    // save would have left.
    copies::keep(&path).unwrap();
    copies::keep(&path).unwrap();
    let unfinished = directory.path().join("notes.bak.sefy-tmp");
    fs::copy(&path, &unfinished).unwrap();
    let witnesses: Vec<PathBuf> = [
        path.clone(),
        copies::path(&path, 1),
        copies::path(&path, 2),
        unfinished.clone(),
    ]
    .iter()
    .map(|part| witness(part))
    .collect();
    for seen in &witnesses {
        assert!(Vault::open(seen, PASSWORD).is_ok(), "{}", seen.display());
    }

    let wiped = vault.wipe();

    assert!(wiped.complete(), "{:?}", wiped.failed);
    let roles: Vec<Role> = wiped.destroyed.iter().map(|part| part.role).collect();
    assert_eq!(
        roles,
        vec![Role::Vault, Role::Copy(1), Role::Copy(2), Role::Unfinished]
    );
    for part in [
        &path,
        &copies::path(&path, 1),
        &copies::path(&path, 2),
        &unfinished,
    ] {
        assert!(!part.exists(), "{} is still there", part.display());
    }
    for seen in &witnesses {
        assert!(
            matches!(
                Vault::open(seen, PASSWORD),
                Err(Error::WrongPasswordOrNotAVault)
            ),
            "{} still opens",
            seen.display()
        );
    }
}

#[test]
fn a_copy_under_an_earlier_password_goes_too() {
    // A copy the current password does not open is still the vault's: it may
    // be the one a retired password still opens, which is exactly what an
    // emergency must not leave behind.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");
    let vault = vault_with_a_secret(&path);
    let stranger = copies::path(&path, 3);
    Vault::create(&stranger, b"an older password").unwrap();

    let wiped = vault.wipe();

    assert!(wiped.complete());
    assert!(!stranger.exists());
}

#[test]
fn a_wipe_names_the_remote_it_cannot_reach() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");
    let mut vault = vault_with_a_secret(&path);
    vault.record_sync("github", "push").unwrap();
    vault.save().unwrap();

    let wiped = vault.wipe();

    let stamp = wiped.last_sync.expect("the vault had reached a remote");
    assert_eq!(stamp.transport, "github");
}

#[test]
fn a_vault_that_never_left_this_machine_says_so() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");

    let wiped = vault_with_a_secret(&path).wipe();

    assert!(wiped.last_sync.is_none());
}

#[test]
fn files_beside_the_vault_that_are_not_sefys_are_left_alone() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");
    let vault = vault_with_a_secret(&path);
    let neighbours: Vec<PathBuf> = ["notes.bak.4", "notes.bak.old", "notes.txt"]
        .iter()
        .map(|name| directory.path().join(name))
        .collect();
    for neighbour in &neighbours {
        fs::write(neighbour, "mine").unwrap();
    }

    vault.wipe();

    for neighbour in &neighbours {
        assert_eq!(fs::read_to_string(neighbour).unwrap(), "mine");
    }
}

#[test]
fn the_listing_is_what_the_wipe_destroys() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");
    let vault = vault_with_a_secret(&path);
    copies::keep(&path).unwrap();
    let listed = wipe::parts(&path);

    let wiped = vault.wipe();

    assert_eq!(wiped.destroyed, listed);
}
