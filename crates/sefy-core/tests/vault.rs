//! End-to-end behaviour of a vault file: what survives a save, what a wrong
//! password gets, and what never reaches the disk.

use sefy_core::{Credential, Error, ItemKind, NewItem, Payload, Query, Vault};
use std::fs;
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

fn note(text: &str) -> Payload {
    Payload::Note {
        text: text.to_owned(),
    }
}

#[test]
fn items_survive_a_save_and_reopen() {
    let fixture = fixture();

    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let note_id = vault
        .add(NewItem::new("shed", note("combination 4815")).with_tags(["home", "codes"]))
        .unwrap();
    let credential_id = vault
        .add(
            NewItem::new(
                "mail",
                Payload::Credential(Credential {
                    login: "someone".to_owned(),
                    password: "hunter2".to_owned(),
                    url: Some("https://example.invalid".to_owned()),
                    totp: Some("JBSWY3DPEHPK3PXP".to_owned()),
                    notes: None,
                }),
            )
            .with_tags(["mail"]),
        )
        .unwrap();
    vault.save().unwrap();

    let reopened = Vault::open(&fixture.path, PASSWORD).unwrap();

    let restored_note = reopened.get(note_id).unwrap();
    assert_eq!(restored_note.summary.title, "shed");
    assert_eq!(restored_note.summary.kind, ItemKind::Note);
    assert_eq!(restored_note.summary.tags, vec!["codes", "home"]);
    assert_eq!(restored_note.payload, note("combination 4815"));

    let restored_credential = reopened.get(credential_id).unwrap();
    match restored_credential.payload {
        Payload::Credential(credential) => {
            assert_eq!(credential.login, "someone");
            assert_eq!(credential.password, "hunter2");
            assert_eq!(credential.totp.as_deref(), Some("JBSWY3DPEHPK3PXP"));
            assert_eq!(credential.notes, None);
        }
        other => panic!("expected a credential, got {other:?}"),
    }
}

#[test]
fn attachments_come_back_byte_for_byte() {
    let fixture = fixture();
    // Bytes that would break anything treating the payload as text.
    let bytes: Vec<u8> = (0..=255u8).cycle().take(3 * 1024 * 1024 + 7).collect();

    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let id = vault
        .add(NewItem::new(
            "keyfile",
            Payload::File {
                filename: "id_ed25519".to_owned(),
                bytes: bytes.clone(),
            },
        ))
        .unwrap();
    vault.save().unwrap();

    let reopened = Vault::open(&fixture.path, PASSWORD).unwrap();
    match reopened.get(id).unwrap().payload {
        Payload::File {
            filename,
            bytes: restored,
        } => {
            assert_eq!(filename, "id_ed25519");
            assert_eq!(restored, bytes);
        }
        other => panic!("expected a file, got {other:?}"),
    }
}

#[test]
fn wrong_password_does_not_open_the_vault() {
    let fixture = fixture();
    Vault::create(&fixture.path, PASSWORD).unwrap();

    assert!(matches!(
        Vault::open(&fixture.path, b"not the password"),
        Err(Error::WrongPasswordOrNotAVault)
    ));
}

#[test]
fn a_corrupted_file_does_not_open_the_vault() {
    let fixture = fixture();
    Vault::create(&fixture.path, PASSWORD).unwrap();

    let mut bytes = fs::read(&fixture.path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&fixture.path, &bytes).unwrap();

    assert!(matches!(
        Vault::open(&fixture.path, PASSWORD),
        Err(Error::WrongPasswordOrNotAVault)
    ));
}

#[test]
fn an_unrelated_file_does_not_open_as_a_vault() {
    let fixture = fixture();
    // Long enough to clear the size check, so the tag verification is what
    // rejects it.
    fs::write(&fixture.path, vec![0x5au8; 4096]).unwrap();
    assert!(matches!(
        Vault::open(&fixture.path, PASSWORD),
        Err(Error::WrongPasswordOrNotAVault)
    ));

    fs::write(&fixture.path, b"tiny").unwrap();
    assert!(matches!(
        Vault::open(&fixture.path, PASSWORD),
        Err(Error::TooSmall)
    ));
}

#[test]
fn creating_over_an_existing_file_is_refused() {
    let fixture = fixture();
    Vault::create(&fixture.path, PASSWORD).unwrap();

    assert!(matches!(
        Vault::create(&fixture.path, PASSWORD),
        Err(Error::AlreadyExists(_))
    ));
}

#[test]
fn secrets_never_appear_in_the_file() {
    let fixture = fixture();
    let secret = "xyzzy-plugh-secret-string";

    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault
        .add(NewItem::new("thing", note(secret)).with_tags(["a-distinctive-tag"]))
        .unwrap();
    vault.save().unwrap();

    let bytes = fs::read(&fixture.path).unwrap();
    for needle in [secret, "a-distinctive-tag", "thing", "SQLite format 3"] {
        assert!(
            !bytes
                .windows(needle.len())
                .any(|window| window == needle.as_bytes()),
            "{needle:?} leaked into the vault file"
        );
    }
}

#[test]
fn saving_leaves_no_other_files_behind() {
    let fixture = fixture();
    let directory = fixture.path.parent().unwrap();

    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault.add(NewItem::new("one", note("first"))).unwrap();
    vault.save().unwrap();
    vault.add(NewItem::new("two", note("second"))).unwrap();
    vault.save().unwrap();

    let entries: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![fixture.path.file_name().unwrap().to_owned()]);
}

#[test]
fn a_failed_save_leaves_the_previous_vault_intact() {
    let fixture = fixture();

    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let id = vault.add(NewItem::new("kept", note("original"))).unwrap();
    vault.save().unwrap();
    let before = fs::read(&fixture.path).unwrap();

    // A directory in place of the temporary file makes the write fail after the
    // vault file already exists.
    let temporary = fixture.path.with_file_name("notes.bak.sefy-tmp");
    fs::create_dir(&temporary).unwrap();

    vault
        .add(NewItem::new("lost", note("never saved")))
        .unwrap();
    assert!(vault.save().is_err());

    assert_eq!(fs::read(&fixture.path).unwrap(), before);
    fs::remove_dir(&temporary).unwrap();

    let reopened = Vault::open(&fixture.path, PASSWORD).unwrap();
    assert_eq!(reopened.list().unwrap().len(), 1);
    assert_eq!(reopened.get(id).unwrap().payload, note("original"));
}

#[test]
fn updates_and_removals_persist() {
    let fixture = fixture();

    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let kept = vault
        .add(NewItem::new("old title", note("old text")).with_tags(["stale"]))
        .unwrap();
    let removed = vault.add(NewItem::new("gone", note("bye"))).unwrap();

    vault
        .update(
            kept,
            Some("new title".to_owned()),
            Some(note("new text")),
            Some(vec!["fresh".to_owned()]),
        )
        .unwrap();
    vault.remove(removed).unwrap();
    vault.save().unwrap();

    let reopened = Vault::open(&fixture.path, PASSWORD).unwrap();
    let item = reopened.get(kept).unwrap();
    assert_eq!(item.summary.title, "new title");
    assert_eq!(item.summary.tags, vec!["fresh"]);
    assert_eq!(item.payload, note("new text"));

    assert!(matches!(reopened.get(removed), Err(Error::ItemNotFound(_))));
    assert_eq!(reopened.tags().unwrap(), vec![("fresh".to_owned(), 1)]);
}

#[test]
fn an_items_kind_cannot_change() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let id = vault.add(NewItem::new("note", note("text"))).unwrap();

    let result = vault.update(
        id,
        None,
        Some(Payload::Credential(Credential::default())),
        None,
    );
    assert!(matches!(result, Err(Error::ItemKindMismatch { .. })));

    // The rejected edit changed nothing.
    assert_eq!(vault.get(id).unwrap().payload, note("text"));
}

#[test]
fn removing_a_missing_item_is_an_error() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    assert!(matches!(vault.remove(404), Err(Error::ItemNotFound(404))));
}

#[test]
fn search_filters_by_text_kind_and_tags() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();

    let bank_note = vault
        .add(NewItem::new("bank pin", note("1234")).with_tags(["money", "home"]))
        .unwrap();
    let bank_login = vault
        .add(
            NewItem::new(
                "bank login",
                Payload::Credential(Credential {
                    login: "customer".to_owned(),
                    password: "s3cret".to_owned(),
                    url: Some("https://bank.invalid".to_owned()),
                    ..Credential::default()
                }),
            )
            .with_tags(["money"]),
        )
        .unwrap();
    vault
        .add(NewItem::new("grocery list", note("milk")).with_tags(["home"]))
        .unwrap();

    let ids = |summaries: Vec<sefy_core::ItemSummary>| {
        let mut ids: Vec<i64> = summaries.into_iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids
    };

    assert_eq!(
        ids(vault.search(&Query::all().text("bank")).unwrap()),
        vec![bank_note, bank_login]
    );
    assert_eq!(
        ids(vault
            .search(&Query::all().text("bank").kind(ItemKind::Note))
            .unwrap()),
        vec![bank_note]
    );
    assert_eq!(
        ids(vault.search(&Query::all().tags(["money", "home"])).unwrap()),
        vec![bank_note]
    );
    // Matches inside a credential's fields, not just its title.
    assert_eq!(
        ids(vault.search(&Query::all().text("customer")).unwrap()),
        vec![bank_login]
    );
    assert_eq!(vault.list().unwrap().len(), 3);
}

#[test]
fn search_is_case_insensitive_and_takes_wildcards_literally() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();

    let literal = vault.add(NewItem::new("100% cotton", note("tag"))).unwrap();
    vault.add(NewItem::new("plain", note("nothing"))).unwrap();

    assert_eq!(
        vault.search(&Query::all().text("COTTON")).unwrap()[0].id,
        literal
    );

    let percent = vault.search(&Query::all().text("100%")).unwrap();
    assert_eq!(percent.len(), 1);
    assert_eq!(percent[0].id, literal);

    // A bare wildcard matches a literal one, not every item.
    let wildcard = vault.search(&Query::all().text("%")).unwrap();
    assert_eq!(wildcard.len(), 1);
    assert_eq!(wildcard[0].id, literal);
    assert!(vault.search(&Query::all().text("_")).unwrap().is_empty());
}

#[test]
fn attachment_bytes_are_not_searched() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault
        .add(NewItem::new(
            "blob",
            Payload::File {
                filename: "notes.txt".to_owned(),
                bytes: b"a needle hidden in the bytes".to_vec(),
            },
        ))
        .unwrap();

    assert!(
        vault
            .search(&Query::all().text("needle"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(vault.search(&Query::all().text("blob")).unwrap().len(), 1);
}

#[test]
fn changing_the_password_rewrites_the_file() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let id = vault.add(NewItem::new("thing", note("text"))).unwrap();
    vault.save().unwrap();

    vault.change_password(b"a different password").unwrap();

    assert!(matches!(
        Vault::open(&fixture.path, PASSWORD),
        Err(Error::WrongPasswordOrNotAVault)
    ));
    let reopened = Vault::open(&fixture.path, b"a different password").unwrap();
    assert_eq!(reopened.get(id).unwrap().payload, note("text"));
}

#[test]
fn a_vault_opens_on_a_machine_that_never_saw_it() {
    // Nothing in the file may depend on where it was written: a vault created
    // in one directory must open verbatim from another.
    let origin = fixture();
    let mut vault = Vault::create(&origin.path, PASSWORD).unwrap();
    let id = vault
        .add(NewItem::new("portable", note("travels")))
        .unwrap();
    vault.save().unwrap();
    let bytes = fs::read(&origin.path).unwrap();

    let elsewhere = tempfile::tempdir().unwrap();
    let copy = elsewhere.path().join("unrelated-name");
    fs::write(&copy, &bytes).unwrap();

    let opened = Vault::open(&copy, PASSWORD).unwrap();
    assert_eq!(opened.get(id).unwrap().payload, note("travels"));
}

#[test]
fn a_reference_resolves_by_id_and_by_title() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let bank = vault.add(NewItem::new("bank", note("4815"))).unwrap();
    vault
        .add(NewItem::new("grocery list", note("milk")))
        .unwrap();

    assert_eq!(vault.resolve(&bank.to_string()).unwrap().id, bank);
    assert_eq!(vault.resolve("bank").unwrap().id, bank);
    assert_eq!(vault.resolve("BANK").unwrap().id, bank);
    // Substring of a title, and a match on the note's text.
    assert_eq!(
        vault.resolve("groc").unwrap().id,
        vault.resolve("milk").unwrap().id
    );
}

#[test]
fn an_exact_title_wins_over_substring_matches() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let exact = vault.add(NewItem::new("mail", note("a"))).unwrap();
    vault.add(NewItem::new("mail — work", note("b"))).unwrap();
    vault.add(NewItem::new("mailing list", note("c"))).unwrap();

    assert_eq!(vault.resolve("mail").unwrap().id, exact);
}

#[test]
fn an_ambiguous_reference_reports_its_candidates() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault
        .add(NewItem::new("mail — personal", note("a")))
        .unwrap();
    vault.add(NewItem::new("mail — work", note("b"))).unwrap();

    match vault.resolve("mail") {
        Err(Error::Ambiguous {
            reference,
            candidates,
        }) => {
            assert_eq!(reference, "mail");
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("expected an ambiguity, got {other:?}"),
    }
}

#[test]
fn a_reference_matching_nothing_is_an_error() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault.add(NewItem::new("bank", note("4815"))).unwrap();

    assert!(matches!(vault.resolve("nowhere"), Err(Error::NotFound(_))));
    assert!(matches!(vault.resolve("   "), Err(Error::NotFound(_))));
    // A number with no such id falls through to the text search, not silence.
    assert!(matches!(vault.resolve("9999"), Err(Error::NotFound(_))));
}

#[test]
fn a_numeric_title_stays_reachable_when_no_such_id_exists() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let numeric = vault.add(NewItem::new("2024", note("archive"))).unwrap();

    assert_eq!(vault.resolve("2024").unwrap().id, numeric);
}

#[test]
fn an_export_round_trips_through_json() {
    let origin = fixture();
    let mut vault = Vault::create(&origin.path, PASSWORD).unwrap();
    vault
        .add(NewItem::new("bank", note("code 4815")).with_tags(["money", "home"]))
        .unwrap();
    vault
        .add(NewItem::new(
            "mail",
            Payload::Credential(Credential {
                login: "someone".to_owned(),
                password: "hunter2".to_owned(),
                url: Some("https://example.invalid".to_owned()),
                totp: Some("JBSWY3DPEHPK3PXP".to_owned()),
                notes: None,
            }),
        ))
        .unwrap();
    vault
        .add(NewItem::new(
            "keyfile",
            Payload::File {
                filename: "id_ed25519".to_owned(),
                // Bytes that no text encoding would survive.
                bytes: (0..=255u8).collect(),
            },
        ))
        .unwrap();

    let json = sefy_core::exchange::to_json(&sefy_core::exchange::export(&vault).unwrap()).unwrap();

    let destination = fixture();
    let mut restored = Vault::create(&destination.path, b"another password").unwrap();
    let count = sefy_core::exchange::import(
        &mut restored,
        &sefy_core::exchange::from_json(&json).unwrap(),
    )
    .unwrap();
    assert_eq!(count, 3);

    let bank = restored.resolve("bank").unwrap();
    assert_eq!(restored.get(bank.id).unwrap().payload, note("code 4815"));
    assert_eq!(bank.tags, vec!["home", "money"]);

    match restored
        .get(restored.resolve("mail").unwrap().id)
        .unwrap()
        .payload
    {
        Payload::Credential(credential) => {
            assert_eq!(credential.password, "hunter2");
            assert_eq!(credential.totp.as_deref(), Some("JBSWY3DPEHPK3PXP"));
            assert_eq!(credential.notes, None);
        }
        other => panic!("expected a credential, got {other:?}"),
    }

    match restored
        .get(restored.resolve("keyfile").unwrap().id)
        .unwrap()
        .payload
    {
        Payload::File { filename, bytes } => {
            assert_eq!(filename, "id_ed25519");
            assert_eq!(bytes, (0..=255u8).collect::<Vec<u8>>());
        }
        other => panic!("expected a file, got {other:?}"),
    }
}

#[test]
fn an_import_appends_rather_than_merging() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault.add(NewItem::new("bank", note("original"))).unwrap();

    let json = sefy_core::exchange::to_json(&sefy_core::exchange::export(&vault).unwrap()).unwrap();
    sefy_core::exchange::import(&mut vault, &sefy_core::exchange::from_json(&json).unwrap())
        .unwrap();

    // Two items with the same title now, and nothing was overwritten.
    assert_eq!(vault.list().unwrap().len(), 2);
    assert!(matches!(
        vault.resolve("bank"),
        Err(Error::Ambiguous { .. })
    ));
}

#[test]
fn a_malformed_export_is_refused_before_anything_is_inserted() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();

    // The first entry is fine; the second has no text. Nothing may land.
    let json = r#"{
        "sefy_export": 1,
        "items": [
            { "title": "fine", "kind": "note", "text": "here" },
            { "title": "broken", "kind": "note" }
        ]
    }"#;

    let export = sefy_core::exchange::from_json(json).unwrap();
    assert!(matches!(
        sefy_core::exchange::import(&mut vault, &export),
        Err(Error::MalformedExport { index: 1, .. })
    ));
    assert!(vault.list().unwrap().is_empty());
}

#[test]
fn an_export_from_a_future_version_is_refused() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();

    let export = sefy_core::exchange::from_json(r#"{"sefy_export": 99, "items": []}"#).unwrap();
    assert!(matches!(
        sefy_core::exchange::import(&mut vault, &export),
        Err(Error::UnsupportedExport(99))
    ));
}

#[test]
fn something_that_is_not_an_export_is_refused() {
    assert!(matches!(
        sefy_core::exchange::from_json("not json at all"),
        Err(Error::UnreadableExport(_))
    ));
    assert!(matches!(
        sefy_core::exchange::from_json(r#"{"items": []}"#),
        Err(Error::UnreadableExport(_))
    ));
}

#[test]
fn an_empty_password_is_accepted() {
    // Weak, but the caller's business; the library must not silently refuse it.
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, b"").unwrap();
    let id = vault.add(NewItem::new("thing", note("text"))).unwrap();
    vault.save().unwrap();

    assert_eq!(
        Vault::open(&fixture.path, b"")
            .unwrap()
            .get(id)
            .unwrap()
            .payload,
        note("text")
    );
}
