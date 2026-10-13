//! The audit: what it finds, what it leaves alone, and what it never says.
//!
//! Every password and key here is synthetic.

use sefy_core::{Field, ItemKind, NewItem, Payload, Report, Vault, audit, expiry};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD: &[u8] = b"correct horse battery staple";
const DAY: i64 = 86_400;

/// Strong enough that zxcvbn has nothing to say about it.
const STRONG: &str = "vT4#qLw9!zR2@xN7$kP5";
const ALSO_STRONG: &str = "Hq8^mW3&dJ6*rB1%cF0!";

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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn login(title: &str, password: &str) -> NewItem {
    NewItem::new(
        title,
        Payload::fields(
            ItemKind::Login,
            [
                Field::public("login", "ada"),
                Field::secret("password", password),
                Field::public("url", format!("https://{title}.example.com")),
            ],
        ),
    )
}

fn record(title: &str, kind: ItemKind, fields: Vec<Field>) -> NewItem {
    NewItem::new(title, Payload::fields(kind, fields))
}

fn titles(places: &[sefy_core::Place]) -> Vec<&str> {
    places.iter().map(|place| place.title.as_str()).collect()
}

fn days_from_now(days: i64) -> String {
    let (year, month, day) = sefy_core::time::civil_from_days(now().div_euclid(DAY) + days);
    format!("{year:04}-{month:02}-{day:02}")
}

#[test]
fn a_password_people_would_guess_is_weak_and_a_pin_is_not_audited_at_all() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault.add(login("forum", "password1")).unwrap();
    vault.add(login("mail", STRONG)).unwrap();
    vault
        .add(record(
            "home",
            ItemKind::Wifi,
            vec![
                Field::public("ssid", "attic"),
                Field::secret("password", "12345678"),
            ],
        ))
        .unwrap();
    vault
        .add(record(
            "visa",
            ItemKind::Card,
            vec![
                Field::secret("number", "4111111111111111"),
                Field::secret("cvv", "123"),
                Field::secret("pin", "1234"),
            ],
        ))
        .unwrap();

    let report = audit(&vault, now()).unwrap();
    let weak: Vec<(&str, &str)> = report
        .weak
        .iter()
        .map(|weak| (weak.place.title.as_str(), weak.place.field.as_str()))
        .collect();
    assert_eq!(weak.len(), 2, "{weak:?}");
    assert!(weak.contains(&("forum", "password")));
    assert!(weak.contains(&("home", "password")));
    assert!(report.weak.iter().all(|weak| weak.score < 3));
    assert_eq!(report.passwords, 3, "the PIN and the CVV are not passwords");
}

#[test]
fn the_line_between_weak_and_not_is_a_score_of_three() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    // Two words zxcvbn knows: 2 of 4. A word, another and two digits: 3.
    assert_eq!(sefy_core::estimate("correcthorse", &[]).score, 2);
    assert_eq!(sefy_core::estimate("bluewhale42", &[]).score, 3);
    vault.add(login("router", "correcthorse")).unwrap();
    vault.add(login("garden", "bluewhale42")).unwrap();

    let report = audit(&vault, now()).unwrap();
    let weak: Vec<&str> = report
        .weak
        .iter()
        .map(|weak| weak.place.title.as_str())
        .collect();
    assert_eq!(weak, ["router"]);
}

#[test]
fn a_password_built_from_its_own_record_counts_against_it() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    // Twenty letters zxcvbn rates at the top on their own; they are the
    // site's name and the account's, which an attacker who knows whose
    // account it is tries first.
    let built = "QuillfeatherLovelace";
    let alone = sefy_core::estimate(built, &[]).score;
    assert!(alone >= 3, "the premise: {alone}");

    vault
        .add(NewItem::new(
            "quillfeather",
            Payload::fields(
                ItemKind::Login,
                [
                    Field::public("login", "lovelace@mail.example.com"),
                    Field::secret("password", built),
                    Field::public("url", "https://www.quillfeather.net/sign-in"),
                ],
            ),
        ))
        .unwrap();
    let report = audit(&vault, now()).unwrap();
    assert_eq!(report.weak.len(), 1, "{:?}", report.weak);
}

#[test]
fn one_password_in_several_items_is_one_finding_naming_every_item() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault.add(login("bank", STRONG)).unwrap();
    vault.add(login("mail", ALSO_STRONG)).unwrap();
    vault.add(login("forum", STRONG)).unwrap();
    vault
        .add(record(
            "home",
            ItemKind::Wifi,
            vec![
                Field::public("ssid", "attic"),
                Field::secret("password", STRONG),
            ],
        ))
        .unwrap();
    // A password added to a kind that has none is still a password.
    vault
        .add(record(
            "savings",
            ItemKind::Bank,
            vec![
                Field::secret("account", "DE00 0000"),
                Field::secret("password", ALSO_STRONG),
            ],
        ))
        .unwrap();
    // The same value twice in one item is not reuse between items.
    vault
        .add(record(
            "lonely",
            ItemKind::SshKey,
            vec![
                Field::secret("private-key", "synthetic key"),
                Field::secret("passphrase", "Zk2!pW9#vQ4$mR7&"),
                Field::secret("password", "Zk2!pW9#vQ4$mR7&"),
            ],
        ))
        .unwrap();

    let report = audit(&vault, now()).unwrap();
    let groups: Vec<Vec<&str>> = report.reused.iter().map(|group| titles(group)).collect();
    assert_eq!(
        groups,
        [vec!["bank", "forum", "home"], vec!["mail", "savings"]]
    );
}

#[test]
fn an_issued_secret_in_two_items_is_a_duplicate_and_short_ones_are_left_alone() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    for title in ["visa", "visa renewed"] {
        vault
            .add(record(
                title,
                ItemKind::Card,
                vec![
                    Field::secret("number", "4111111111111111"),
                    Field::secret("cvv", "123"),
                    Field::secret("pin", "0000"),
                ],
            ))
            .unwrap();
    }
    vault
        .add(record(
            "ci",
            ItemKind::ApiToken,
            vec![Field::secret("token", "ghp_synthetic")],
        ))
        .unwrap();
    vault
        .add(record(
            "deploy",
            ItemKind::Login,
            vec![
                Field::public("login", "bot"),
                Field::secret("token", "ghp_synthetic"),
            ],
        ))
        .unwrap();
    // The same digits as a card number, but an account is another thing.
    vault
        .add(record(
            "current",
            ItemKind::Bank,
            vec![Field::secret("account", "4111111111111111")],
        ))
        .unwrap();

    let report = audit(&vault, now()).unwrap();
    let groups: Vec<(Vec<&str>, &str)> = report
        .duplicated
        .iter()
        .map(|group| (titles(group), group[0].field.as_str()))
        .collect();
    assert_eq!(
        groups,
        [
            (vec!["visa", "visa renewed"], "number"),
            (vec!["ci", "deploy"], "token"),
        ]
    );
    assert!(report.reused.is_empty());
}

#[test]
fn a_password_is_old_by_when_it_took_its_value_not_by_when_the_item_changed() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let long_ago = now() - 500 * DAY;
    let id = vault
        .add_existing(
            login("mail", STRONG),
            "6f1c2a3b-4d5e-4f60-8a71-92b3c4d5e6f7",
            long_ago,
            long_ago,
        )
        .unwrap();
    vault.add(login("fresh", ALSO_STRONG)).unwrap();

    let report = audit(&vault, now()).unwrap();
    assert_eq!(titles_of_old(&report), ["mail"]);
    assert_eq!(report.old[0].since, long_ago);

    // A new address is a new version, and the password in it is as old as
    // ever.
    let mut moved = vault.get(id).unwrap().payload;
    if let Payload::Fields { fields, .. } = &mut moved {
        fields[2].value = "https://post.example.com".to_owned();
    }
    vault.update(id, None, Some(moved), None).unwrap();
    let report = audit(&vault, now()).unwrap();
    assert_eq!(titles_of_old(&report), ["mail"]);
    assert_eq!(report.old[0].since, long_ago);

    // A new password is not old, however old the item.
    let mut changed = vault.get(id).unwrap().payload;
    if let Payload::Fields { fields, .. } = &mut changed {
        fields[1].value = "Nw5@tY8!gS3#hU6^".to_owned();
    }
    vault.update(id, None, Some(changed), None).unwrap();
    let report = audit(&vault, now()).unwrap();
    assert!(report.old.is_empty(), "{:?}", report.old);

    // Looked at a year and a day later, both are.
    let report = audit(&vault, now() + 366 * DAY).unwrap();
    assert_eq!(titles_of_old(&report), ["mail", "fresh"]);
}

fn titles_of_old(report: &Report) -> Vec<&str> {
    let mut titles: Vec<&str> = report
        .old
        .iter()
        .map(|old| old.place.title.as_str())
        .collect();
    titles.sort_by_key(|title| *title != "mail");
    titles
}

#[test]
fn records_past_or_near_their_date_are_named_and_the_rest_are_not() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    let token = |title: &str, expires: &str| {
        record(
            title,
            ItemKind::ApiToken,
            vec![
                Field::secret("token", format!("tok-{title}")),
                Field::public(expiry::FIELD, expires),
            ],
        )
    };
    vault.add(token("gone", &days_from_now(-3))).unwrap();
    vault.add(token("today", &days_from_now(0))).unwrap();
    vault.add(token("soon", &days_from_now(10))).unwrap();
    vault.add(token("month", &days_from_now(30))).unwrap();
    vault.add(token("later", &days_from_now(31))).unwrap();
    vault.add(token("odd", "whenever")).unwrap();
    vault.add(token("long gone", &days_from_now(-40))).unwrap();
    // A card whose old printed date was not one sefy could read.
    vault
        .add(record(
            "old card",
            ItemKind::Card,
            vec![
                Field::secret("number", "5500000000000004"),
                Field::public(expiry::LEGACY_CARD_NAME, "2029"),
            ],
        ))
        .unwrap();

    let report = audit(&vault, now()).unwrap();
    let names = |dues: &[sefy_core::Due]| -> Vec<String> {
        dues.iter().map(|due| due.place.title.clone()).collect()
    };
    assert_eq!(names(&report.expired), ["long gone", "gone"]);
    assert_eq!(names(&report.expiring), ["today", "soon", "month"]);
    let unreadable: Vec<(&str, &str, &str)> = report
        .unreadable
        .iter()
        .map(|odd| {
            (
                odd.place.title.as_str(),
                odd.place.field.as_str(),
                odd.value.as_str(),
            )
        })
        .collect();
    assert_eq!(
        unreadable,
        [
            ("odd", "expires", "whenever"),
            ("old card", "expiry", "2029")
        ]
    );
    // Six dates; "whenever" is not one.
    assert_eq!(report.dated, 6);
}

#[test]
fn a_clean_vault_reports_nothing_and_counts_what_it_looked_at() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault.add(login("mail", STRONG)).unwrap();
    vault
        .add(NewItem::new(
            "diary",
            Payload::Note {
                text: "password1 expires: yesterday".to_owned(),
            },
        ))
        .unwrap();

    let report = audit(&vault, now()).unwrap();
    assert_eq!(report.findings(), 0, "{report:?}");
    assert_eq!(report.items, 2);
    assert_eq!(report.passwords, 1);
    assert_eq!(report.dated, 0);
}

#[test]
fn a_report_names_places_and_never_a_secret() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault.add(login("forum", "password1")).unwrap();
    vault.add(login("shop", "password1")).unwrap();
    for title in ["visa", "visa again"] {
        vault
            .add(record(
                title,
                ItemKind::Card,
                vec![Field::secret("number", "4111111111111111")],
            ))
            .unwrap();
    }

    let report = audit(&vault, now() + 800 * DAY).unwrap();
    assert!(report.findings() >= 4, "{report:?}");
    let shown = format!("{report:?}");
    assert!(!shown.contains("password1"), "{shown}");
    assert!(!shown.contains("4111"), "{shown}");
}

#[test]
fn an_audit_changes_nothing() {
    let fixture = fixture();
    let mut vault = Vault::create(&fixture.path, PASSWORD).unwrap();
    vault.add(login("forum", "password1")).unwrap();
    vault.save().unwrap();
    let before = std::fs::read(&fixture.path).unwrap();
    let writes = vault.writes();

    audit(&vault, now()).unwrap();
    assert_eq!(vault.writes(), writes);
    assert_eq!(std::fs::read(&fixture.path).unwrap(), before);
}
