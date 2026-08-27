//! The format-v1 promise, checked against a real file rather than the code.
//!
//! `vault-v0.5.0.blob` was written by the published 0.5.0 binary and is kept
//! byte for byte. Every dependency bump that touches the key derivation or the
//! cipher — argon2, chacha20poly1305, getrandom — silently changes what the
//! password unlocks; reasoning about the crates cannot catch that, but failing
//! to open this file does. The vault a user created on 0.1.x must keep opening,
//! and that is a release criterion for 1.0, not a nicety.

use sefy_core::{ItemKind, NewItem, Payload, Vault};
use std::fs;
use std::path::PathBuf;

/// The password the fixture was created with. Fictional, like every secret in
/// these tests.
const PASSWORD: &[u8] = b"correct horse battery staple";

/// Copies the fixture somewhere writable: opening a vault is read-only, but the
/// test also writes to it, and the checked-in file must stay untouched.
fn fixture_copy() -> (tempfile::TempDir, PathBuf) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("vault-v0.5.0.blob");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");
    fs::copy(&source, &path).unwrap();
    (directory, path)
}

#[test]
fn a_vault_written_by_0_5_0_still_opens_and_reads() {
    let (_directory, path) = fixture_copy();

    let vault = Vault::open(&path, PASSWORD).unwrap();
    let items = vault.list().unwrap();
    assert_eq!(items.len(), 2, "the fixture holds two notes");

    let shed = items
        .iter()
        .find(|item| item.title == "shed")
        .expect("the fixture's first note");
    assert_eq!(shed.kind, ItemKind::Note);
    assert_eq!(shed.tags, vec!["codes", "home"]);

    let Payload::Note { text } = vault.get(shed.id).unwrap().payload else {
        panic!("the fixture's first note is a note");
    };
    assert_eq!(text.trim(), "combination 4815");
}

#[test]
fn a_vault_written_by_0_5_0_still_takes_new_items() {
    let (_directory, path) = fixture_copy();

    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let added = vault
        .add(NewItem::new(
            "added today",
            Payload::Note {
                text: "written by the current build".to_owned(),
            },
        ))
        .unwrap();
    vault.save().unwrap();

    let reopened = Vault::open(&path, PASSWORD).unwrap();
    assert_eq!(reopened.list().unwrap().len(), 3);
    assert_eq!(reopened.get(added).unwrap().summary.title, "added today");
}
