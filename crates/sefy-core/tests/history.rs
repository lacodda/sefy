//! The history of an item's contents: what an edit keeps, what a restore
//! brings back, and what neither of them touches.

use sefy_core::{Error, Field, ItemKind, NewItem, Payload, Query, Vault};
use std::path::PathBuf;

const PASSWORD: &[u8] = b"correct horse battery staple";

struct Fixture {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");
    Fixture {
        _directory: directory,
        path,
    }
}

fn login(password: &str, url: &str) -> Payload {
    Payload::fields(
        ItemKind::Login,
        [
            Field::public("login", "ada"),
            Field::secret("password", password),
            Field::public("url", url),
        ],
    )
}

fn note(text: &str) -> Payload {
    Payload::Note {
        text: text.to_owned(),
    }
}

/// A vault with one login, its password changed twice.
fn changed_twice(fixture: &Fixture) -> (Vault, i64) {
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault.set_device(Some("desk".to_owned()));
    let id = vault
        .add(NewItem::new(
            "mail",
            login("first", "https://mail.example.com"),
        ))
        .unwrap();
    vault
        .update(
            id,
            None,
            Some(login("second", "https://mail.example.com")),
            None,
        )
        .unwrap();
    vault
        .update(
            id,
            None,
            Some(login("third", "https://new.example.com")),
            None,
        )
        .unwrap();
    vault.save().unwrap();
    (vault, id)
}

fn password(payload: &Payload) -> &str {
    &payload.field("password").unwrap().value
}

#[test]
fn an_edit_keeps_what_it_replaced() {
    let fixture = fixture();
    let (vault, id) = changed_twice(&fixture);

    let history = vault.history(id).unwrap();
    let passwords: Vec<&str> = history
        .iter()
        .map(|version| password(&version.payload))
        .collect();
    assert_eq!(passwords, ["first", "second", "third"], "oldest first");

    let seqs: Vec<i64> = history.iter().map(|version| version.seq).collect();
    assert_eq!(seqs, [1, 2, 3]);
    assert!(history[2].current);
    assert!(!history[0].current && !history[1].current);
    assert!(history.iter().all(|version| !version.conflict));
    assert!(
        history
            .iter()
            .all(|version| version.device.as_deref() == Some("desk")),
        "each version says where it was written"
    );
    assert_eq!(vault.earlier_versions(id).unwrap(), 2);
}

#[test]
fn history_survives_a_save_and_reopen() {
    let fixture = fixture();
    let (vault, id) = changed_twice(&fixture);
    let before = vault.history(id).unwrap();
    drop(vault);

    let reopened = Vault::open(&fixture.path, PASSWORD).unwrap();
    assert_eq!(reopened.history(id).unwrap(), before);
}

#[test]
fn every_version_has_an_identity_of_its_own() {
    let fixture = fixture();
    let (vault, id) = changed_twice(&fixture);
    let history = vault.history(id).unwrap();

    let mut identities: Vec<&str> = history
        .iter()
        .map(|version| version.uuid.as_str())
        .collect();
    identities.sort_unstable();
    identities.dedup();
    assert_eq!(identities.len(), 3);
    assert!(
        identities
            .iter()
            .all(|uuid| *uuid != vault.summary(id).unwrap().uuid),
        "a version is not the item"
    );
}

#[test]
fn a_new_title_or_new_tags_make_no_version() {
    // Labels, not values: renaming an item is not something to restore from.
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let id = vault.add(NewItem::new("shed", note("4815"))).unwrap();

    vault
        .update(
            id,
            Some("garden shed".to_owned()),
            None,
            Some(vec!["home".to_owned()]),
        )
        .unwrap();

    assert_eq!(vault.history(id).unwrap().len(), 1);
    assert_eq!(vault.earlier_versions(id).unwrap(), 0);
}

#[test]
fn saving_the_same_contents_again_makes_no_version() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let id = vault.add(NewItem::new("shed", note("4815"))).unwrap();

    vault.update(id, None, Some(note("4815")), None).unwrap();

    assert_eq!(vault.history(id).unwrap().len(), 1);
}

#[test]
fn a_restore_brings_back_the_whole_contents_and_can_itself_be_undone() {
    let fixture = fixture();
    let (mut vault, id) = changed_twice(&fixture);
    let first = vault.history(id).unwrap()[0].uuid.clone();

    assert!(vault.restore(id, &first, None).unwrap());

    let current = vault.get(id).unwrap().payload;
    assert_eq!(current, login("first", "https://mail.example.com"));

    let history = vault.history(id).unwrap();
    assert_eq!(history.len(), 4, "the restore is a version of its own");
    assert_eq!(
        password(&history[2].payload),
        "third",
        "what it replaced is kept"
    );
    assert_ne!(
        history[3].uuid, first,
        "a restore writes a new version rather than reviving the old one"
    );
    assert_eq!(history[3].seq, 4);
}

#[test]
fn a_restored_field_comes_back_alone() {
    let fixture = fixture();
    let (mut vault, id) = changed_twice(&fixture);
    let first = vault.history(id).unwrap()[0].uuid.clone();

    assert!(vault.restore(id, &first, Some("password")).unwrap());

    let current = vault.get(id).unwrap().payload;
    assert_eq!(password(&current), "first");
    assert_eq!(
        current.field("url").unwrap().value,
        "https://new.example.com",
        "the other fields keep their current values"
    );
    assert!(
        current.field("password").unwrap().secret,
        "and secrecy comes back with the value"
    );
}

#[test]
fn a_field_the_record_no_longer_has_comes_back_at_the_end() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let id = vault
        .add(NewItem::new("mail", login("s3cret", "https://example.com")))
        .unwrap();
    vault
        .update(
            id,
            None,
            Some(Payload::fields(
                ItemKind::Login,
                [
                    Field::public("login", "ada"),
                    Field::secret("password", "s3cret"),
                ],
            )),
            None,
        )
        .unwrap();
    let first = vault.history(id).unwrap()[0].uuid.clone();

    vault.restore(id, &first, Some("url")).unwrap();

    let Payload::Fields { fields, .. } = vault.get(id).unwrap().payload else {
        panic!("still a record");
    };
    let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
    assert_eq!(names, ["login", "password", "url"]);
}

#[test]
fn a_field_the_version_did_not_have_is_refused_with_what_it_did_have() {
    let fixture = fixture();
    let (mut vault, id) = changed_twice(&fixture);
    let first = vault.history(id).unwrap()[0].uuid.clone();

    let error = vault.restore(id, &first, Some("pin")).unwrap_err();

    match error {
        Error::FieldNotInVersion { name, available } => {
            assert_eq!(name, "pin");
            assert_eq!(available, ["login", "password", "url"]);
        }
        other => panic!("expected FieldNotInVersion, got {other:?}"),
    }
    assert_eq!(
        vault.history(id).unwrap().len(),
        3,
        "a refused restore writes nothing"
    );
}

#[test]
fn restoring_what_is_already_current_changes_nothing() {
    let fixture = fixture();
    let (mut vault, id) = changed_twice(&fixture);
    let second = vault.history(id).unwrap()[1].uuid.clone();
    vault.restore(id, &second, None).unwrap();
    let before = vault.history(id).unwrap();

    assert!(!vault.restore(id, &second, None).unwrap());
    assert_eq!(vault.history(id).unwrap(), before);
}

#[test]
fn a_version_that_is_not_there_is_named_as_missing() {
    let fixture = fixture();
    let (mut vault, id) = changed_twice(&fixture);

    let error = vault
        .restore(id, "00000000-0000-4000-8000-000000000000", None)
        .unwrap_err();
    assert!(matches!(error, Error::VersionNotFound { .. }), "{error:?}");
}

#[test]
fn a_version_of_another_item_cannot_be_restored_into_this_one() {
    let fixture = fixture();
    let (mut vault, id) = changed_twice(&fixture);
    let other = vault.add(NewItem::new("shed", note("one"))).unwrap();
    vault.update(other, None, Some(note("two")), None).unwrap();
    let foreign = vault.history(other).unwrap()[0].uuid.clone();

    let error = vault.restore(id, &foreign, None).unwrap_err();
    assert!(matches!(error, Error::VersionNotFound { .. }), "{error:?}");
}

#[test]
fn a_note_is_restored_whole_and_never_by_field() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let id = vault.add(NewItem::new("shed", note("one"))).unwrap();
    vault.update(id, None, Some(note("two")), None).unwrap();
    let first = vault.history(id).unwrap()[0].uuid.clone();

    assert!(matches!(
        vault.restore(id, &first, Some("text")),
        Err(Error::FieldNotInVersion { .. })
    ));
    vault.restore(id, &first, None).unwrap();
    assert_eq!(vault.get(id).unwrap().payload, note("one"));
}

#[test]
fn removing_an_item_removes_its_history() {
    // A removed secret is removed, not moved somewhere it can no longer be
    // seen from. No row of its history may outlive it.
    let fixture = fixture();
    let (mut vault, id) = changed_twice(&fixture);
    vault.remove(id).unwrap();
    vault.save().unwrap();

    let sealed = std::fs::read(&fixture.path).unwrap();
    let database = sefy_core::format::decode(PASSWORD, &sealed).unwrap();
    let connection = sefy_core::db::load(&database).unwrap();
    let left: i64 = connection
        .query_row("SELECT COUNT(*) FROM versions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn an_old_value_is_not_something_to_find_an_item_by() {
    // Search reads the current contents only. An old URL turning up an item
    // would say the old value is in there, and the old value of a public field
    // can be the one someone meant to be rid of.
    let fixture = fixture();
    let (vault, _) = changed_twice(&fixture);

    assert!(
        vault
            .search(&Query::all().text("mail.example.com"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        vault
            .search(&Query::all().text("new.example.com"))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn history_never_reaches_the_disk_in_the_clear() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let id = vault
        .add(NewItem::new("shed", note("OLDSECRETVALUE")))
        .unwrap();
    vault.update(id, None, Some(note("new")), None).unwrap();
    vault.save().unwrap();

    let file = std::fs::read(&fixture.path).unwrap();
    assert!(
        !file
            .windows(b"OLDSECRETVALUE".len())
            .any(|window| window == b"OLDSECRETVALUE")
    );
}
