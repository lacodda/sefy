//! Records written before 0.7.0, checked against a real file rather than the
//! code.
//!
//! `vault-v0.6.0.blob` was written by the published 0.6.0 binary and is kept
//! byte for byte. Up to that version a login was a `credential` row with five
//! fixed columns; from 0.7.0 it is a set of named fields. Reasoning about the
//! migration cannot show that a user's accounts survive it — opening the file
//! they actually have can.
//!
//! The second half of this file is the harder promise. A vault travels, and the
//! machine at the other end may still run 0.6.0: it writes into the
//! `credentials` table long after this build has migrated the database. An
//! `user_version` of 3 therefore says "this database passed through the
//! migration once", never "every row has been brought along" — the same lesson
//! the uuid migration taught in 0.2.0, and the reason the pass that moves rows
//! across runs unconditionally on every open.

use sefy_core::{Field, ItemKind, NewItem, Payload, Vault};
use std::fs;
use std::path::{Path, PathBuf};

/// The password the fixture was created with. Fictional, like every secret in
/// these tests.
const PASSWORD: &[u8] = b"correct horse battery staple";

/// Copies the fixture somewhere writable, leaving the checked-in file alone.
fn fixture_copy() -> (tempfile::TempDir, PathBuf) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("vault-v0.6.0.blob");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");
    fs::copy(&source, &path).unwrap();
    (directory, path)
}

/// Reads one item's fields by title, failing the test if it is not a record.
fn fields_of(vault: &Vault, title: &str) -> Vec<Field> {
    let summary = vault.resolve(title).unwrap();
    match vault.get(summary.id).unwrap().payload {
        Payload::Fields { fields, .. } => fields,
        other => panic!("{title:?} should be a record, got {other:?}"),
    }
}

/// Looks up one field's value.
fn value_of(fields: &[Field], name: &str) -> String {
    fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("no field named {name:?}"))
        .value
        .clone()
}

#[test]
fn a_credential_written_by_0_6_0_becomes_a_login_with_fields() {
    let (_directory, path) = fixture_copy();

    let vault = Vault::open(&path, PASSWORD).unwrap();
    let summary = vault.resolve("mail").unwrap();
    assert_eq!(summary.kind, ItemKind::Login, "the kind is renamed too");

    let fields = fields_of(&vault, "mail");
    assert_eq!(value_of(&fields, "login"), "someone@example.com");
    assert_eq!(value_of(&fields, "password"), "hunter2");
    assert_eq!(value_of(&fields, "url"), "https://mail.example.com");
    assert_eq!(value_of(&fields, "totp"), "JBSWY3DPEHPK3PXP");
    assert_eq!(value_of(&fields, "notes"), "recovery in the drawer");
}

#[test]
fn the_migration_keeps_secrecy_where_it_belongs() {
    let (_directory, path) = fixture_copy();
    let vault = Vault::open(&path, PASSWORD).unwrap();
    let fields = fields_of(&vault, "mail");

    for name in ["password", "totp"] {
        let field = fields.iter().find(|field| field.name == name).unwrap();
        assert!(field.secret, "{name} must not be printable by `show`");
    }
    for name in ["login", "url", "notes"] {
        let field = fields.iter().find(|field| field.name == name).unwrap();
        assert!(!field.secret, "{name} was never a secret");
    }
}

#[test]
fn an_absent_optional_stays_absent_rather_than_becoming_empty() {
    // The fixture's second credential has no url, totp or notes. Storing them
    // as empty fields would turn "this account has no URL" into "its URL is the
    // empty string", and `show` would print three blank lines that mean
    // nothing.
    let (_directory, path) = fixture_copy();
    let vault = Vault::open(&path, PASSWORD).unwrap();
    let fields = fields_of(&vault, "minimal");

    let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
    assert_eq!(names, ["login", "password"]);
}

#[test]
fn a_note_written_by_0_6_0_is_untouched_by_the_migration() {
    let (_directory, path) = fixture_copy();
    let vault = Vault::open(&path, PASSWORD).unwrap();

    let summary = vault.resolve("shed").unwrap();
    assert_eq!(summary.kind, ItemKind::Note);
    assert_eq!(summary.tags, vec!["codes", "home"]);
    let Payload::Note { text } = vault.get(summary.id).unwrap().payload else {
        panic!("the fixture's note is a note");
    };
    assert_eq!(text.trim(), "combination 4815");
}

#[test]
fn a_migrated_vault_still_takes_new_items() {
    let (_directory, path) = fixture_copy();

    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    vault
        .add(NewItem::new(
            "visa",
            Payload::fields(
                ItemKind::Card,
                [
                    Field::secret("number", "4111111111111111"),
                    Field::public("expiry", "01/29"),
                ],
            ),
        ))
        .unwrap();
    vault.save().unwrap();

    let reopened = Vault::open(&path, PASSWORD).unwrap();
    assert_eq!(reopened.list().unwrap().len(), 4);
    assert_eq!(value_of(&fields_of(&reopened, "visa"), "expiry"), "01/29");
}

#[test]
fn migrating_twice_does_not_change_what_was_migrated_once() {
    let (_directory, path) = fixture_copy();

    let vault = Vault::open(&path, PASSWORD).unwrap();
    let first = fields_of(&vault, "mail");
    let uuid = vault.resolve("mail").unwrap().uuid;
    vault.save().unwrap();

    let reopened = Vault::open(&path, PASSWORD).unwrap();
    assert_eq!(fields_of(&reopened, "mail"), first);
    assert_eq!(
        reopened.resolve("mail").unwrap().uuid,
        uuid,
        "an item must stay the same item to a merge on the other machine"
    );
}

/// Writes a `credentials` row straight into a vault file, the way 0.6.0 does.
///
/// The point is a database this build has already migrated: 0.6.0 knows nothing
/// of the `fields` table, so the account it adds lands where it always did, and
/// nothing in the file says it is there.
fn insert_credential_the_old_way(path: &Path, title: &str, login: &str, password: &str) {
    let sealed = fs::read(path).unwrap();
    let database = sefy_core::format::decode(PASSWORD, &sealed).unwrap();

    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .deserialize_read_exact(
            rusqlite::MAIN_DB,
            std::io::Cursor::new(&database[..]),
            database.len(),
            false,
        )
        .unwrap();

    connection
        .execute(
            "INSERT INTO items (uuid, title, kind, created_at, updated_at)
             VALUES (?1, ?2, 'credential', 900, 900)",
            rusqlite::params![format!("00000000-0000-4000-8000-{:012}", 1), title],
        )
        .unwrap();
    let id = connection.last_insert_rowid();
    connection
        .execute(
            "INSERT INTO credentials (item_id, login, password, url, totp, notes)
             VALUES (?1, ?2, ?3, NULL, NULL, NULL)",
            rusqlite::params![id, login, password],
        )
        .unwrap();

    let updated = connection.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
    fs::write(path, sefy_core::format::encode(PASSWORD, &updated).unwrap()).unwrap();
}

#[test]
fn an_account_added_by_0_6_0_after_the_migration_is_still_picked_up() {
    // The defect this guards against: a migration that runs only when
    // `user_version` is behind would skip this row forever, and the account
    // would read as a login with no fields at all — present in every listing,
    // empty in every `show`. The same shape as the 0.2.0 uuid defect.
    let (_directory, path) = fixture_copy();

    // Migrate once, so `user_version` is already current.
    Vault::open(&path, PASSWORD).unwrap().save().unwrap();
    insert_credential_the_old_way(&path, "added by an old build", "late@example.com", "s3cret");

    let vault = Vault::open(&path, PASSWORD).unwrap();
    let summary = vault.resolve("added by an old build").unwrap();
    assert_eq!(summary.kind, ItemKind::Login);

    let fields = fields_of(&vault, "added by an old build");
    assert_eq!(value_of(&fields, "login"), "late@example.com");
    assert_eq!(value_of(&fields, "password"), "s3cret");
}

#[test]
fn an_edit_is_not_undone_by_the_row_the_old_build_left_behind() {
    // The pre-0.7.0 table is deliberately left in place, which raises the
    // question this answers: a credential that was migrated and then edited
    // must not be overwritten by its own stale `credentials` row the next time
    // the vault is opened.
    let (_directory, path) = fixture_copy();

    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let id = vault.resolve("mail").unwrap().id;
    let mut fields = fields_of(&vault, "mail");
    fields
        .iter_mut()
        .find(|field| field.name == "password")
        .unwrap()
        .value = "changed".to_owned();
    vault
        .update(
            id,
            None,
            Some(Payload::fields(ItemKind::Login, fields)),
            None,
        )
        .unwrap();
    vault.save().unwrap();

    let reopened = Vault::open(&path, PASSWORD).unwrap();
    assert_eq!(
        value_of(&fields_of(&reopened, "mail"), "password"),
        "changed"
    );
}

#[test]
fn the_migration_leaves_no_readable_copy_behind_for_an_older_build() {
    // 0.6.0 reads a password straight out of the `credentials` table, without
    // consulting the item's kind. So a row left there after migration would let
    // that build hand out a secret for an item its own `show` and `ls` call a
    // kind they do not understand — one binary answering "do you know this
    // item" two different ways. Emptying the table makes it wrong consistently.
    let (_directory, path) = fixture_copy();
    Vault::open(&path, PASSWORD).unwrap().save().unwrap();

    let sealed = fs::read(&path).unwrap();
    let database = sefy_core::format::decode(PASSWORD, &sealed).unwrap();
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .deserialize_read_exact(
            rusqlite::MAIN_DB,
            std::io::Cursor::new(&database[..]),
            database.len(),
            false,
        )
        .unwrap();

    let left_behind: i64 = connection
        .query_row("SELECT COUNT(*) FROM credentials", [], |row| row.get(0))
        .unwrap();
    assert_eq!(left_behind, 0, "no secret may stay in the pre-0.7.0 table");

    // The table itself stays, so an older build's own writes still land
    // somewhere this build will pick them up rather than failing outright.
    let table_survives: bool = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'credentials'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
        == 1;
    assert!(table_survives);
}
