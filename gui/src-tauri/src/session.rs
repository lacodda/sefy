//! The one vault the window has open, and how it gets there.
//!
//! The vault lives here, in this process, for as long as it is unlocked; the
//! page is told what it is (`VaultStatus`), never handed the database. Locking
//! drops it, which wipes the password the core holds and lets the in-memory
//! database go.

use crate::wire::{
    Failure, FailureKind, Filter, ItemCard, ItemRow, Opening, TagCount, VaultStatus,
};
use sefy_core::{ItemKind, Payload, Query, Totp, Vault};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use zeroize::Zeroizing;

/// The vault the window has open, if any.
#[derive(Default)]
pub struct Session {
    vault: Mutex<Option<Vault>>,
}

impl Session {
    /// Puts an opened vault in the window, in place of whatever was open.
    pub fn hold(&self, vault: Vault) -> Result<VaultStatus, Failure> {
        let status = VaultStatus::of(&vault.status()?);
        *self.slot() = Some(vault);
        Ok(status)
    }

    /// Closes the open vault, if there is one.
    pub fn lock(&self) {
        // Taken out and dropped here rather than left to the next unlock, so
        // the password and the database are gone the moment the window says
        // the vault is locked.
        drop(self.slot().take());
    }

    /// What and where the open vault is.
    pub fn status(&self) -> Result<VaultStatus, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        Ok(VaultStatus::of(&vault.status()?))
    }

    /// Whether a vault is open.
    pub fn is_open(&self) -> bool {
        self.slot().is_some()
    }

    /// The items `filter` lets through, best match first.
    ///
    /// The core finds them (`Vault::search`, the same search as `sefy find`)
    /// and lists them newest first. With text typed, the window then puts the
    /// items whose *title* matches ahead of those that match only inside -
    /// see [`rank`] - keeping newest first within each.
    pub fn items(&self, filter: &Filter) -> Result<Vec<ItemRow>, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        let mut query = Query::all();
        let text = filter
            .text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        if let Some(text) = text {
            query = query.text(text);
        }
        if let Some(kind) = &filter.kind {
            query = query.kind(ItemKind::parse(kind));
        }
        if let Some(tag) = &filter.tag {
            query = query.tags([tag.as_str()]);
        }
        let mut found = vault.search(&query)?;
        if let Some(text) = text {
            found.sort_by_key(|item| rank(&item.title, text));
        }
        Ok(found.iter().map(ItemRow::of).collect())
    }

    /// Every tag in use, with how many items carry it.
    pub fn tags(&self) -> Result<Vec<TagCount>, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        Ok(vault
            .tags()?
            .into_iter()
            .map(|(tag, count)| TagCount {
                tag,
                count: usize::try_from(count).unwrap_or(0),
            })
            .collect())
    }

    /// The item with `id`, as the card shows it, as of `now` (Unix seconds).
    pub fn card(&self, id: i64, now: i64) -> Result<ItemCard, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        let item = vault.get(id)?;
        Ok(ItemCard::of(&item, vault.earlier_versions(id)?, now))
    }

    /// One field of the item with `id`, and what it is called, as of `now`
    /// (Unix seconds): what Copy puts on the clipboard and Reveal shows.
    ///
    /// `None` takes what the item is mostly about, as `sefy get` does: a
    /// note's text, the first secret field of a record. A one-time password
    /// key gives the code it makes now, never the key - the key is set up
    /// once and the code is what is wanted every time after, which is also
    /// what `sefy fill` hands over.
    pub fn value(
        &self,
        id: i64,
        field: Option<&str>,
        now: u64,
    ) -> Result<(Zeroizing<String>, String), Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        let item = vault.get(id)?;
        let title = &item.summary.title;
        match &item.payload {
            Payload::Note { text } => match field {
                None => Ok((Zeroizing::new(text.clone()), "text".to_owned())),
                Some(_) => Err(refused(format!("{title:?} is a note; it has no fields"))),
            },
            Payload::Fields { kind, fields } => {
                let name = match field {
                    Some(name) => name,
                    None => kind
                        .template()
                        .and_then(|template| template.default_field())
                        .map(|field| field.name)
                        .ok_or_else(|| {
                            refused(format!("{title:?} has no field that is the secret"))
                        })?,
                };
                let stored = fields
                    .iter()
                    .find(|field| field.name == name)
                    .ok_or_else(|| refused(format!("{title:?} has no {name:?}")))?;
                if stored.name == sefy_core::otp::FIELD {
                    let totp = Totp::parse(&stored.value).map_err(|_| {
                        refused(format!(
                            "the one-time password key of {title:?} cannot make a code"
                        ))
                    })?;
                    return Ok((Zeroizing::new(totp.code_at(now)), "code".to_owned()));
                }
                Ok((Zeroizing::new(stored.value.clone()), stored.name.clone()))
            }
            Payload::File { .. } => Err(refused(format!(
                "{title:?} is a file; files leave the vault with sefy extract"
            ))),
            Payload::Unknown { kind } => Err(refused(format!(
                "{title:?} is a {kind}, which a newer sefy wrote; update sefy to open it"
            ))),
        }
    }

    /// The slot, even after a panic elsewhere held the lock: what is in it is
    /// a vault or nothing, and neither is left half-changed by a panic.
    fn slot(&self) -> MutexGuard<'_, Option<Vault>> {
        self.vault.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// How well `title` matches what was typed: the lower, the better.
///
/// The core's search matches text anywhere - in the title, in a note, in a
/// public field - and that is right for finding things. It is not the order
/// a person expects once they have typed `mail`: the item called `mailbox`
/// first, and the note that mentions mail somewhere after it. So: the whole
/// title, then a title that starts with it, then a word in the title that
/// does, then anywhere in the title, then everything the core found inside.
pub fn rank(title: &str, typed: &str) -> u8 {
    let title = title.to_lowercase();
    let typed = typed.trim().to_lowercase();
    if title == typed {
        0
    } else if title.starts_with(&typed) {
        1
    } else if title
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(&typed))
    {
        2
    } else if title.contains(&typed) {
        3
    } else {
        4
    }
}

fn refused(message: String) -> Failure {
    Failure {
        kind: FailureKind::Other,
        message,
    }
}

/// Opens the vault at `path` — the slow part of unlocking, which stretches
/// the password on purpose and so belongs off the window's thread.
pub fn open(path: &Path, password: &[u8]) -> Result<Vault, Failure> {
    Vault::open(path, password).map_err(Failure::from)
}

/// The file the window was started on: the first argument that is not a flag,
/// as "Open with" and a terminal pass it, or else `SEFY_VAULT`, the one the
/// command line uses.
///
/// Nothing else, and nothing remembered: sefy keeps no list of the vaults it
/// has opened. A list like that, in a predictable place under the user's
/// profile, would name exactly the files the format spends all its effort
/// making unremarkable — to anyone who looks there, and to any scanner that
/// knows where sefy keeps its things.
pub fn opening(args: impl IntoIterator<Item = OsString>, vault_env: Option<OsString>) -> Opening {
    let argument = args
        .into_iter()
        .skip(1)
        .find(|arg| !arg.to_string_lossy().starts_with('-'));
    let file = argument
        .or(vault_env)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        // A path relative to wherever the launcher stood would mean another
        // file once the dialog moves elsewhere.
        .map(|path| std::path::absolute(&path).unwrap_or(path))
        .map(|path| path.display().to_string());
    Opening { file }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWORD: &[u8] = b"correct horse battery staple";

    fn vault_in(directory: &tempfile::TempDir) -> PathBuf {
        let path = directory.path().join("notes.bak");
        let mut vault = Vault::create(&path, PASSWORD).unwrap();
        vault
            .add(sefy_core::NewItem::new(
                "bank",
                sefy_core::Payload::Note {
                    text: "1234".into(),
                },
            ))
            .unwrap();
        vault.save().unwrap();
        path
    }

    #[test]
    fn an_unlocked_vault_answers_until_it_is_locked() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();

        let status = session.hold(open(&path, PASSWORD).unwrap()).unwrap();
        assert_eq!(status.name, "notes.bak");
        assert_eq!(status.items, 1);
        assert!(session.is_open());
        assert_eq!(session.status().unwrap(), status);

        session.lock();
        assert!(!session.is_open());
        assert_eq!(session.status().unwrap_err().kind, FailureKind::Locked);
    }

    #[test]
    fn a_wrong_password_is_refused_and_leaves_nothing_open() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();

        let failure = open(&path, b"not it").unwrap_err();
        assert_eq!(failure.kind, FailureKind::Refused);
        assert!(!session.is_open());
    }

    #[test]
    fn any_other_file_is_refused_the_same_way() {
        // A file that is not a vault and a vault under another password are
        // one answer: there is nothing in either to tell them apart.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("holiday.jpg");
        std::fs::write(&path, vec![7u8; 4096]).unwrap();
        assert_eq!(
            open(&path, PASSWORD).unwrap_err().kind,
            FailureKind::Refused
        );

        let tiny = directory.path().join("tiny");
        std::fs::write(&tiny, b"x").unwrap();
        assert_eq!(
            open(&tiny, PASSWORD).unwrap_err().kind,
            FailureKind::Refused
        );
    }

    #[test]
    fn a_file_that_is_not_there_is_missing_rather_than_refused() {
        let directory = tempfile::tempdir().unwrap();
        let failure = open(&directory.path().join("gone.bak"), PASSWORD).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Missing);
    }

    #[test]
    fn unlocking_another_vault_closes_the_first() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let session = Session::default();

        session
            .hold(open(&vault_in(&first), PASSWORD).unwrap())
            .unwrap();
        let other = second.path().join("archive.dat");
        Vault::create(&other, PASSWORD).unwrap();
        session.hold(open(&other, PASSWORD).unwrap()).unwrap();

        let status = session.status().unwrap();
        assert_eq!(status.name, "archive.dat");
        assert_eq!(status.items, 0);
    }

    /// A vault with a login carrying a one-time password key, a note, a
    /// file and a card, open in a session.
    fn browsing(directory: &tempfile::TempDir) -> Session {
        use sefy_core::{Field, NewItem, Payload};
        let path = directory.path().join("notes.bak");
        let mut vault = Vault::create(&path, PASSWORD).unwrap();
        vault
            .add(
                NewItem::new(
                    "mailbox",
                    Payload::fields(
                        ItemKind::Login,
                        [
                            Field::public("login", "kmiller-dev"),
                            Field::secret("password", "hunter2-not-real"),
                            Field::secret("totp", "JBSWY3DPEHPK3PXP"),
                        ],
                    ),
                )
                .with_tags(["work"]),
            )
            .unwrap();
        vault
            .add(NewItem::new(
                "recovery codes",
                Payload::Note {
                    text: "mailbox: F7Q2-XXXX".into(),
                },
            ))
            .unwrap();
        vault
            .add(NewItem::new(
                "scan.pdf",
                Payload::File {
                    filename: "scan.pdf".into(),
                    bytes: vec![1, 2, 3],
                },
            ))
            .unwrap();
        vault
            .add(NewItem::new(
                "travel card",
                Payload::fields(
                    ItemKind::Card,
                    [
                        Field::secret("number", "4111111111111111"),
                        Field::public("expires", "2030-01"),
                    ],
                ),
            ))
            .unwrap();
        vault.save().unwrap();
        let session = Session::default();
        session.hold(open(&path, PASSWORD).unwrap()).unwrap();
        session
    }

    fn titles(rows: &[ItemRow]) -> Vec<&str> {
        rows.iter().map(|row| row.title.as_str()).collect()
    }

    fn id_of(session: &Session, title: &str) -> i64 {
        session
            .items(&Filter::default())
            .unwrap()
            .into_iter()
            .find(|row| row.title == title)
            .unwrap()
            .id
    }

    #[test]
    fn a_title_that_matches_comes_before_a_match_inside() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let filter = Filter {
            text: Some("mailbox".into()),
            ..Filter::default()
        };
        // The note says mailbox in its text and is newer; the login is called
        // mailbox and comes first anyway.
        assert_eq!(
            titles(&session.items(&filter).unwrap()),
            ["mailbox", "recovery codes"]
        );
    }

    #[test]
    fn the_rail_narrows_by_kind_and_by_tag() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let notes = Filter {
            kind: Some("note".into()),
            ..Filter::default()
        };
        assert_eq!(titles(&session.items(&notes).unwrap()), ["recovery codes"]);
        let work = Filter {
            tag: Some("work".into()),
            ..Filter::default()
        };
        assert_eq!(titles(&session.items(&work).unwrap()), ["mailbox"]);
    }

    #[test]
    fn a_secret_is_never_what_the_list_is_searched_by() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let filter = Filter {
            text: Some("hunter2".into()),
            ..Filter::default()
        };
        assert!(session.items(&filter).unwrap().is_empty());
    }

    #[test]
    fn the_card_names_every_secret_and_carries_none() {
        use crate::wire::{CardBody, CardField};
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let card = session.card(id_of(&session, "mailbox"), 0).unwrap();

        let wire = serde_json::to_string(&card).unwrap();
        for secret in ["hunter2-not-real", "JBSWY3DPEHPK3PXP"] {
            assert!(!wire.contains(secret), "the card carries {secret}: {wire}");
        }
        let CardBody::Record { fields } = card.body else {
            panic!("a login is a record");
        };
        assert_eq!(
            fields,
            [
                CardField::Plain {
                    name: "login".into(),
                    value: "kmiller-dev".into(),
                    days_left: None,
                },
                CardField::Secret {
                    name: "password".into()
                },
                CardField::Code {
                    name: "totp".into(),
                    period: 30
                },
            ]
        );
    }

    #[test]
    fn the_date_a_card_stops_working_is_counted_down() {
        use crate::wire::{CardBody, CardField};
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        // 2029-12-31 00:00 UTC: the card runs to the end of January 2030.
        let card = session
            .card(id_of(&session, "travel card"), 1_893_369_600)
            .unwrap();
        let CardBody::Record { fields, .. } = card.body else {
            panic!("a card is a record");
        };
        let Some(CardField::Plain { days_left, .. }) = fields
            .iter()
            .find(|field| matches!(field, CardField::Plain { name, .. } if name == "expires"))
        else {
            panic!("the card shows when it stops working");
        };
        assert_eq!(*days_left, Some(31));
    }

    #[test]
    fn a_copy_takes_what_sefy_get_would() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);

        let (value, what) = session.value(id_of(&session, "mailbox"), None, 0).unwrap();
        assert_eq!(
            (value.as_str(), what.as_str()),
            ("hunter2-not-real", "password")
        );

        let (value, what) = session
            .value(id_of(&session, "recovery codes"), None, 0)
            .unwrap();
        assert_eq!(
            (value.as_str(), what.as_str()),
            ("mailbox: F7Q2-XXXX", "text")
        );

        let (value, what) = session
            .value(id_of(&session, "mailbox"), Some("login"), 0)
            .unwrap();
        assert_eq!((value.as_str(), what.as_str()), ("kmiller-dev", "login"));
    }

    #[test]
    fn a_one_time_password_gives_its_code_and_never_its_key() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let id = id_of(&session, "mailbox");
        let (first, what) = session.value(id, Some("totp"), 59).unwrap();
        assert_eq!(what, "code");
        assert_eq!(first.len(), 6);
        assert!(first.chars().all(|c| c.is_ascii_digit()));
        // Another period, another code.
        let (later, _) = session.value(id, Some("totp"), 1_111_111_109).unwrap();
        assert_ne!(first, later);
    }

    #[test]
    fn a_file_is_not_copied() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let failure = session
            .value(id_of(&session, "scan.pdf"), None, 0)
            .unwrap_err();
        assert!(
            failure.message.contains("sefy extract"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn a_locked_session_shows_and_copies_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let id = id_of(&session, "mailbox");
        session.lock();
        assert_eq!(
            session.items(&Filter::default()).unwrap_err().kind,
            FailureKind::Locked
        );
        assert_eq!(session.card(id, 0).unwrap_err().kind, FailureKind::Locked);
        assert_eq!(
            session.value(id, None, 0).unwrap_err().kind,
            FailureKind::Locked
        );
    }

    #[test]
    fn ranking_goes_whole_title_then_start_then_word_then_anywhere() {
        assert_eq!(rank("Mailbox", "mailbox"), 0);
        assert_eq!(rank("mailbox archive", "mail"), 1);
        assert_eq!(rank("work · mailroom", "mail"), 2);
        assert_eq!(rank("email", "mail"), 3);
        assert_eq!(rank("recovery codes", "mail"), 4);
    }

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_file_named_at_launch_comes_before_the_environment() {
        let opening = opening(
            args(&["sefy-gui", "--some-flag", "notes.bak"]),
            Some("elsewhere.dat".into()),
        );
        let file = opening.file.unwrap();
        assert!(file.ends_with("notes.bak"), "{file}");
        assert!(
            Path::new(&file).is_absolute(),
            "{file} would mean another file once the dialog moves"
        );
    }

    #[test]
    fn without_an_argument_the_window_opens_on_sefy_vault() {
        let opening = opening(args(&["sefy-gui"]), Some("elsewhere.dat".into()));
        assert!(opening.file.unwrap().ends_with("elsewhere.dat"));
    }

    #[test]
    fn with_neither_the_person_picks_the_file() {
        assert_eq!(opening(args(&["sefy-gui"]), None).file, None);
        assert_eq!(opening(args(&["sefy-gui"]), Some("".into())).file, None);
    }
}
