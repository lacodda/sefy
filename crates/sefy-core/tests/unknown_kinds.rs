//! What happens when a vault holds an item this build does not understand.
//!
//! Vaults travel between machines, and the two ends need not run the same
//! version of sefy. So a build must expect to meet an item written by a newer
//! one — and the only acceptable answer is to carry it, not to choke on it.
//!
//! Up to and including 0.5.0 the answer was to choke: a single item of an
//! unrecognized kind made `ls`, `find`, `export` and `merge` fail outright,
//! reporting "no item with id N" about an item that was plainly there. That
//! turned adding any new kind into a breaking change for everyone syncing
//! between versions, which is what these tests exist to prevent.

use sefy_core::{ItemKind, NewItem, Payload, Query, Vault, exchange, merge};
use std::path::{Path, PathBuf};

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

fn note(text: &str) -> Payload {
    Payload::Note {
        text: text.to_owned(),
    }
}

/// Writes an item of a kind this build has never heard of, the way a future
/// version of sefy would: a row in `items` plus its contents in a table this
/// build knows nothing about.
///
/// Going through the database directly is the point. There is no API for
/// creating an unknown kind — by definition — and a test that faked one at the
/// model level would prove nothing about what a real newer vault does.
fn add_item_of_a_future_kind(path: &Path, title: &str) {
    let file = std::fs::read(path).unwrap();
    let database = sefy_core::format::decode(PASSWORD, &file).unwrap();
    let connection = sefy_core::db::load(&database).unwrap();

    connection
        .execute(
            "INSERT INTO items (uuid, title, kind, created_at, updated_at)
             VALUES ('11111111-2222-4333-8444-555555555555', ?1, 'card', 1000, 1000)",
            [title],
        )
        .unwrap();
    let id = connection.last_insert_rowid();
    connection
        .execute_batch("CREATE TABLE cards (item_id INTEGER NOT NULL, number TEXT NOT NULL)")
        .unwrap();
    connection
        .execute(
            "INSERT INTO cards (item_id, number) VALUES (?1, '4111111111111111')",
            [id],
        )
        .unwrap();

    let dumped = sefy_core::db::dump(&connection).unwrap();
    std::fs::write(path, sefy_core::format::encode(PASSWORD, &dumped).unwrap()).unwrap();
}

fn vault_with_an_unknown_item(fixture: &Fixture) -> Vault {
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault
        .add(NewItem::new("shed", note("combination 4815")))
        .unwrap();
    vault.save().unwrap();

    add_item_of_a_future_kind(&fixture.path, "my card");
    Vault::open(&fixture.path, PASSWORD).unwrap()
}

#[test]
fn an_item_of_an_unknown_kind_is_listed_rather_than_breaking_the_listing() {
    let fixture = fixture();
    let vault = vault_with_an_unknown_item(&fixture);

    let items = vault.list().unwrap();
    assert_eq!(items.len(), 2, "both items are listed");

    let unknown = items
        .iter()
        .find(|item| item.title == "my card")
        .expect("the item from a newer sefy is in the listing");
    assert_eq!(unknown.kind, ItemKind::Unknown("card".to_owned()));
    assert!(!unknown.kind.is_known());
    assert_eq!(
        unknown.kind.as_str(),
        "card",
        "it keeps the name it was stored under"
    );
}

#[test]
fn searching_still_works_with_an_unknown_item_in_the_vault() {
    let fixture = fixture();
    let vault = vault_with_an_unknown_item(&fixture);

    // The known item is still findable: one unreadable neighbour must not take
    // the search down with it.
    let found = vault.search(&Query::all().text("shed")).unwrap();
    assert_eq!(found.len(), 1);

    // And the unknown one is findable by what this build can see of it.
    let found = vault.search(&Query::all().text("my card")).unwrap();
    assert_eq!(found.len(), 1);
}

#[test]
fn reading_an_unknown_item_reports_its_kind_instead_of_guessing() {
    let fixture = fixture();
    let vault = vault_with_an_unknown_item(&fixture);

    let summary = vault
        .list()
        .unwrap()
        .into_iter()
        .find(|i| i.title == "my card")
        .unwrap();
    let item = vault.get(summary.id).unwrap();

    // It reads back as itself, carrying the name and nothing invented.
    assert_eq!(
        item.payload,
        Payload::Unknown {
            kind: "card".to_owned()
        }
    );
}

#[test]
fn an_export_carries_an_unknown_item_and_says_its_contents_are_missing() {
    let fixture = fixture();
    let vault = vault_with_an_unknown_item(&fixture);

    let export = exchange::export(&vault).unwrap();
    assert_eq!(export.items.len(), 2, "the export holds both items");

    let unknown = export
        .items
        .iter()
        .find(|item| item.title == "my card")
        .expect("an export that dropped it would make the vault a trap");
    assert_eq!(unknown.kind, "card");
    assert!(
        unknown.contents_not_exported,
        "the entry must admit it is incomplete rather than look like an empty item"
    );
    assert!(unknown.uuid.is_some(), "its identity travels with it");
}

#[test]
fn importing_an_entry_this_build_cannot_store_is_counted_not_fatal() {
    let source = fixture();
    let vault = vault_with_an_unknown_item(&source);
    let export = exchange::export(&vault).unwrap();

    let destination = fixture();
    let mut fresh = Vault::create(&destination.path, PASSWORD).unwrap();
    let report = exchange::import(&mut fresh, &export).unwrap();

    // The one it cannot represent does not stop the other from arriving.
    assert_eq!(report.added, 1);
    assert_eq!(report.unsupported, 1);
    assert_eq!(report.total(), 2);
    assert_eq!(fresh.list().unwrap().len(), 1);
}

#[test]
fn a_merge_leaves_an_unknown_item_where_it_is_and_moves_the_rest() {
    let source = fixture();
    let with_unknown = vault_with_an_unknown_item(&source);

    let destination = fixture();
    let mut target = Vault::create(&destination.path, PASSWORD).unwrap();

    let report = merge(&mut target, &with_unknown).unwrap();

    assert_eq!(report.added, 1, "the readable item comes across");
    assert_eq!(
        report.unsupported, 1,
        "the unreadable one is reported, not copied as an empty item"
    );
    assert!(
        !report.is_empty(),
        "a merge that skipped something has not done nothing"
    );

    // Nothing was invented on this side.
    let titles: Vec<String> = target
        .list()
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(titles, vec!["shed".to_owned()]);

    // And nothing was taken from the other side either.
    assert_eq!(with_unknown.list().unwrap().len(), 2);
}

#[test]
fn an_unknown_item_can_be_retitled_and_retagged_but_not_rewritten() {
    let fixture = fixture();
    let mut vault = vault_with_an_unknown_item(&fixture);
    let id = vault
        .list()
        .unwrap()
        .into_iter()
        .find(|i| i.title == "my card")
        .unwrap()
        .id;

    // Title and tags live beside the contents, so they are still editable.
    vault
        .update(
            id,
            Some("my travel card".to_owned()),
            None,
            Some(vec!["wallet".to_owned()]),
        )
        .unwrap();
    vault.save().unwrap();

    let reopened = Vault::open(&fixture.path, PASSWORD).unwrap();
    let summary = reopened.summary(id).unwrap();
    assert_eq!(summary.title, "my travel card");
    assert_eq!(summary.tags, vec!["wallet"]);
    assert_eq!(summary.kind, ItemKind::Unknown("card".to_owned()));

    // Replacing the contents is refused: this build cannot read what is there
    // and must not write over it.
    let mut vault = Vault::open(&fixture.path, PASSWORD).unwrap();
    let refused = vault.update(
        id,
        None,
        Some(Payload::Unknown {
            kind: "card".to_owned(),
        }),
        None,
    );
    assert!(
        refused.is_err(),
        "writing an unknown payload must be refused"
    );
}

#[test]
fn the_contents_of_an_unknown_item_survive_being_carried_by_this_build() {
    let fixture = fixture();
    let mut vault = vault_with_an_unknown_item(&fixture);

    // Open, change something unrelated, and save: the round trip a sync does.
    vault.add(NewItem::new("spare", note("another"))).unwrap();
    vault.save().unwrap();

    // The future kind's own table is still there, with its row intact — this
    // build passed the item through without understanding a byte of it.
    let file = std::fs::read(&fixture.path).unwrap();
    let database = sefy_core::format::decode(PASSWORD, &file).unwrap();
    let connection = sefy_core::db::load(&database).unwrap();
    let number: String = connection
        .query_row("SELECT number FROM cards", [], |row| row.get(0))
        .expect("the newer sefy's table and row are untouched");
    assert_eq!(number, "4111111111111111");
}

#[test]
fn an_entry_of_an_unknown_kind_is_skipped_even_without_the_missing_contents_flag() {
    // An export need not come from sefy. Another tool, or a hand-written file,
    // can name a kind this build does not have without setting the flag that
    // sefy's own exporter would — and one such entry must not take the import
    // of everything else down with it.
    let json = r#"{
        "sefy_export": 1,
        "items": [
            { "title": "shed", "kind": "note", "text": "combination 4815" },
            { "title": "my card", "kind": "card", "number": "4111111111111111" }
        ]
    }"#;

    let export = exchange::from_json(json).unwrap();
    assert!(
        !export.items[1].contents_not_exported,
        "the point of this case is that the flag is absent"
    );

    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let report = exchange::import(&mut vault, &export).unwrap();

    assert_eq!(report.added, 1);
    assert_eq!(report.unsupported, 1);
    assert_eq!(vault.list().unwrap().len(), 1);
}
