//! What in a vault deserves a second look: passwords that are weak, reused or
//! unchanged for years, secrets kept twice, and records past or near the day
//! they stop working.
//!
//! Everything is worked out on this machine from what the vault already
//! holds. Nothing is looked up anywhere — not a breach list, not a hash
//! prefix — and nothing leaves: an audit that phoned out would be the first
//! thing about sefy to put a vault's contents, in any form, on a network.
//!
//! A report names places, never values. It says *that* two items share a
//! password and which ones; it does not say what the password is, nor what
//! made zxcvbn think little of it, since "a date is easy to guess" is already
//! a fact about what the password contains.

use crate::error::Result;
use crate::expiry::{self, Expiry, Stated};
use crate::history::Version;
use crate::model::{AuditAs, Field, ItemKind, Payload};
use crate::strength;
use crate::vault::Vault;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use zeroize::Zeroize;

/// A password scoring below this on zxcvbn's scale of 0 to 4 is weak.
///
/// 3 is "safely unguessable: moderate protection from an offline attack". A
/// 2 survives a site that limits sign-in attempts and little else — and a
/// password stored in a password manager has no reason to be one a person
/// could remember.
pub const WEAK_BELOW: u8 = 3;

/// A password unchanged for longer than this many days is old.
///
/// A year. Changing passwords on a schedule is not advice anyone gives any
/// more; this is about the password from before the last breach that nobody
/// noticed, and a year is long enough that it says something.
pub const OLD_AFTER_DAYS: i64 = 365;

/// Where something was found: an item, and the field in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The item's id in this vault.
    pub id: i64,
    /// What the item is called.
    pub title: String,
    /// What kind of item it is.
    pub kind: ItemKind,
    /// The field the finding is about.
    pub field: String,
}

/// A password zxcvbn thinks little of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weak {
    /// Where it is.
    pub place: Place,
    /// zxcvbn's score, below [`WEAK_BELOW`].
    pub score: u8,
}

/// A password that has been what it is for more than [`OLD_AFTER_DAYS`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Old {
    /// Where it is.
    pub place: Place,
    /// When it took its current value, seconds since the Unix epoch.
    pub since: i64,
}

/// A record past, or near, the last day it works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    /// The record, and the field its date is in.
    pub place: Place,
    /// The date.
    pub expiry: Expiry,
}

/// A date sefy cannot count from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// The record, and the field the value is in.
    pub place: Place,
    /// What the field holds. A date is public, so it is quoted.
    pub value: String,
}

/// What an audit found, with what it looked at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Items in the vault.
    pub items: usize,
    /// Passwords looked at.
    pub passwords: usize,
    /// Records that state a date they stop working.
    pub dated: usize,
    /// Items of a kind this build does not know, which it could not look
    /// into.
    pub unknown: usize,
    /// Passwords that are easy to guess, weakest first.
    pub weak: Vec<Weak>,
    /// Passwords shared by more than one item: one group per password.
    pub reused: Vec<Vec<Place>>,
    /// Issued secrets — a card number, a token, a key — held by more than one
    /// item: one group per secret.
    pub duplicated: Vec<Vec<Place>>,
    /// Passwords unchanged for over [`OLD_AFTER_DAYS`], oldest first.
    pub old: Vec<Old>,
    /// Records past their last day, longest gone first.
    pub expired: Vec<Due>,
    /// Records with no more than [`expiry::SOON_DAYS`] left, soonest first.
    pub expiring: Vec<Due>,
    /// Dates that are not dates.
    pub unreadable: Vec<Unreadable>,
}

impl Report {
    /// How many things the report asks to be looked at: one per weak or old
    /// password, one per shared secret however many items share it, one per
    /// record that is due or whose date cannot be read.
    pub fn findings(&self) -> usize {
        self.weak.len()
            + self.reused.len()
            + self.duplicated.len()
            + self.old.len()
            + self.expired.len()
            + self.expiring.len()
            + self.unreadable.len()
    }
}

/// Audits a vault as it stands at `now`, seconds since the Unix epoch.
///
/// Reads every item and, for the items holding a password, their history;
/// changes nothing.
pub fn audit(vault: &Vault, now: i64) -> Result<Report> {
    let mut report = Report::default();
    let mut passwords: BTreeMap<[u8; 32], Vec<Place>> = BTreeMap::new();
    let mut issued: BTreeMap<[u8; 32], Vec<Place>> = BTreeMap::new();

    let mut summaries = vault.list()?;
    summaries.sort_by_key(|summary| summary.id);
    report.items = summaries.len();

    for summary in summaries {
        let mut item = vault.get(summary.id)?;
        let place = |field: &str| Place {
            id: summary.id,
            title: summary.title.clone(),
            kind: summary.kind.clone(),
            field: field.to_owned(),
        };

        let fields = match &item.payload {
            Payload::Fields { fields, .. } => fields,
            Payload::Unknown { .. } => {
                report.unknown += 1;
                continue;
            }
            Payload::Note { .. } | Payload::File { .. } => continue,
        };

        let mut history: Option<Vec<Version>> = None;
        for field in fields.iter().filter(|field| !field.value.is_empty()) {
            match AuditAs::of(&summary.kind, field) {
                AuditAs::Password => {
                    report.passwords += 1;
                    passwords
                        .entry(digest("password", &field.value))
                        .or_default()
                        .push(place(&field.name));

                    let context = context_of(&summary.title, fields);
                    let context: Vec<&str> = context.iter().map(String::as_str).collect();
                    let score = strength::estimate(&field.value, &context).score;
                    if score < WEAK_BELOW {
                        report.weak.push(Weak {
                            place: place(&field.name),
                            score,
                        });
                    }

                    if history.is_none() {
                        history = Some(vault.history(summary.id)?);
                    }
                    let versions = history.as_deref().expect("read just above");
                    let since = unchanged_since(versions, field);
                    if now - since > OLD_AFTER_DAYS * 86_400 {
                        report.old.push(Old {
                            place: place(&field.name),
                            since,
                        });
                    }
                }
                AuditAs::Unique => issued
                    .entry(digest(&field.name, &field.value))
                    .or_default()
                    .push(place(&field.name)),
                AuditAs::Nothing => {}
            }
        }

        match expiry::of(&item.payload) {
            Some(Stated::Date(date)) => {
                report.dated += 1;
                let due = Due {
                    place: place(expiry::FIELD),
                    expiry: date,
                };
                match date.days_left(now) {
                    days if days < 0 => report.expired.push(due),
                    days if days <= expiry::SOON_DAYS => report.expiring.push(due),
                    _ => {}
                }
            }
            Some(Stated::Unreadable(value)) => report.unreadable.push(Unreadable {
                place: place(expiry::FIELD),
                value: value.to_owned(),
            }),
            None => {}
        }
        // A card whose date from before 0.15.0 could not be read as one keeps
        // it under the old name; it is still a date nobody can count from.
        if summary.kind == ItemKind::Card
            && let Some(field) = item.payload.field(expiry::LEGACY_CARD_NAME)
            && item.payload.field(expiry::FIELD).is_none()
        {
            report.unreadable.push(Unreadable {
                place: place(&field.name),
                value: field.value.clone(),
            });
        }

        item.payload.zeroize();
        for version in history.iter_mut().flatten() {
            version.payload.zeroize();
        }
    }

    report.reused = shared(passwords);
    report.duplicated = shared(issued);
    report.weak.sort_by_key(|weak| (weak.score, weak.place.id));
    report.old.sort_by_key(|old| (old.since, old.place.id));
    report
        .expired
        .sort_by_key(|due| (due.expiry.last_day(), due.place.id));
    report
        .expiring
        .sort_by_key(|due| (due.expiry.last_day(), due.place.id));
    Ok(report)
}

/// A value's fingerprint, so that values can be compared without being kept.
///
/// `class` keeps classes apart: a token and a card number that happen to be
/// the same digits are not one secret kept twice.
fn digest(class: &str, value: &str) -> [u8; 32] {
    Sha256::new()
        .chain_update(class.as_bytes())
        .chain_update([0])
        .chain_update(value.as_bytes())
        .finalize()
        .into()
}

/// The groups of places that share a value across more than one item, in the
/// order of the first item in each.
fn shared(groups: BTreeMap<[u8; 32], Vec<Place>>) -> Vec<Vec<Place>> {
    let mut shared: Vec<Vec<Place>> = groups
        .into_values()
        .filter(|places| {
            let mut ids: Vec<i64> = places.iter().map(|place| place.id).collect();
            ids.dedup();
            ids.len() > 1
        })
        .collect();
    shared.sort_by_key(|places| places[0].id);
    shared
}

/// The words a password should not be built from: the record's title and its
/// public fields, split where an address or a name would be — so that
/// `mail.example.com` offers `example` on its own.
fn context_of(title: &str, fields: &[Field]) -> Vec<String> {
    let mut words = Vec::new();
    let values = std::iter::once(title).chain(
        fields
            .iter()
            .filter(|field| !field.secret && field.name != "notes")
            .map(|field| field.value.as_str()),
    );
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        words.push(value.to_owned());
        words.extend(
            value
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| word.chars().count() >= 3)
                .map(str::to_owned),
        );
    }
    words
}

/// When a field took the value it has now.
///
/// Walks the item's line of edits back from the current contents for as long
/// as the field held the same value, and answers with the moment the earliest
/// of those was written. Contents that lost a merge are not part of the line:
/// they were never what the item said.
fn unchanged_since(versions: &[Version], field: &Field) -> i64 {
    let current = versions
        .last()
        .expect("history ends with the current version");
    let mut since = current.made_at;
    let mut line: Vec<&Version> = versions
        .iter()
        .filter(|version| !version.current && !version.conflict && version.seq < current.seq)
        .collect();
    line.sort_by_key(|version| (version.seq, version.made_at));
    for version in line.into_iter().rev() {
        match version.payload.field(&field.name) {
            Some(then) if then.value == field.value => since = version.made_at,
            _ => break,
        }
    }
    since
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(seq: i64, made_at: i64, password: &str, conflict: bool) -> Version {
        Version {
            uuid: format!("v{seq}-{conflict}"),
            seq,
            made_at,
            device: None,
            conflict,
            current: false,
            payload: Payload::fields(
                ItemKind::Login,
                [
                    Field::public("login", "ada"),
                    Field::secret("password", password),
                ],
            ),
        }
    }

    fn current(mut version: Version) -> Version {
        version.current = true;
        version
    }

    #[test]
    fn a_password_is_as_old_as_the_first_version_that_had_it() {
        let password = Field::secret("password", "same");
        let versions = [
            version(1, 100, "older", false),
            version(2, 200, "same", false),
            version(3, 300, "same", false),
            current(version(4, 400, "same", false)),
        ];
        assert_eq!(unchanged_since(&versions, &password), 200);
    }

    #[test]
    fn a_password_brought_back_is_as_old_as_its_return() {
        let password = Field::secret("password", "first");
        let versions = [
            version(1, 100, "first", false),
            version(2, 200, "second", false),
            current(version(3, 300, "first", false)),
        ];
        assert_eq!(unchanged_since(&versions, &password), 300);
    }

    #[test]
    fn contents_that_lost_a_merge_do_not_interrupt_the_line() {
        let password = Field::secret("password", "same");
        let versions = [
            version(1, 100, "same", false),
            version(2, 150, "theirs", true),
            current(version(3, 200, "same", false)),
        ];
        assert_eq!(unchanged_since(&versions, &password), 100);
    }

    #[test]
    fn a_password_with_no_history_is_as_old_as_its_contents() {
        let password = Field::secret("password", "only");
        let versions = [current(version(1, 500, "only", false))];
        assert_eq!(unchanged_since(&versions, &password), 500);
    }

    #[test]
    fn a_value_is_shared_only_across_items() {
        let place = |id: i64| Place {
            id,
            title: format!("item {id}"),
            kind: ItemKind::Login,
            field: "password".to_owned(),
        };
        let mut groups = BTreeMap::new();
        groups.insert([1; 32], vec![place(3), place(3)]);
        groups.insert([2; 32], vec![place(5), place(7)]);
        groups.insert([3; 32], vec![place(1), place(9)]);
        groups.insert([4; 32], vec![place(2)]);
        let shared = shared(groups);
        let ids: Vec<Vec<i64>> = shared
            .iter()
            .map(|places| places.iter().map(|place| place.id).collect())
            .collect();
        assert_eq!(ids, [vec![1, 9], vec![5, 7]]);
    }

    #[test]
    fn a_class_keeps_the_same_digits_apart() {
        assert_ne!(digest("number", "4111"), digest("account", "4111"));
        assert_eq!(digest("number", "4111"), digest("number", "4111"));
        // The separator keeps a class from running into its value.
        assert_ne!(digest("ab", "c"), digest("a", "bc"));
    }

    #[test]
    fn the_context_offers_the_words_inside_an_address() {
        let fields = [
            Field::public("login", "ada@quillfeather.net"),
            Field::secret("password", "hidden"),
            Field::public("url", "https://mail.quillfeather.net/login"),
            Field::public("notes", "nothing from here"),
        ];
        let words = context_of("Quill mail", &fields);
        assert!(words.contains(&"quillfeather".to_owned()), "{words:?}");
        assert!(words.contains(&"Quill mail".to_owned()));
        assert!(!words.iter().any(|word| word.contains("hidden")));
        assert!(!words.iter().any(|word| word.contains("nothing")));
    }
}
