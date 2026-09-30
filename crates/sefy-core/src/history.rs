//! Earlier contents of an item, and what changed between two of them.
//!
//! A version is one state of an item's **contents** — a note's text, a
//! record's fields, a file's bytes. Titles and tags are labels rather than
//! values: renaming an item does not make a version, and bringing back an old
//! password does not bring back the old name with it. "Newest wins" is fine
//! for a label and ruinous for a secret, which is the whole reason this module
//! exists.
//!
//! Every version carries its own identity. Two copies of a vault that parted
//! at version 2 can each go on to write a version numbered 3 with different
//! contents, so neither the item's identity nor the number can tell versions
//! apart; the version's own uuid can, and that is what a merge matches on so
//! that history arriving twice is still kept once. The number, `seq`, orders a
//! line of edits the way they were made, whatever the clocks of the machines
//! that made them said.

use crate::error::{Error, Result};
use crate::exchange::ExportField;
use crate::model::{Field, ItemKind, Payload};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// One state an item's contents have been in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Identity of this state, the same in every vault that holds it.
    pub uuid: String,
    /// Place in the item's line of edits: one more than the version it
    /// replaced.
    pub seq: i64,
    /// When these contents were written, seconds since the Unix epoch.
    pub made_at: i64,
    /// Name of the machine they were written on, when it was known.
    pub device: Option<String>,
    /// Whether this version lost a conflict in a merge and was kept rather
    /// than dropped.
    pub conflict: bool,
    /// Whether these are the item's current contents.
    pub current: bool,
    /// The contents themselves.
    pub payload: Payload,
}

/// What became of one part of an item between two versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The field's name; `text` for a note, `filename` and `contents` for a
    /// file.
    pub name: String,
    /// Whether the value is a secret on either side, and so never shown.
    pub secret: bool,
    /// What happened to it.
    pub kind: ChangeKind,
}

/// How a part of an item differs between two versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Present in both, with the same value and secrecy.
    Same,
    /// Present in both, with a different value or secrecy.
    Changed,
    /// Present only in the older of the two.
    Removed,
    /// Present only in the newer of the two.
    Added,
}

/// Compares two states of an item's contents, part by part.
///
/// Parts come in the newer version's order, followed by whatever only the
/// older one had. Secrecy is reported rather than acted on: this says *that* a
/// password changed, and leaves it to the caller never to print what it was.
pub fn compare(old: &Payload, new: &Payload) -> Vec<Change> {
    let before = parts(old);
    let after = parts(new);

    let mut changes: Vec<Change> = after
        .iter()
        .map(|part| {
            let kind = match before.iter().find(|earlier| earlier.name == part.name) {
                Some(earlier) if earlier.value == part.value && earlier.secret == part.secret => {
                    ChangeKind::Same
                }
                Some(_) => ChangeKind::Changed,
                None => ChangeKind::Added,
            };
            let was_secret = before
                .iter()
                .any(|earlier| earlier.name == part.name && earlier.secret);
            Change {
                name: part.name.to_owned(),
                secret: part.secret || was_secret,
                kind,
            }
        })
        .collect();

    changes.extend(
        before
            .iter()
            .filter(|earlier| !after.iter().any(|part| part.name == earlier.name))
            .map(|earlier| Change {
                name: earlier.name.to_owned(),
                secret: earlier.secret,
                kind: ChangeKind::Removed,
            }),
    );
    changes
}

/// Names of the parts that differ between two versions, in [`compare`] order.
pub fn changed(old: &Payload, new: &Payload) -> Vec<String> {
    compare(old, new)
        .into_iter()
        .filter(|change| change.kind != ChangeKind::Same)
        .map(|change| change.name)
        .collect()
}

/// The text of one part, for the parts that are text.
///
/// `text` of a note, a field by name, the `filename` of a file. A file's
/// `contents` are bytes and have no text to give.
pub fn text_of<'a>(payload: &'a Payload, name: &str) -> Option<&'a str> {
    match payload {
        Payload::Note { text } if name == NOTE_TEXT => Some(text),
        Payload::Fields { .. } => payload.field(name).map(|field| field.value.as_str()),
        Payload::File { filename, .. } if name == FILE_NAME => Some(filename),
        _ => None,
    }
}

/// What a note's body is called when it is compared.
const NOTE_TEXT: &str = "text";
/// What a file's name is called when it is compared.
const FILE_NAME: &str = "filename";
/// What a file's bytes are called when they are compared.
const FILE_CONTENTS: &str = "contents";

/// One comparable part of an item's contents.
struct Part<'a> {
    name: &'a str,
    value: &'a [u8],
    secret: bool,
}

fn parts(payload: &Payload) -> Vec<Part<'_>> {
    match payload {
        Payload::Note { text } => vec![Part {
            name: NOTE_TEXT,
            value: text.as_bytes(),
            secret: false,
        }],
        Payload::Fields { fields, .. } => fields
            .iter()
            .map(|field| Part {
                name: &field.name,
                value: field.value.as_bytes(),
                secret: field.secret,
            })
            .collect(),
        // Bytes are marked secret so that nobody is tempted to show them as
        // lines: a file is not text, whatever it happens to contain.
        Payload::File { filename, bytes } => vec![
            Part {
                name: FILE_NAME,
                value: filename.as_bytes(),
                secret: false,
            },
            Part {
                name: FILE_CONTENTS,
                value: bytes,
                secret: true,
            },
        ],
        // A kind this build cannot read has no parts it could name.
        Payload::Unknown { .. } => Vec::new(),
    }
}

/// One line of a comparison between two texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line<'a> {
    /// In both, unchanged.
    Same(&'a str),
    /// Only in the older text.
    Removed(&'a str),
    /// Only in the newer text.
    Added(&'a str),
}

/// Beyond this many cells, the comparison stops looking for the shortest
/// answer and says "all of this went, all of that came".
///
/// The table below costs four bytes a cell. Notes worth keeping in a secret
/// store stay far under this; one that does not still gets a correct answer,
/// only a longer one.
const MOST_CELLS: usize = 1_000_000;

/// Compares two texts line by line.
///
/// The longest common subsequence of lines, found over what is left once the
/// shared beginning and end are set aside — which for an edit of a few lines
/// in a long note is almost nothing. Written here rather than taken from a
/// crate: it is thirty lines, and every dependency is something a secret
/// store's users have to trust.
pub fn lines<'a>(old: &'a str, new: &'a str) -> Vec<Line<'a>> {
    let before: Vec<&str> = old.lines().collect();
    let after: Vec<&str> = new.lines().collect();

    let head = before
        .iter()
        .zip(&after)
        .take_while(|(a, b)| a == b)
        .count();
    let tail = before[head..]
        .iter()
        .rev()
        .zip(after[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let a = &before[head..before.len() - tail];
    let b = &after[head..after.len() - tail];

    let mut out: Vec<Line<'a>> = before[..head].iter().map(|line| Line::Same(line)).collect();

    if a.len().saturating_mul(b.len()) > MOST_CELLS {
        out.extend(a.iter().map(|line| Line::Removed(line)));
        out.extend(b.iter().map(|line| Line::Added(line)));
    } else {
        // common[i][j]: how many lines a[i..] and b[j..] share in order.
        let width = b.len() + 1;
        let mut common = vec![0u32; (a.len() + 1) * width];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                common[i * width + j] = if a[i] == b[j] {
                    common[(i + 1) * width + j + 1] + 1
                } else {
                    common[(i + 1) * width + j].max(common[i * width + j + 1])
                };
            }
        }

        let (mut i, mut j) = (0, 0);
        while i < a.len() && j < b.len() {
            if a[i] == b[j] {
                out.push(Line::Same(a[i]));
                i += 1;
                j += 1;
            } else if common[(i + 1) * width + j] >= common[i * width + j + 1] {
                out.push(Line::Removed(a[i]));
                i += 1;
            } else {
                out.push(Line::Added(b[j]));
                j += 1;
            }
        }
        out.extend(a[i..].iter().map(|line| Line::Removed(line)));
        out.extend(b[j..].iter().map(|line| Line::Added(line)));
    }

    out.extend(
        before[before.len() - tail..]
            .iter()
            .map(|line| Line::Same(line)),
    );
    out
}

/// A version's contents as they are kept in the `versions` table.
///
/// JSON rather than a row per field: an earlier version is only ever read
/// whole, and a frozen copy has no business being queried — or searched,
/// which is where an old password would otherwise turn up.
#[derive(Serialize, Deserialize)]
struct Stored {
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fields: Vec<ExportField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bytes_base64: Option<String>,
}

/// Renders contents for the `versions` table.
pub(crate) fn encode(payload: &Payload) -> Result<Zeroizing<String>> {
    let mut stored = Stored {
        kind: payload.kind().as_str().to_owned(),
        text: None,
        fields: Vec::new(),
        filename: None,
        bytes_base64: None,
    };
    match payload {
        Payload::Note { text } => stored.text = Some(text.clone()),
        Payload::Fields { fields, .. } => {
            stored.fields = fields
                .iter()
                .map(|field| ExportField {
                    name: field.name.clone(),
                    value: field.value.clone(),
                    secret: field.secret,
                })
                .collect();
        }
        Payload::File { filename, bytes } => {
            stored.filename = Some(filename.clone());
            stored.bytes_base64 = Some(BASE64.encode(bytes));
        }
        // Nothing to keep: this build holds the name of the kind and none of
        // the contents, and a version that claimed otherwise would be empty.
        Payload::Unknown { kind } => {
            return Err(Error::UnknownItemKind {
                id: 0,
                kind: kind.clone(),
            });
        }
    }
    let json = serde_json::to_string(&stored).map_err(|_| Error::UnreadableVersion)?;
    Ok(Zeroizing::new(json))
}

/// Reads contents back out of the `versions` table.
///
/// A kind this build does not know comes back as [`Payload::Unknown`], the
/// same as a current item of that kind: a newer sefy wrote it, and the version
/// is kept rather than refused.
pub(crate) fn decode(json: &str) -> Result<Payload> {
    let stored: Stored = serde_json::from_str(json).map_err(|_| Error::UnreadableVersion)?;
    let kind = ItemKind::parse(&stored.kind);
    Ok(match kind {
        ItemKind::Note => Payload::Note {
            text: stored.text.ok_or(Error::UnreadableVersion)?,
        },
        ItemKind::File => Payload::File {
            filename: stored.filename.ok_or(Error::UnreadableVersion)?,
            bytes: BASE64
                .decode(stored.bytes_base64.ok_or(Error::UnreadableVersion)?)
                .map_err(|_| Error::UnreadableVersion)?,
        },
        ItemKind::Unknown(name) => Payload::Unknown { kind: name },
        kind => Payload::Fields {
            kind,
            fields: stored
                .fields
                .into_iter()
                .map(|field| Field {
                    name: field.name,
                    value: field.value,
                    secret: field.secret,
                })
                .collect(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn every_kind_of_contents_survives_being_kept() {
        let payloads = [
            Payload::Note {
                text: "line one\nline two".to_owned(),
            },
            login("hunter2", "https://example.com"),
            Payload::File {
                filename: "id_ed25519".to_owned(),
                bytes: vec![0, 1, 2, 255],
            },
        ];
        for payload in payloads {
            let json = encode(&payload).unwrap();
            assert_eq!(decode(&json).unwrap(), payload);
        }
    }

    #[test]
    fn a_kind_from_a_newer_sefy_is_kept_as_unknown_rather_than_refused() {
        let payload = decode(r#"{"kind":"passport","number":"X"}"#).unwrap();
        assert_eq!(
            payload,
            Payload::Unknown {
                kind: "passport".to_owned()
            }
        );
    }

    #[test]
    fn contents_that_cannot_be_read_are_not_mistaken_for_empty_ones() {
        assert!(matches!(
            decode(r#"{"kind":"note"}"#),
            Err(Error::UnreadableVersion)
        ));
        assert!(matches!(decode("not json"), Err(Error::UnreadableVersion)));
    }

    #[test]
    fn a_comparison_names_what_changed_and_what_did_not() {
        let changes = compare(
            &login("old", "https://example.com"),
            &login("new", "https://example.com"),
        );
        let summary: Vec<(&str, ChangeKind, bool)> = changes
            .iter()
            .map(|change| (change.name.as_str(), change.kind, change.secret))
            .collect();
        assert_eq!(
            summary,
            [
                ("login", ChangeKind::Same, false),
                ("password", ChangeKind::Changed, true),
                ("url", ChangeKind::Same, false),
            ]
        );
    }

    #[test]
    fn fields_that_came_and_went_are_named_as_such() {
        let old = Payload::fields(
            ItemKind::Login,
            [Field::public("login", "ada"), Field::public("notes", "n")],
        );
        let new = Payload::fields(
            ItemKind::Login,
            [Field::public("login", "ada"), Field::secret("pin", "1234")],
        );
        let changes = compare(&old, &new);
        assert_eq!(changes[1].name, "pin");
        assert_eq!(changes[1].kind, ChangeKind::Added);
        assert!(changes[1].secret);
        assert_eq!(changes[2].name, "notes");
        assert_eq!(changes[2].kind, ChangeKind::Removed);
        assert_eq!(changed(&old, &new), ["pin", "notes"]);
    }

    #[test]
    fn a_field_that_was_secret_stays_secret_in_a_comparison() {
        // Made public later, the old value is still a secret: showing it as a
        // line diff would print the very thing the record used to hide.
        let old = Payload::fields(ItemKind::Login, [Field::secret("password", "s3cret")]);
        let new = Payload::fields(ItemKind::Login, [Field::public("password", "s3cret")]);
        let change = &compare(&old, &new)[0];
        assert_eq!(
            change.kind,
            ChangeKind::Changed,
            "secrecy is part of a field"
        );
        assert!(change.secret);
    }

    #[test]
    fn a_file_compares_its_name_and_its_bytes_but_offers_no_bytes_as_text() {
        let old = Payload::File {
            filename: "a.txt".to_owned(),
            bytes: b"one".to_vec(),
        };
        let new = Payload::File {
            filename: "a.txt".to_owned(),
            bytes: b"two".to_vec(),
        };
        let changes = compare(&old, &new);
        assert_eq!(changes[0].kind, ChangeKind::Same);
        assert_eq!(changes[1].name, "contents");
        assert_eq!(changes[1].kind, ChangeKind::Changed);
        assert!(changes[1].secret);
        assert_eq!(text_of(&new, "contents"), None);
        assert_eq!(text_of(&new, "filename"), Some("a.txt"));
    }

    #[test]
    fn a_line_diff_keeps_what_is_shared_and_marks_the_rest() {
        let diff = lines("a\nb\nc\nd", "a\nB\nc\nd\ne");
        assert_eq!(
            diff,
            [
                Line::Same("a"),
                Line::Removed("b"),
                Line::Added("B"),
                Line::Same("c"),
                Line::Same("d"),
                Line::Added("e"),
            ]
        );
    }

    #[test]
    fn a_line_diff_finds_the_longest_shared_run_not_the_first_one() {
        // A greedy walk would pair the first "x" and lose the three lines
        // after it; the table sees that keeping them is the shorter answer.
        let diff = lines("x\na\nb\nc", "a\nb\nc\nx");
        let same = diff
            .iter()
            .filter(|line| matches!(line, Line::Same(_)))
            .count();
        assert_eq!(same, 3, "{diff:?}");
    }

    #[test]
    fn a_line_diff_of_identical_or_empty_texts_is_all_or_nothing() {
        assert!(
            lines("a\nb", "a\nb")
                .iter()
                .all(|line| matches!(line, Line::Same(_)))
        );
        assert_eq!(lines("", "new"), [Line::Added("new")]);
        assert_eq!(lines("old", ""), [Line::Removed("old")]);
    }

    #[test]
    fn a_line_diff_too_large_to_table_is_still_correct() {
        let old: String = (0..1500).map(|n| format!("old {n}\n")).collect();
        let new: String = (0..1500).map(|n| format!("new {n}\n")).collect();
        let diff = lines(&old, &new);
        let removed = diff
            .iter()
            .filter(|line| matches!(line, Line::Removed(_)))
            .count();
        let added = diff
            .iter()
            .filter(|line| matches!(line, Line::Added(_)))
            .count();
        assert_eq!((removed, added), (1500, 1500));
    }
}
