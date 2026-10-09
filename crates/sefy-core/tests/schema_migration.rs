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

/// The same treatment for the 3 → 4 move, against a file from the published
/// 0.7.1 binary.
///
/// 0.8.0 added `meta`, where a fact about the vault itself lives: the first of
/// them is when it last reached a remote, which `sefy status` reports. A vault
/// written before that table existed has to open, keep everything in it, and
/// answer "never" rather than failing on a table it has never seen.
mod version_four {
    use super::PASSWORD;
    use sefy_core::Vault;
    use std::fs;
    use std::path::PathBuf;

    /// Copies the 0.7.1 fixture somewhere writable.
    fn fixture_copy() -> (tempfile::TempDir, PathBuf) {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("vault-v0.7.1.blob");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.bak");
        fs::copy(&source, &path).unwrap();
        (directory, path)
    }

    #[test]
    fn a_vault_from_0_7_1_opens_and_keeps_everything_in_it() {
        let (_directory, path) = fixture_copy();
        let vault = Vault::open(&path, PASSWORD).unwrap();

        let titles: Vec<String> = vault
            .list()
            .unwrap()
            .into_iter()
            .map(|item| item.title)
            .collect();
        assert!(
            titles.contains(&"a note from 0.7.1".to_owned())
                && titles.contains(&"a login from 0.7.1".to_owned()),
            "both items survived the migration: {titles:?}"
        );
    }

    #[test]
    fn a_vault_written_before_the_table_existed_says_it_never_synced() {
        // The answer that matters: "never", rather than an error about a
        // missing table. A vault older than the feature is the ordinary case
        // for a long time after it ships.
        let (_directory, path) = fixture_copy();
        let vault = Vault::open(&path, PASSWORD).unwrap();

        assert!(vault.last_sync().unwrap().is_none());
        assert_eq!(vault.stats().unwrap().last_sync, None);
    }

    #[test]
    fn the_migration_moves_the_schema_forward_and_the_stamp_survives_a_reopen() {
        let (_directory, path) = fixture_copy();

        let before = Vault::open(&path, PASSWORD).unwrap().stats().unwrap();
        assert_eq!(
            before.schema,
            sefy_core::db::SCHEMA_VERSION,
            "opening brings the schema up to date"
        );
        assert_eq!(before.items, 2);

        let mut vault = Vault::open(&path, PASSWORD).unwrap();
        vault.record_sync("file", "push").unwrap();
        vault.save().unwrap();
        drop(vault);

        let reopened = Vault::open(&path, PASSWORD).unwrap();
        let stamp = reopened.last_sync().unwrap().expect("recorded");
        assert_eq!(stamp.transport, "file");
        assert_eq!(stamp.operation, "push");
    }

    #[test]
    fn every_item_of_a_vault_from_before_history_starts_with_one_version() {
        // Nothing before 0.12.0 kept a history, so there is none to invent:
        // each item's contents become its first known version, dated by the
        // last change the row records, on a machine nobody wrote down.
        let (_directory, path) = fixture_copy();
        let vault = Vault::open(&path, PASSWORD).unwrap();

        for summary in vault.list().unwrap() {
            let history = vault.history(summary.id).unwrap();
            assert_eq!(history.len(), 1, "{}", summary.title);
            assert_eq!(history[0].seq, 1);
            assert_eq!(history[0].made_at, summary.updated_at);
            assert_eq!(history[0].device, None);
            assert!(history[0].current);
        }
    }

    #[test]
    fn the_first_edit_after_the_migration_keeps_what_0_7_1_wrote() {
        let (_directory, path) = fixture_copy();
        let mut vault = Vault::open(&path, PASSWORD).unwrap();
        let id = vault.resolve("a note from 0.7.1").unwrap().id;
        let written_then = vault.get(id).unwrap().payload;

        vault
            .update(
                id,
                None,
                Some(sefy_core::Payload::Note {
                    text: "rewritten".to_owned(),
                }),
                None,
            )
            .unwrap();
        vault.save().unwrap();

        let reopened = Vault::open(&path, PASSWORD).unwrap();
        let history = reopened.history(id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].payload, written_then);
    }

    #[test]
    fn the_counts_a_status_reports_come_off_a_real_file() {
        // Reasoning about the numbers cannot show they are right; a file
        // written by a binary that is published can.
        let (_directory, path) = fixture_copy();
        let stats = Vault::open(&path, PASSWORD).unwrap().stats().unwrap();

        assert_eq!(stats.items, 2);
        assert_eq!(stats.tags, 1, "both items carry the one tag");
        let kinds: Vec<&str> = stats
            .by_kind
            .iter()
            .map(|(kind, _)| kind.as_str())
            .collect();
        assert!(
            kinds.contains(&"note") && kinds.contains(&"login"),
            "both kinds are counted: {kinds:?}"
        );
    }
}

/// The 4 → 5 move seen from the other end: a vault this build has migrated,
/// written into by 0.11.x, which knows nothing of history.
///
/// The writes below are the SQL that build issues, run against the migrated
/// database, the same way the 0.6.0 case above does it. The live check with the
/// published binary is part of every release; these keep it from regressing in
/// between.
mod version_five {
    use super::PASSWORD;
    use sefy_core::{NewItem, Payload, Vault};
    use std::fs;
    use std::path::Path;

    fn note(text: &str) -> Payload {
        Payload::Note {
            text: text.to_owned(),
        }
    }

    /// Runs `sql` against the vault file as an older build would: the database
    /// decrypted into memory, foreign keys on as that build turns them on, and
    /// sealed back.
    fn as_an_older_build(path: &Path, sql: &str) {
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
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        connection.execute_batch(sql).unwrap();
        let bytes = connection.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
        fs::write(path, sefy_core::format::encode(PASSWORD, &bytes).unwrap()).unwrap();
    }

    /// A vault written by this build: one note, edited once.
    fn migrated() -> (tempfile::TempDir, std::path::PathBuf, i64) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.bak");
        let mut vault = Vault::create(&path, PASSWORD).unwrap();
        let id = vault.add(NewItem::new("shed", note("one"))).unwrap();
        vault.update(id, None, Some(note("two")), None).unwrap();
        vault.save().unwrap();
        (directory, path, id)
    }

    #[test]
    fn an_item_an_older_build_adds_is_given_a_version_on_the_next_open() {
        let (_directory, path, _) = migrated();
        as_an_older_build(
            &path,
            "INSERT INTO items (uuid, title, kind, created_at, updated_at)
                 VALUES ('00000000-0000-4000-8000-000000000011', 'added by 0.11', 'note', 900, 900);
             INSERT INTO notes (item_id, text) VALUES (last_insert_rowid(), 'plain');",
        );

        let vault = Vault::open(&path, PASSWORD).unwrap();
        let id = vault.resolve("added by 0.11").unwrap().id;
        let history = vault.history(id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].uuid.len(), 36, "a version needs an identity");
        assert_eq!(history[0].made_at, 900);
    }

    #[test]
    fn an_edit_by_an_older_build_becomes_current_and_leaves_the_history_alone() {
        // 0.11 rewrites the contents in place and keeps nothing: the value it
        // replaced is gone, which is what that build always did. What must not
        // happen is losing the history this build already kept, or failing to
        // open the vault over it.
        let (_directory, path, id) = migrated();
        as_an_older_build(
            &path,
            &format!("UPDATE notes SET text = 'three' WHERE item_id = {id};"),
        );

        let vault = Vault::open(&path, PASSWORD).unwrap();
        assert_eq!(vault.get(id).unwrap().payload, note("three"));
        let history = vault.history(id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].payload, note("one"));
    }

    #[test]
    fn a_delete_by_an_older_build_takes_the_history_with_it() {
        // The cascade lives in the table, not in this build's code, so an
        // older build removing an item cannot leave its old secrets behind in
        // a table it has never heard of.
        let (_directory, path, id) = migrated();
        as_an_older_build(&path, &format!("DELETE FROM items WHERE id = {id};"));

        let sealed = fs::read(&path).unwrap();
        let database = sefy_core::format::decode(PASSWORD, &sealed).unwrap();
        let connection = sefy_core::db::load(&database).unwrap();
        let left: i64 = connection
            .query_row("SELECT COUNT(*) FROM versions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }
}

/// Two copies of a vault from before history, each migrated on its own
/// machine, have to recognise each other's contents — or the first edit after
/// upgrading both machines reads as a conflict on the other one.
mod migrated_apart {
    use super::PASSWORD;
    use sefy_core::{Payload, Vault};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn copies_migrated_separately_merge_an_edit_without_a_conflict() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("vault-v0.7.1.blob");
        let directory = tempfile::tempdir().unwrap();
        let here = directory.path().join("here.bak");
        let there = directory.path().join("there.bak");
        fs::copy(&source, &here).unwrap();
        fs::copy(&source, &there).unwrap();

        // Each machine opens its own copy and writes it back: migrated apart.
        let mut theirs = Vault::open(&there, PASSWORD).unwrap();
        let their_id = theirs.resolve("a note from 0.7.1").unwrap().id;
        theirs
            .update(
                their_id,
                None,
                Some(Payload::Note {
                    text: "edited after the upgrade".to_owned(),
                }),
                None,
            )
            .unwrap();
        theirs.save().unwrap();

        let mut mine = Vault::open(&here, PASSWORD).unwrap();
        let report = sefy_core::merge(&mut mine, &theirs).unwrap();

        assert!(report.conflicts.is_empty(), "{report:?}");
        assert_eq!(report.updated.len(), 1);
        let id = mine.resolve("a note from 0.7.1").unwrap().id;
        assert_eq!(
            mine.history(id).unwrap().len(),
            2,
            "the contents from before are one version, not two"
        );
    }
}
