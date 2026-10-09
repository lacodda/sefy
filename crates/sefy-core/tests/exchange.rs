//! Moving contents in and out: sefy's own JSON, KeePass XML, Bitwarden JSON
//! and CSVs of passwords, read and written the way the programs that make them
//! do. Every fixture here is synthetic.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use sefy_core::{
    Error, Field, Format, ImportReport, ItemKind, NewItem, Outcome, Payload, Target, Vault,
    exchange, merge,
};
use std::io::Write;
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

fn empty_vault(fixture: &Fixture) -> Vault {
    Vault::create(&fixture.path, PASSWORD).unwrap()
}

fn note(text: &str) -> Payload {
    Payload::Note {
        text: text.to_owned(),
    }
}

fn payload_of(vault: &Vault, title: &str) -> Payload {
    vault.get(vault.resolve(title).unwrap().id).unwrap().payload
}

fn field(payload: &Payload, name: &str) -> Option<(String, bool)> {
    payload
        .field(name)
        .map(|field| (field.value.clone(), field.secret))
}

fn notice<'a>(report: &'a ImportReport, title: &str) -> &'a sefy_core::Notice {
    report
        .notices
        .iter()
        .find(|notice| notice.title == title)
        .unwrap_or_else(|| panic!("no notice about {title:?} in {:?}", report.notices))
}

fn count(report: &ImportReport, kind: ItemKind) -> usize {
    report
        .added
        .iter()
        .find(|(added, _)| *added == kind)
        .map_or(0, |(_, count)| *count)
}

/// A vault holding one item of every kind, two of them edited once.
fn every_kind(fixture: &Fixture) -> Vault {
    let mut vault = empty_vault(fixture);
    let bank = vault
        .add(NewItem::new("bank", note("code 4815")).with_tags(["money", "home"]))
        .unwrap();
    vault
        .update(
            bank,
            None,
            Some(note("code 1623\r\nline two & <three>")),
            None,
        )
        .unwrap();
    let mail = vault
        .add(NewItem::new(
            "mail",
            Payload::fields(
                ItemKind::Login,
                [
                    Field::public("login", "someone"),
                    Field::secret("password", "first"),
                    Field::public("url", "https://mail.example.invalid"),
                    Field::secret("totp", "JBSWY3DPEHPK3PXP"),
                    Field::public("notes", "the old one"),
                    Field::secret("recovery", "abc-def"),
                ],
            ),
        ))
        .unwrap();
    let mut current = vault.get(mail).unwrap().payload;
    if let Payload::Fields { fields, .. } = &mut current {
        fields[1].value = "hunter2".to_owned();
    }
    vault.update(mail, None, Some(current), None).unwrap();
    vault
        .add(NewItem::new(
            "linked",
            Payload::fields(
                ItemKind::Login,
                [
                    Field::public("login", "ada"),
                    Field::secret("password", "pw"),
                    Field::secret(
                        "totp",
                        "otpauth://totp/Example:ada?secret=JBSWY3DPEHPK3PXP&issuer=Example&digits=8",
                    ),
                ],
            ),
        ))
        .unwrap();
    vault
        .add(
            NewItem::new(
                "visa",
                Payload::fields(
                    ItemKind::Card,
                    [
                        Field::secret("number", "4111111111111111"),
                        Field::public("holder", "A LOVELACE"),
                        Field::public("expiry", "03/2029"),
                        Field::secret("cvv", "123"),
                    ],
                ),
            )
            .with_tags(["money"]),
        )
        .unwrap();
    vault
        .add(NewItem::new(
            "home wifi",
            Payload::fields(
                ItemKind::Wifi,
                [
                    Field::public("ssid", "attic"),
                    Field::secret("password", "wpa-secret"),
                ],
            ),
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
    vault.save().unwrap();
    vault
}

/// Every item's title, kind, tags and contents, for comparing two vaults.
fn contents(vault: &Vault) -> Vec<(String, String, Vec<String>, Payload)> {
    let mut all: Vec<_> = vault
        .list()
        .unwrap()
        .into_iter()
        .map(|summary| {
            let payload = vault.get(summary.id).unwrap().payload;
            (
                summary.title,
                summary.kind.as_str().to_owned(),
                summary.tags,
                payload,
            )
        })
        .collect();
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all
}

// sefy's own JSON

#[test]
fn a_sefy_export_round_trips_every_kind_with_identity_and_times() {
    let origin = fixture();
    let vault = every_kind(&origin);
    let exported = exchange::export(&vault, Target::Sefy { history: false }).unwrap();
    assert_eq!(exported.report.written, 6);
    assert_eq!(exported.report.versions, 0);

    let destination = fixture();
    let mut restored = Vault::create(&destination.path, b"another password").unwrap();
    let report = exchange::import(&mut restored, &exported.text).unwrap();
    assert_eq!(report.format, Format::Sefy);
    assert_eq!(report.added_total(), 6);
    assert_eq!(count(&report, ItemKind::Login), 2);
    assert_eq!(contents(&restored), contents(&vault));

    for summary in vault.list().unwrap() {
        let there = restored
            .summary(restored.find_by_uuid(&summary.uuid).unwrap().unwrap())
            .unwrap();
        assert_eq!(there.created_at, summary.created_at, "{}", summary.title);
        assert_eq!(there.updated_at, summary.updated_at, "{}", summary.title);
    }
}

#[test]
fn history_leaves_only_when_asked_for_and_arrives_whole() {
    let origin = fixture();
    let vault = every_kind(&origin);

    let snapshot = exchange::export(&vault, Target::Sefy { history: false }).unwrap();
    assert!(!snapshot.text.contains("\"history\""));
    assert!(
        !snapshot.text.contains("\"first\""),
        "an old password leaked"
    );

    let full = exchange::export(&vault, Target::Sefy { history: true }).unwrap();
    assert_eq!(full.report.versions, 2);
    assert!(full.text.contains("\"first\""));

    let destination = fixture();
    let mut restored = empty_vault(&destination);
    let report = exchange::import(&mut restored, &full.text).unwrap();
    assert_eq!(report.versions, 2);

    let mail = restored.resolve("mail").unwrap().id;
    let history = restored.history(mail).unwrap();
    let original = vault.history(vault.resolve("mail").unwrap().id).unwrap();
    assert_eq!(
        history, original,
        "versions keep their identity, place and contents"
    );
}

#[test]
fn an_imported_export_merges_back_without_a_false_conflict() {
    // The reason an export carries the version its contents are: imported
    // under a fresh one, the copy and its origin would each think the other's
    // contents a stranger, and the origin's next edit would merge in as a
    // conflict instead of an update.
    let origin = fixture();
    let mut vault = every_kind(&origin);
    let exported = exchange::export(&vault, Target::Sefy { history: false }).unwrap();

    let copy = fixture();
    let mut imported = empty_vault(&copy);
    exchange::import(&mut imported, &exported.text).unwrap();

    let bank = vault.resolve("bank").unwrap().id;
    vault
        .update(bank, None, Some(note("changed after the export")), None)
        .unwrap();
    vault.save().unwrap();

    let report = merge(&mut imported, &vault).unwrap();
    assert!(report.conflicts.is_empty(), "{:?}", report.conflicts);
    assert_eq!(report.updated.len(), 1);
    assert_eq!(
        payload_of(&imported, "bank"),
        note("changed after the export")
    );
}

#[test]
fn re_importing_an_export_does_not_duplicate_what_is_already_here() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    vault.add(NewItem::new("bank", note("original"))).unwrap();

    let exported = exchange::export(&vault, Target::Sefy { history: false }).unwrap();
    let report = exchange::import(&mut vault, &exported.text).unwrap();

    assert_eq!(report.added_total(), 0);
    assert_eq!(report.skipped, 1);
    assert_eq!(vault.list().unwrap().len(), 1);
}

#[test]
fn an_import_keeps_the_vault_as_it_was_beside_it_and_a_futile_one_writes_nothing() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    vault.add(NewItem::new("bank", note("original"))).unwrap();
    vault.save().unwrap();
    let before = std::fs::read(&fixture.path).unwrap();
    let file = r#"{"sefy_export":1,"items":[
        {"uuid":"0d6f3d5e-0000-4000-8000-000000000001","title":"mail","kind":"note","text":"imported"}
    ]}"#;

    let report = exchange::import(&mut vault, file).unwrap();

    let kept = report
        .kept_copy
        .expect("an import that adds keeps a copy first");
    assert_eq!(std::fs::read(&kept).unwrap(), before);

    // The same file again adds nothing, so it must neither write nor push
    // the useful copy down the rotation.
    let writes = vault.writes();
    let again = exchange::import(&mut vault, file).unwrap();
    assert_eq!(again.added_total(), 0);
    assert_eq!(again.kept_copy, None);
    assert_eq!(vault.writes(), writes);
    assert_eq!(sefy_core::copies::list(&fixture.path).len(), 1);
}

#[test]
fn an_entry_without_an_identity_is_always_added() {
    // Exports written by 0.1.x carry no uuid, and neither does JSON written by
    // hand. Matching on titles instead would silently collapse two accounts
    // that happen to share a name.
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    vault.add(NewItem::new("bank", note("original"))).unwrap();

    let legacy = r#"{"sefy_export":1,"items":[
        {"title":"bank","kind":"note","text":"from an older export"}
    ]}"#;
    let report = exchange::import(&mut vault, legacy).unwrap();

    assert_eq!(report.added_total(), 1);
    assert!(matches!(
        vault.resolve("bank"),
        Err(Error::Ambiguous { .. })
    ));
}

#[test]
fn a_flat_login_from_before_fields_still_imports() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let old = r#"{"sefy_export":1,"items":[
        {"title":"mail","kind":"credential","login":"someone","password":"hunter2"}
    ]}"#;
    exchange::import(&mut vault, old).unwrap();
    let mail = payload_of(&vault, "mail");
    assert_eq!(mail.kind(), ItemKind::Login);
    assert_eq!(field(&mail, "password"), Some(("hunter2".to_owned(), true)));
}

#[test]
fn a_malformed_export_is_refused_before_anything_is_inserted() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let json = r#"{
        "sefy_export": 1,
        "items": [
            { "title": "fine", "kind": "note", "text": "here" },
            { "title": "broken", "kind": "note" }
        ]
    }"#;
    assert!(matches!(
        exchange::import(&mut vault, json),
        Err(Error::MalformedExport { index: 1, .. })
    ));
    assert!(vault.list().unwrap().is_empty());
}

#[test]
fn an_export_from_a_future_version_is_refused() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    assert!(matches!(
        exchange::import(&mut vault, r#"{"sefy_export": 99, "items": []}"#),
        Err(Error::UnsupportedExport(99))
    ));
}

#[test]
fn a_file_that_is_none_of_the_formats_is_refused_by_name() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    for text in [
        "not json at all",
        r#"{"items": []}"#,
        "<html><body/></html>",
        "date,amount\n2024-01-01,12\n",
    ] {
        assert!(
            matches!(
                exchange::import(&mut vault, text),
                Err(Error::UnrecognizedImport)
            ),
            "{text}"
        );
    }
    assert!(vault.list().unwrap().is_empty());
}

// KeePass XML

/// A KeePass time as KDBX 4 writes it in a database's own XML: seconds since
/// the year one, little-endian, in base64.
fn kdbx4_time(unix: i64) -> String {
    BASE64.encode((unix + 62_135_596_800).to_le_bytes())
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

/// A database as KeePass 2 exports it: groups, a recycle bin, history, the
/// native one-time password parts, a compressed attachment.
fn keepass_fixture() -> String {
    let attachment = BASE64.encode(gzip(b"-----BEGIN OPENSSH PRIVATE KEY-----\nsynthetic\n"));
    let lone = BASE64.encode(gzip(b"lone attachment"));
    format!(
        r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<KeePassFile>
	<Meta>
		<Generator>KeePass</Generator>
		<RecycleBinEnabled>True</RecycleBinEnabled>
		<RecycleBinUUID>cHFyc3R1dnd4eXp7fH1+fw==</RecycleBinUUID>
		<Binaries>
			<Binary ID="0" Compressed="True">{attachment}</Binary>
			<Binary ID="1" Compressed="True">{lone}</Binary>
		</Binaries>
	</Meta>
	<Root>
		<Group>
			<UUID>AAAAAAAAAAAAAAAAAAAAAA==</UUID>
			<Name>Database</Name>
			<Entry>
				<UUID>AAECAwQFBgcICQoLDA0ODw==</UUID>
				<Tags>work;mail</Tags>
				<Times>
					<CreationTime>2024-01-01T00:00:00Z</CreationTime>
					<LastModificationTime>2024-03-01T00:00:00Z</LastModificationTime>
				</Times>
				<String><Key>Title</Key><Value>mail</Value></String>
				<String><Key>UserName</Key><Value>someone</Value></String>
				<String><Key>Password</Key><Value ProtectInMemory="True">now &amp; &lt;then&gt;</Value></String>
				<String><Key>URL</Key><Value>https://mail.example.invalid</Value></String>
				<String><Key>Notes</Key><Value>line one&#13;
line two</Value></String>
				<String><Key>Recovery</Key><Value ProtectInMemory="True">abc-def</Value></String>
				<String><Key>Hint</Key><Value>the usual</Value></String>
				<String><Key>TimeOtp-Secret-Base32</Key><Value ProtectInMemory="True">JBSWY3DPEHPK3PXP</Value></String>
				<History>
					<Entry>
						<UUID>AAECAwQFBgcICQoLDA0ODw==</UUID>
						<Times><LastModificationTime>2024-01-01T00:00:00Z</LastModificationTime></Times>
						<String><Key>Title</Key><Value>mail</Value></String>
						<String><Key>UserName</Key><Value>someone</Value></String>
						<String><Key>Password</Key><Value ProtectInMemory="True">oldest</Value></String>
					</Entry>
					<Entry>
						<UUID>AAECAwQFBgcICQoLDA0ODw==</UUID>
						<Times><LastModificationTime>2024-01-15T00:00:00Z</LastModificationTime></Times>
						<String><Key>Title</Key><Value>mail (renamed)</Value></String>
						<String><Key>UserName</Key><Value>someone</Value></String>
						<String><Key>Password</Key><Value ProtectInMemory="True">oldest</Value></String>
					</Entry>
					<Entry>
						<UUID>AAECAwQFBgcICQoLDA0ODw==</UUID>
						<Times><LastModificationTime>2024-02-01T00:00:00Z</LastModificationTime></Times>
						<String><Key>Title</Key><Value>mail</Value></String>
						<String><Key>UserName</Key><Value>someone</Value></String>
						<String><Key>Password</Key><Value ProtectInMemory="True">older</Value></String>
					</Entry>
				</History>
			</Entry>
			<Group>
				<UUID>EBESExQVFhcYGRobHB0eHw==</UUID>
				<Name>Internet</Name>
				<Group>
					<UUID>ICEiIyQlJicoKSorLC0uLw==</UUID>
					<Name>Shops</Name>
					<Entry>
						<UUID>MDEyMzQ1Njc4OTo7PD0+Pw==</UUID>
						<Times>
							<CreationTime>{created}</CreationTime>
							<LastModificationTime>{created}</LastModificationTime>
						</Times>
						<String><Key>Title</Key><Value>shop</Value></String>
						<String><Key>UserName</Key><Value>ada</Value></String>
						<String><Key>Password</Key><Value ProtectInMemory="True">pw</Value></String>
						<String><Key>TimeOtp-Secret-Base32</Key><Value>JBSWY3DPEHPK3PXP</Value></String>
						<String><Key>TimeOtp-Algorithm</Key><Value>HMAC-SHA-256</Value></String>
						<String><Key>TimeOtp-Length</Key><Value>8</Value></String>
						<Binary><Key>id_ed25519</Key><Value Ref="0" /></Binary>
					</Entry>
					<Entry>
						<UUID>QEFCQ0RFRkdISUpLTE1OTw==</UUID>
						<String><Key>Title</Key><Value>xc</Value></String>
						<String><Key>Password</Key><Value>p</Value></String>
						<String><Key>otp</Key><Value ProtectInMemory="True">otpauth://totp/Example:ada?secret=JBSWY3DPEHPK3PXP&amp;issuer=Example</Value></String>
					</Entry>
				</Group>
			</Group>
			<Entry>
				<UUID>UFFSU1RVVldYWVpbXF1eXw==</UUID>
				<String><Key>Title</Key><Value>shed</Value></String>
				<String><Key>UserName</Key><Value></Value></String>
				<String><Key>Password</Key><Value ProtectInMemory="True"></Value></String>
				<String><Key>Notes</Key><Value>combination 4815</Value></String>
			</Entry>
			<Entry>
				<UUID>YGFiY2RlZmdoaWprbG1ubw==</UUID>
				<String><Key>Title</Key><Value>scan</Value></String>
				<Binary><Key>passport.pdf</Key><Value Ref="1" /></Binary>
			</Entry>
			<Entry>
				<UUID>gIGCg4SFhoeIiYqLjI2Ojw==</UUID>
				<String><Key>Title</Key><Value>empty</Value></String>
			</Entry>
			<Group>
				<UUID>cHFyc3R1dnd4eXp7fH1+fw==</UUID>
				<Name>Recycle Bin</Name>
				<Entry>
					<UUID>kJGSk5SVlpeYmZqbnJ2enw==</UUID>
					<String><Key>Title</Key><Value>thrown away</Value></String>
					<String><Key>Password</Key><Value>gone</Value></String>
				</Entry>
			</Group>
		</Group>
		<DeletedObjects />
	</Root>
</KeePassFile>
"#,
        created = kdbx4_time(1_709_164_800),
    )
}

#[test]
fn a_keepass_export_comes_in_with_groups_as_tags_and_the_bin_left_behind() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let report = exchange::import(&mut vault, &keepass_fixture()).unwrap();

    assert_eq!(report.format, Format::KeePass);
    assert_eq!(count(&report, ItemKind::Login), 3);
    assert_eq!(count(&report, ItemKind::Note), 1);
    assert_eq!(count(&report, ItemKind::File), 2);
    assert_eq!(notice(&report, "thrown away").outcome, Outcome::NotImported);
    assert_eq!(notice(&report, "empty").outcome, Outcome::NotImported);
    assert!(vault.resolve("thrown away").is_err());

    let mail = vault.resolve("mail").unwrap();
    assert_eq!(mail.uuid, "00010203-0405-0607-0809-0a0b0c0d0e0f");
    assert_eq!(mail.tags, ["mail", "work"]);
    assert_eq!(mail.created_at, 1_704_067_200);
    assert_eq!(mail.updated_at, 1_709_251_200);
    let shop = vault.resolve("shop").unwrap();
    assert_eq!(shop.tags, ["Internet/Shops"]);
    assert_eq!(
        shop.created_at, 1_709_164_800,
        "a KDBX 4 binary time is read too"
    );

    assert_eq!(payload_of(&vault, "shed"), note("combination 4815"));
}

#[test]
fn a_keepass_entrys_strings_land_in_the_fields_they_mean() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let report = exchange::import(&mut vault, &keepass_fixture()).unwrap();

    let mail = payload_of(&vault, "mail");
    assert_eq!(field(&mail, "login"), Some(("someone".to_owned(), false)));
    assert_eq!(
        field(&mail, "password"),
        Some(("now & <then>".to_owned(), true))
    );
    assert_eq!(
        field(&mail, "notes"),
        Some(("line one\r\nline two".to_owned(), false)),
        "a carriage return written as a reference survives"
    );
    assert_eq!(field(&mail, "Recovery"), Some(("abc-def".to_owned(), true)));
    assert_eq!(field(&mail, "Hint"), Some(("the usual".to_owned(), false)));
    // KeePass 2's own one-time password parts become one key.
    assert_eq!(
        field(&mail, "totp"),
        Some(("JBSWY3DPEHPK3PXP".to_owned(), true))
    );
    assert!(mail.field("TimeOtp-Secret-Base32").is_none());

    // Parameters a bare key cannot carry make a link instead.
    let shop = payload_of(&vault, "shop");
    let (key, _) = field(&shop, "totp").unwrap();
    assert!(key.starts_with("otpauth://totp/"), "{key}");
    assert!(
        key.contains("algorithm=SHA256") && key.contains("digits=8"),
        "{key}"
    );
    sefy_core::Totp::parse(&key).unwrap();

    // KeePassXC's link is taken as it is.
    assert_eq!(
        field(&payload_of(&vault, "xc"), "totp").unwrap().0,
        "otpauth://totp/Example:ada?secret=JBSWY3DPEHPK3PXP&issuer=Example"
    );
    assert!(report.notices.iter().all(|notice| notice.title != "xc"));
}

#[test]
fn keepass_attachments_become_files_and_a_lone_one_keeps_its_entry() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let report = exchange::import(&mut vault, &keepass_fixture()).unwrap();

    // Beside a login: a file of its own, said so, with the entry's tags.
    assert_eq!(notice(&report, "shop").outcome, Outcome::Reshaped);
    let attached = vault.resolve("shop - id_ed25519").unwrap();
    assert_eq!(attached.tags, ["Internet/Shops"]);
    assert_eq!(
        payload_of(&vault, "shop - id_ed25519"),
        Payload::File {
            filename: "id_ed25519".to_owned(),
            bytes: b"-----BEGIN OPENSSH PRIVATE KEY-----\nsynthetic\n".to_vec(),
        }
    );

    // An entry that is nothing but an attachment is that file.
    let scan = vault.resolve("scan").unwrap();
    assert_eq!(scan.uuid, "60616263-6465-6667-6869-6a6b6c6d6e6f");
    assert_eq!(
        payload_of(&vault, "scan"),
        Payload::File {
            filename: "passport.pdf".to_owned(),
            bytes: b"lone attachment".to_vec(),
        }
    );

    // A second import recognises the attachments as well as the entries.
    let again = exchange::import(&mut vault, &keepass_fixture()).unwrap();
    assert_eq!(again.added_total(), 0);
    assert_eq!(again.skipped, 6);
}

#[test]
fn a_keepass_entrys_history_arrives_as_its_versions() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let report = exchange::import(&mut vault, &keepass_fixture()).unwrap();
    assert_eq!(report.versions, 2);

    let mail = vault.resolve("mail").unwrap().id;
    let history = vault.history(mail).unwrap();
    let passwords: Vec<String> = history
        .iter()
        .map(|version| version.payload.field("password").unwrap().value.clone())
        .collect();
    // The renamed snapshot changed no contents, so it is no version; the
    // first of the pair keeps its time, which is when "oldest" was written.
    assert_eq!(passwords, ["oldest", "older", "now & <then>"]);
    assert_eq!(history[0].made_at, 1_704_067_200);
    assert_eq!(history[1].made_at, 1_706_745_600);
    assert_eq!(
        history
            .iter()
            .map(|version| version.seq)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    // The snapshots knew less than the entry does now, and say so.
    assert!(history[0].payload.field("url").is_none());
}

#[test]
fn an_attachment_the_file_left_out_is_named_and_the_rest_still_arrives() {
    // KeePassXC's XML export writes an entry's attachment as a reference to a
    // pool it does not write. Refusing the file would refuse every entry in it.
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<KeePassFile>
	<Meta><RecycleBinUUID>AAAAAAAAAAAAAAAAAAAAAA==</RecycleBinUUID></Meta>
	<Root>
		<Group>
			<UUID>AAAAAAAAAAAAAAAAAAAAAA==</UUID>
			<Name>Root</Name>
			<Entry>
				<UUID>AAECAwQFBgcICQoLDA0ODw==</UUID>
				<String><Key>Title</Key><Value>server</Value></String>
				<String><Key>Password</Key><Value>pw</Value></String>
				<Binary><Key>id_ed25519</Key><Value Ref="0"/></Binary>
			</Entry>
			<Entry>
				<UUID>EBESExQVFhcYGRobHB0eHw==</UUID>
				<String><Key>Title</Key><Value>scan</Value></String>
				<Binary><Key>passport.pdf</Key><Value Ref="1"/></Binary>
			</Entry>
		</Group>
	</Root>
</KeePassFile>"#;
    let report = exchange::import(&mut vault, xml).unwrap();
    assert_eq!(report.added_total(), 1);
    let server = notice(&report, "server");
    assert_eq!(server.outcome, Outcome::InPart);
    assert!(
        server.reason.contains("\"id_ed25519\""),
        "{}",
        server.reason
    );
    assert!(
        server.reason.contains("attachment-export"),
        "{}",
        server.reason
    );
    assert_eq!(notice(&report, "scan").outcome, Outcome::NotImported);
    assert_eq!(
        field(&payload_of(&vault, "server"), "password").unwrap().0,
        "pw"
    );
}

#[test]
fn a_keepass_file_with_values_still_sealed_is_refused_whole() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let sealed = keepass_fixture().replace(
        "<Value ProtectInMemory=\"True\">abc-def</Value>",
        "<Value Protected=\"True\">q83vEjRWeJA=</Value>",
    );
    assert!(matches!(
        exchange::import(&mut vault, &sealed),
        Err(Error::SealedImport { .. })
    ));
    assert!(vault.list().unwrap().is_empty());
}

#[test]
fn a_keepass_export_round_trips_every_kind() {
    let origin = fixture();
    let vault = every_kind(&origin);
    let exported = exchange::export(&vault, Target::KeePass { history: false }).unwrap();
    assert_eq!(exported.report.written, 6);
    assert!(exported.text.contains("<Key>UserName</Key>"));
    assert!(exported.text.contains("<Key>TimeOtp-Secret-Base32</Key>"));
    assert!(exported.text.contains("<Key>otp</Key>"));
    assert!(!exported.text.contains("first"), "an old password leaked");

    let destination = fixture();
    let mut restored = empty_vault(&destination);
    let report = exchange::import(&mut restored, &exported.text).unwrap();
    assert!(report.notices.is_empty(), "{:?}", report.notices);
    assert_eq!(contents(&restored), contents(&vault));
    for summary in vault.list().unwrap() {
        assert!(
            restored.find_by_uuid(&summary.uuid).unwrap().is_some(),
            "{} kept its identity",
            summary.title
        );
    }

    // And back into the vault it came from, nothing doubles.
    let mut origin_again = vault;
    let again = exchange::import(&mut origin_again, &exported.text).unwrap();
    assert_eq!(again.skipped, 6);
}

#[test]
fn a_keepass_export_carries_history_when_asked() {
    let origin = fixture();
    let vault = every_kind(&origin);
    let exported = exchange::export(&vault, Target::KeePass { history: true }).unwrap();
    assert_eq!(exported.report.versions, 2);
    assert!(exported.text.contains("<History>"));

    let destination = fixture();
    let mut restored = empty_vault(&destination);
    let report = exchange::import(&mut restored, &exported.text).unwrap();
    assert_eq!(report.versions, 2);

    let mail = restored.resolve("mail").unwrap().id;
    let passwords: Vec<String> = restored
        .history(mail)
        .unwrap()
        .iter()
        .map(|version| version.payload.field("password").unwrap().value.clone())
        .collect();
    assert_eq!(passwords, ["first", "hunter2"]);
}

#[test]
fn a_value_xml_cannot_carry_stops_a_keepass_export_by_name() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    vault
        .add(NewItem::new("bell", note("ring \u{7} ring")))
        .unwrap();
    match exchange::export(&vault, Target::KeePass { history: false }) {
        Err(Error::Unexportable { title, .. }) => assert_eq!(title, "bell"),
        Err(other) => panic!("unexpected error {other}"),
        Ok(_) => panic!("a value XML cannot carry was written"),
    }
    // sefy's own JSON carries it.
    exchange::export(&vault, Target::Sefy { history: false }).unwrap();
}

// Bitwarden JSON

const BITWARDEN: &str = r#"{
  "encrypted": false,
  "folders": [
    { "id": "f1", "name": "Personal" },
    { "id": "f2", "name": "Work/Servers" }
  ],
  "items": [
    {
      "id": "6D5E3C2B-1A09-4F8E-9D7C-6B5A4F3E2D1C",
      "type": 1,
      "name": "mail",
      "notes": "the main one",
      "folderId": "f1",
      "favorite": true,
      "fields": [
        { "name": "Recovery", "value": "abc-def", "type": 1 },
        { "name": "Plan", "value": "family", "type": 0 },
        { "name": "Verified", "value": "true", "type": 2 },
        { "name": "Linked", "value": null, "type": 3, "linkedId": 100 }
      ],
      "login": {
        "uris": [
          { "match": null, "uri": "https://mail.example.invalid" },
          { "match": null, "uri": "https://webmail.example.invalid" }
        ],
        "username": "someone",
        "password": "current",
        "totp": "JBSW Y3DP EHPK 3PXP",
        "fido2Credentials": [ { "credentialId": "synthetic" } ]
      },
      "passwordHistory": [
        { "lastUsedDate": "2024-03-01T00:00:00.000Z", "password": "previous" },
        { "lastUsedDate": "2024-02-01T00:00:00.000Z", "password": "Recovery: old-recovery" },
        { "lastUsedDate": "2024-01-15T00:00:00.000Z", "password": "first" }
      ],
      "creationDate": "2024-01-01T00:00:00.000Z",
      "revisionDate": "2024-03-02T00:00:00.000Z",
      "deletedDate": null
    },
    {
      "id": "1b2c3d4e-5f60-4718-9a2b-3c4d5e6f7a8b",
      "type": 2,
      "name": "shed",
      "notes": "combination 4815",
      "folderId": null,
      "fields": [ { "name": "Floor", "value": "2", "type": 0 } ],
      "secureNote": { "type": 0 }
    },
    {
      "id": "2b2c3d4e-5f60-4718-9a2b-3c4d5e6f7a8b",
      "type": 3,
      "name": "visa",
      "folderId": "f1",
      "card": {
        "cardholderName": "A LOVELACE", "brand": "Visa", "number": "4111111111111111",
        "expMonth": "3", "expYear": "2029", "code": "123"
      }
    },
    {
      "id": "3b2c3d4e-5f60-4718-9a2b-3c4d5e6f7a8b",
      "type": 4,
      "name": "me",
      "notes": "keep current",
      "identity": {
        "title": "Ms", "firstName": "Ada", "middleName": null, "lastName": "Lovelace",
        "passportNumber": "X1234567", "email": "ada@example.invalid"
      }
    },
    {
      "id": "4b2c3d4e-5f60-4718-9a2b-3c4d5e6f7a8b",
      "type": 5,
      "name": "server key",
      "folderId": "f2",
      "sshKey": {
        "privateKey": "-----BEGIN OPENSSH PRIVATE KEY-----\nsynthetic\n",
        "publicKey": "ssh-ed25519 AAAA synthetic",
        "keyFingerprint": "SHA256:synthetic"
      }
    },
    {
      "id": "5b2c3d4e-5f60-4718-9a2b-3c4d5e6f7a8b",
      "type": 6,
      "name": "savings",
      "bankAccount": {
        "bankName": "Example Bank", "nameOnAccount": "A Lovelace", "accountNumber": "12345678",
        "iban": "GB00EXMP00000012345678", "swiftCode": "EXMPGB2L", "routingNumber": null, "pin": "0000"
      }
    },
    {
      "id": "6b2c3d4e-5f60-4718-9a2b-3c4d5e6f7a8b",
      "type": 1,
      "name": "steam",
      "login": { "username": "ada", "password": "s", "totp": "steam://ABCDEFGHIJ" }
    },
    {
      "id": "7b2c3d4e-5f60-4718-9a2b-3c4d5e6f7a8b",
      "type": 1,
      "name": "deleted",
      "login": { "username": "gone", "password": "gone" },
      "deletedDate": "2024-02-02T00:00:00.000Z"
    },
    {
      "id": "8b2c3d4e-5f60-4718-9a2b-3c4d5e6f7a8b",
      "type": 99,
      "name": "from the future"
    }
  ]
}"#;

#[test]
fn a_bitwarden_export_maps_every_type_and_names_what_it_could_not() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let report = exchange::import(&mut vault, BITWARDEN).unwrap();

    assert_eq!(report.format, Format::Bitwarden);
    assert_eq!(count(&report, ItemKind::Login), 2);
    assert_eq!(count(&report, ItemKind::Note), 2);
    assert_eq!(count(&report, ItemKind::Card), 1);
    assert_eq!(count(&report, ItemKind::SshKey), 1);
    assert_eq!(count(&report, ItemKind::Bank), 1);

    assert_eq!(notice(&report, "deleted").outcome, Outcome::NotImported);
    assert_eq!(
        notice(&report, "from the future").outcome,
        Outcome::NotImported
    );
    assert_eq!(notice(&report, "me").outcome, Outcome::Reshaped);
    assert_eq!(notice(&report, "steam").outcome, Outcome::Reshaped);
    let about_mail: Vec<_> = report
        .notices
        .iter()
        .filter(|notice| notice.title == "mail")
        .map(|notice| notice.reason.as_str())
        .collect();
    assert!(about_mail.iter().any(|reason| reason.contains("passkey")));
    assert!(
        about_mail
            .iter()
            .any(|reason| reason.contains("\"Linked\""))
    );

    let mail = vault.resolve("mail").unwrap();
    assert_eq!(mail.uuid, "6d5e3c2b-1a09-4f8e-9d7c-6b5a4f3e2d1c");
    assert_eq!(mail.tags, ["Personal"]);
    assert_eq!(mail.created_at, 1_704_067_200);
    assert_eq!(mail.updated_at, 1_709_337_600);
    assert_eq!(vault.resolve("server key").unwrap().tags, ["Work/Servers"]);

    let mail = payload_of(&vault, "mail");
    assert_eq!(
        field(&mail, "url").unwrap().0,
        "https://mail.example.invalid"
    );
    assert_eq!(
        field(&mail, "url-2").unwrap().0,
        "https://webmail.example.invalid"
    );
    assert_eq!(
        field(&mail, "totp"),
        Some(("JBSWY3DPEHPK3PXP".to_owned(), true))
    );
    assert_eq!(field(&mail, "notes").unwrap().0, "the main one");
    assert_eq!(field(&mail, "Recovery"), Some(("abc-def".to_owned(), true)));
    assert_eq!(field(&mail, "Plan"), Some(("family".to_owned(), false)));
    assert_eq!(field(&mail, "Verified"), Some(("true".to_owned(), false)));
    assert!(mail.field("Linked").is_none());

    assert_eq!(
        payload_of(&vault, "shed"),
        note("combination 4815\nFloor: 2")
    );
    let visa = payload_of(&vault, "visa");
    assert_eq!(
        field(&visa, "number"),
        Some(("4111111111111111".to_owned(), true))
    );
    assert_eq!(field(&visa, "expiry").unwrap().0, "03/2029");
    assert_eq!(field(&visa, "cvv"), Some(("123".to_owned(), true)));
    assert_eq!(field(&visa, "brand").unwrap().0, "Visa");

    let Payload::Note { text } = payload_of(&vault, "me") else {
        panic!("an identity is kept as a note");
    };
    assert!(text.contains("first name: Ada"), "{text}");
    assert!(text.contains("passport number: X1234567"), "{text}");
    assert!(text.ends_with("keep current"), "{text}");
    assert!(!text.contains("middle name"), "an empty field is no line");

    let key = payload_of(&vault, "server key");
    assert_eq!(key.kind(), ItemKind::SshKey);
    assert!(field(&key, "private-key").unwrap().1);
    assert_eq!(field(&key, "fingerprint").unwrap().0, "SHA256:synthetic");

    let savings = payload_of(&vault, "savings");
    assert_eq!(
        field(&savings, "account"),
        Some(("12345678".to_owned(), true))
    );
    assert_eq!(field(&savings, "iban").unwrap().0, "GB00EXMP00000012345678");
    assert_eq!(field(&savings, "routing").unwrap().0, "EXMPGB2L");
    assert_eq!(field(&savings, "pin"), Some(("0000".to_owned(), true)));

    assert_eq!(
        field(&payload_of(&vault, "steam"), "otp"),
        Some(("steam://ABCDEFGHIJ".to_owned(), true)),
        "a key sefy cannot read is kept, not dropped"
    );
}

#[test]
fn bitwardens_password_history_becomes_versions_one_change_apart() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let report = exchange::import(&mut vault, BITWARDEN).unwrap();
    assert_eq!(report.versions, 3);

    let mail = vault.resolve("mail").unwrap().id;
    let history = vault.history(mail).unwrap();
    let states: Vec<(String, String, i64)> = history
        .iter()
        .map(|version| {
            (
                version.payload.field("password").unwrap().value.clone(),
                version.payload.field("Recovery").unwrap().value.clone(),
                version.made_at,
            )
        })
        .collect();
    assert_eq!(
        states,
        [
            // Written when the item was created; replaced on 15 January.
            ("first".to_owned(), "old-recovery".to_owned(), 1_704_067_200),
            // A hidden field's old value is that field's, not the password's.
            (
                "previous".to_owned(),
                "old-recovery".to_owned(),
                1_705_276_800
            ),
            ("previous".to_owned(), "abc-def".to_owned(), 1_706_745_600),
            ("current".to_owned(), "abc-def".to_owned(), 1_709_337_600),
        ]
    );
}

#[test]
fn a_second_import_says_nothing_about_entries_it_skipped() {
    // "Its passkey stays behind" about an item that was already here and left
    // alone would describe something that did not happen this time.
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    exchange::import(&mut vault, BITWARDEN).unwrap();
    let again = exchange::import(&mut vault, BITWARDEN).unwrap();
    assert_eq!(again.skipped, 7);
    assert!(
        again
            .notices
            .iter()
            .all(|notice| notice.outcome == Outcome::NotImported),
        "{:?}",
        again.notices
    );
    assert_eq!(again.not_imported(), 2, "what was left behind still is");
}

#[test]
fn an_encrypted_bitwarden_export_is_refused_with_the_way_out() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let sealed = r#"{"encrypted": true, "passwordProtected": true, "salt": "x",
                     "encKeyValidation_DO_NOT_EDIT": "2.x|y|z", "data": "2.a|b|c"}"#;
    match exchange::import(&mut vault, sealed) {
        Err(error @ Error::SealedImport { .. }) => {
            assert!(error.to_string().contains(".json"), "{error}")
        }
        other => panic!(
            "expected a refusal, got {:?}",
            other.map(|report| report.format)
        ),
    }
}

// CSV

#[test]
fn a_chrome_csv_comes_in_as_logins() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let csv = "name,url,username,password,note\n\
               mail,https://mail.example.invalid/,someone,\"pa,ss\"\"word\",\"two\nlines\"\n\
               ,https://www.shop.example.invalid/cart,ada,pw,\n";
    let report = exchange::import(&mut vault, csv).unwrap();
    assert_eq!(report.format, Format::Csv);
    assert_eq!(count(&report, ItemKind::Login), 2);

    let mail = payload_of(&vault, "mail");
    assert_eq!(
        field(&mail, "password"),
        Some(("pa,ss\"word".to_owned(), true))
    );
    assert_eq!(field(&mail, "notes").unwrap().0, "two\nlines");
    assert_eq!(
        field(&mail, "url").unwrap().0,
        "https://mail.example.invalid/"
    );
    // A row with no name is called after its site.
    assert!(vault.resolve("shop.example.invalid").is_ok());
}

#[test]
fn a_firefox_csv_keeps_its_identities_times_and_names_its_bookkeeping() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let csv = "\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\
               \"https://accounts.example.invalid\",\"ada\",\"pw\",,\"https://accounts.example.invalid\",\"{3f2b8c1e-9a4d-4e6f-8b2a-1c0d9e8f7a6b}\",\"1709164800000\",\"1709251200000\",\"1709200000000\"\n";
    let report = exchange::import(&mut vault, csv).unwrap();
    assert_eq!(
        report.columns_left_out,
        ["httpRealm", "formActionOrigin", "timeLastUsed"]
    );

    let summary = vault.resolve("accounts.example.invalid").unwrap();
    assert_eq!(summary.uuid, "3f2b8c1e-9a4d-4e6f-8b2a-1c0d9e8f7a6b");
    assert_eq!(summary.created_at, 1_709_164_800);
    assert_eq!(summary.updated_at, 1_709_200_000);
    let payload = payload_of(&vault, "accounts.example.invalid");
    assert!(payload.field("guid").is_none());
    assert!(payload.field("httpRealm").is_none());

    let again = exchange::import(&mut vault, csv).unwrap();
    assert_eq!(
        again.skipped, 1,
        "a guid makes a second import recognisable"
    );
}

#[test]
fn a_safari_csv_brings_its_one_time_password_keys() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let csv = "Title,URL,Username,Password,Notes,OTPAuth\n\
               mail,https://mail.example.invalid,someone,pw,,otpauth://totp/Mail:someone?secret=JBSWY3DPEHPK3PXP&issuer=Mail\n";
    exchange::import(&mut vault, csv).unwrap();
    assert_eq!(
        field(&payload_of(&vault, "mail"), "totp").unwrap().0,
        "otpauth://totp/Mail:someone?secret=JBSWY3DPEHPK3PXP&issuer=Mail"
    );
}

#[test]
fn a_password_managers_csv_brings_notes_folders_and_its_own_columns() {
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp,Security question\n\
               Personal,1,login,mail,,\"Recovery: abc\nPlan: family\",0,https://mail.example.invalid,someone,pw,,first pet\n\
               Home,,note,shed,combination 4815,,0,,,,,\n\
               ,,login,only a name,,,0,,,,,\n";
    let report = exchange::import(&mut vault, csv).unwrap();
    assert_eq!(count(&report, ItemKind::Login), 1);
    assert_eq!(count(&report, ItemKind::Note), 1);
    assert_eq!(notice(&report, "only a name").outcome, Outcome::NotImported);
    assert_eq!(report.columns_left_out, ["favorite", "reprompt"]);

    let mail = vault.resolve("mail").unwrap();
    assert_eq!(mail.tags, ["Personal"]);
    let mail = payload_of(&vault, "mail");
    assert_eq!(field(&mail, "Recovery"), Some(("abc".to_owned(), true)));
    assert_eq!(field(&mail, "Plan").unwrap().0, "family");
    // A column nobody named is kept, and kept hidden.
    assert_eq!(
        field(&mail, "Security question"),
        Some(("first pet".to_owned(), true))
    );
    assert_eq!(payload_of(&vault, "shed"), note("combination 4815"));
    assert_eq!(vault.resolve("shed").unwrap().tags, ["Home"]);
}

#[test]
fn a_keepassxc_csv_reads_like_its_xml() {
    // KeePassXC's group column starts with the root group's name, and an entry
    // with nothing but notes is a note: the same as its XML says.
    let fixture = fixture();
    let mut vault = empty_vault(&fixture);
    let csv = r#""Group","Title","Username","Password","URL","Notes","TOTP","Icon","Last Modified","Created"
"Root/Internet/Shops","shop","ada","pw","","","","0","2024-03-01T00:00:00Z","2024-01-01T00:00:00Z"
"Root","shed","","","","combination 4815","","0","2024-03-01T00:00:00Z","2024-01-01T00:00:00Z"
"#;
    let report = exchange::import(&mut vault, csv).unwrap();
    assert_eq!(report.columns_left_out, ["Icon"]);
    let shop = vault.resolve("shop").unwrap();
    assert_eq!(shop.tags, ["Internet/Shops"]);
    assert_eq!(shop.created_at, 1_704_067_200);
    assert_eq!(shop.updated_at, 1_709_251_200);
    assert!(vault.resolve("shed").unwrap().tags.is_empty());
    assert_eq!(payload_of(&vault, "shed"), note("combination 4815"));
}

#[test]
fn a_csv_export_writes_logins_and_counts_what_it_cannot() {
    let origin = fixture();
    let vault = every_kind(&origin);
    let exported = exchange::export(&vault, Target::Csv).unwrap();
    let report = &exported.report;
    assert_eq!(report.written, 2);
    assert_eq!(report.trimmed, 1, "the recovery field has no column");
    // In the order kinds are listed everywhere else, not the vault's.
    assert_eq!(
        report.left_out,
        [
            (ItemKind::Note, 1),
            (ItemKind::Card, 1),
            (ItemKind::Wifi, 1),
            (ItemKind::File, 1),
        ]
    );
    assert!(
        exported
            .text
            .starts_with("name,url,username,password,note,totp\n")
    );

    // What a browser reads back is what was written.
    let destination = fixture();
    let mut restored = empty_vault(&destination);
    exchange::import(&mut restored, &exported.text).unwrap();
    let mail = payload_of(&restored, "mail");
    let original = payload_of(&vault, "mail");
    for name in ["login", "password", "url", "totp", "notes"] {
        assert_eq!(field(&mail, name), field(&original, name), "{name}");
    }
    assert_eq!(
        field(&payload_of(&restored, "linked"), "totp"),
        field(&payload_of(&vault, "linked"), "totp")
    );
}
