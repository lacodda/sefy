//! Folding one vault into another.
//!
//! Two copies of a vault drift apart the moment they are edited on different
//! machines, and nothing in the file can warn about it: the format carries no
//! header, no timestamp and no counter in the clear, because any of those would
//! be the signature it exists to avoid. So drift is not detected — it is
//! resolved afterwards, here, with both passwords in hand.
//!
//! Matching is by [`ItemSummary::uuid`](crate::ItemSummary::uuid), the identity
//! an item keeps when it travels. Titles are not used: two accounts can share a
//! name, and renaming an item must not turn it into a different one.
//!
//! Contents are decided by their [versions](crate::history), not by clocks.
//! Each side knows which versions it has been through, so "the other copy is
//! simply behind" and "both copies moved on" can be told apart — and only the
//! second is a conflict.
//!
//! A [`preview`] is the same merge, run on a copy of the destination that
//! nothing ever writes. It is not a separate prediction of what the merge
//! would do, and so it cannot drift from it.

use crate::copies;
use crate::error::Result;
use crate::history::Version;
use crate::model::{NewItem, Payload};
use crate::vault::Vault;
use std::path::PathBuf;

/// What a merge did, and what it could not decide on its own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Items the destination did not have, copied across with their history.
    pub added: Vec<MergedItem>,
    /// Items whose contents, title or tags were taken from the other side
    /// because it had moved on and this one had not.
    pub updated: Vec<MergedItem>,
    /// Items already identical, or already ahead here, left alone.
    pub unchanged: usize,
    /// Items whose contents changed on both sides; the older of the two was
    /// kept in the item's history.
    pub conflicts: Vec<Conflict>,
    /// Earlier versions this vault did not have, brought across into the
    /// history of the items they belong to.
    pub versions: usize,
    /// Items of a kind this build does not know, left where they were.
    ///
    /// A newer sefy wrote them, and this one holds their identity but not their
    /// contents. Copying such an item across would create one that is empty
    /// rather than merely unread — so it stays put, and is counted here instead
    /// of being passed over in silence.
    pub unsupported: usize,
    /// Where the vault's file was kept as it was before the merge wrote it,
    /// when the merge changed anything.
    ///
    /// Always `None` for a [`preview`], which writes nothing and so has
    /// nothing to keep a copy before.
    pub kept_copy: Option<PathBuf>,
}

/// One item a merge brought across or changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedItem {
    /// The item, in the vault merged into. For an item a preview adds, the id
    /// it would get.
    pub id: i64,
    /// Title it carries there after the merge.
    pub title: String,
}

/// One item whose contents changed on both sides since the copies parted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The item, in this vault.
    pub id: i64,
    /// Title it carries here after the merge.
    pub title: String,
    /// Whose contents are current now; the other side's are in the item's
    /// history, marked as a conflict.
    pub current: Side,
}

/// One of the two vaults in a merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The vault being merged into.
    Here,
    /// The vault being merged from.
    There,
}

impl MergeReport {
    /// Whether the merge changed anything at all.
    ///
    /// Skipped items count as something to report even though nothing moved:
    /// "nothing to do" would be read as "the two copies agree", and they do
    /// not — this build simply could not tell. History that came across counts
    /// too: the vault is different for having it.
    pub fn is_empty(&self) -> bool {
        !self.changed() && self.unsupported == 0
    }

    /// Whether the merge changed the vault merged into.
    ///
    /// Unlike [`MergeReport::is_empty`], items left where they were do not
    /// count: reporting them is owed, but they moved nothing, and a vault
    /// nothing moved in needs neither a copy kept nor a write.
    pub fn changed(&self) -> bool {
        !self.added.is_empty()
            || !self.updated.is_empty()
            || !self.conflicts.is_empty()
            || self.versions > 0
    }
}

/// How the contents of one item compare between the two sides.
enum Contents {
    /// The same on both.
    Agree,
    /// The other side's are a version this side has already been past.
    Behind,
    /// This side's are a version the other side has already been past.
    Ahead,
    /// Each side has a version the other has never seen.
    Diverged,
}

/// Folds the contents of `source` into `destination`.
///
/// Item by item, matched on identity:
///
/// - not here yet → copied across with its identity, timestamps and history;
/// - here → the earlier versions it has there and not here are added to its
///   history, and then its contents are settled:
///   - the same, or the other side's are a version this side has been past →
///     left alone;
///   - this side's are a version the other side has been past → the other
///     side's become current, and these stay in history;
///   - both moved on → the side changed more recently becomes current (on a
///     tie, this one), and the other side's version is kept in history marked
///     as a conflict.
///
/// Nothing here throws a secret away: every contents an item leaves behind
/// become a version of it. Titles and tags are labels, where "newest wins" is
/// the reasonable rule, and it is the rule they get.
///
/// Items present here but absent there are never removed. A merge cannot tell
/// "deleted over there" from "added over here" — the two look identical from
/// this side — and deleting someone's secret on a guess is not a trade worth
/// making.
///
/// A merge that changes anything first [keeps a copy](crate::copies) of the
/// destination's file as it was, and writes nothing if that copy cannot be
/// made. A merge that changes nothing writes nothing at all.
pub fn merge(destination: &mut Vault, source: &Vault) -> Result<MergeReport> {
    let mut report = fold(destination, source)?;

    if report.changed() {
        report.kept_copy = copies::keep(destination.path())?;
        destination.save()?;
    }
    Ok(report)
}

/// What [`merge`] would do, with nothing written.
///
/// The merge runs for real, on a copy of the destination held in memory and
/// dropped afterwards: the report is the one the merge itself would give.
pub fn preview(destination: &Vault, source: &Vault) -> Result<MergeReport> {
    folded(destination, source).map(|(_, report)| report)
}

/// A copy of `destination` with `source` folded into it, and what that did.
///
/// For a preview that has to look past the merge: what a sync would send back
/// up is what the remote lacks of the merged result, and only the merged
/// result can say.
pub(crate) fn folded(destination: &Vault, source: &Vault) -> Result<(Vault, MergeReport)> {
    let mut copy = destination.scratch_copy()?;
    let report = fold(&mut copy, source)?;
    Ok((copy, report))
}

/// Folds `source` into `destination` in memory; the caller decides what is
/// written.
pub(crate) fn fold(destination: &mut Vault, source: &Vault) -> Result<MergeReport> {
    let mut report = MergeReport::default();

    for incoming_summary in source.list()? {
        let incoming = source.get(incoming_summary.id)?;
        let uuid = &incoming.summary.uuid;

        // Nothing here can be decided: this build cannot read the incoming
        // contents, so it can neither compare them with what is here nor carry
        // them over. Merging with a version that does know the kind will do it.
        if matches!(incoming.payload, Payload::Unknown { .. }) {
            report.unsupported += 1;
            continue;
        }

        let their_stamp = source.stamp(incoming_summary.id)?;
        let their_past = source.past_versions(incoming_summary.id)?;

        let Some(id) = destination.find_by_uuid(uuid)? else {
            let title = incoming.summary.title.clone();
            let (id, kept) = destination.add_travelled(
                NewItem {
                    title: incoming.summary.title,
                    payload: incoming.payload,
                    tags: incoming.summary.tags,
                },
                uuid,
                (incoming.summary.created_at, incoming.summary.updated_at),
                &their_stamp,
                &their_past,
            )?;
            report.added.push(MergedItem { id, title });
            report.versions += kept;
            continue;
        };

        let existing = destination.get(id)?;
        let my_stamp = destination.stamp(id)?;
        let my_past = destination.past_versions(id)?;

        // Settled from what each side had been through *before* anything
        // moved: the history brought across next would otherwise make this
        // side look as though it had seen what it only just received.
        let contents = if existing.payload == incoming.payload {
            Contents::Agree
        } else if been_through(&my_past, &their_stamp.uuid, &incoming.payload) {
            Contents::Behind
        } else if been_through(&their_past, &my_stamp.uuid, &existing.payload) {
            Contents::Ahead
        } else {
            Contents::Diverged
        };

        for version in &their_past {
            if destination.keep_version(id, version)? {
                report.versions += 1;
            }
        }

        // Strictly older, not "older or the same". Timestamps here are whole
        // seconds, so two machines editing the same item within one second —
        // ordinary once a sync runs after both — carry the same one. Treating
        // that as "the incoming copy is newer" would push the local edit out of
        // the current contents on a tie, which is the one thing the local side
        // never has to accept.
        let theirs_newer = existing.summary.updated_at < incoming.summary.updated_at;

        let mut moved = false;
        match contents {
            Contents::Agree | Contents::Behind => {}
            Contents::Ahead => {
                destination.take_contents(id, &incoming.payload, &their_stamp, false)?;
                moved = true;
            }
            Contents::Diverged => {
                if theirs_newer {
                    destination.take_contents(id, &incoming.payload, &their_stamp, true)?;
                } else {
                    destination.keep_version(
                        id,
                        &Version {
                            uuid: their_stamp.uuid.clone(),
                            seq: their_stamp.seq,
                            made_at: their_stamp.made_at,
                            device: their_stamp.device.clone(),
                            conflict: true,
                            current: false,
                            payload: incoming.payload.clone(),
                        },
                    )?;
                }
                moved = theirs_newer;
            }
        }

        let labels_differ = existing.summary.title != incoming.summary.title
            || existing.summary.tags != incoming.summary.tags;
        let take_labels = labels_differ && theirs_newer;
        let updated_at = existing.summary.updated_at.max(incoming.summary.updated_at);
        if take_labels {
            destination.set_labels(
                id,
                &incoming.summary.title,
                &incoming.summary.tags,
                updated_at,
            )?;
        } else if moved {
            destination.set_updated_at(id, updated_at)?;
        }

        let title = if take_labels {
            incoming.summary.title
        } else {
            existing.summary.title
        };
        if matches!(contents, Contents::Diverged) {
            report.conflicts.push(Conflict {
                id,
                title,
                current: if theirs_newer {
                    Side::There
                } else {
                    Side::Here
                },
            });
        } else if moved || take_labels {
            report.updated.push(MergedItem { id, title });
        } else {
            report.unchanged += 1;
        }
    }

    Ok(report)
}

/// Whether a side has been through the version named `uuid` holding `payload`.
///
/// Both have to match. The identity alone is not enough: an older build that
/// rewrote an item's contents left the identity as it was, and taking its edit
/// for a version already seen would drop it without a word.
fn been_through(past: &[Version], uuid: &str, payload: &Payload) -> bool {
    past.iter()
        .any(|version| version.uuid == uuid && &version.payload == payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Field, ItemKind};
    use std::path::PathBuf;

    const PASSWORD: &[u8] = b"master password";
    const SAME_MOMENT: i64 = 1_700_000_000;

    fn note(text: &str) -> Payload {
        Payload::Note {
            text: text.to_owned(),
        }
    }

    /// Two copies of one vault holding one note, as two machines have after a
    /// copy, each told its own machine's name.
    fn two_copies() -> (tempfile::TempDir, Vault, Vault, i64, i64) {
        let directory = tempfile::tempdir().unwrap();
        let here = directory.path().join("here.bak");
        let there: PathBuf = directory.path().join("there.bak");

        let mut origin = Vault::create(&here, PASSWORD).unwrap();
        origin.set_device(Some("desk".to_owned()));
        let id = origin.add(NewItem::new("bank", note("original"))).unwrap();
        origin.save().unwrap();
        std::fs::copy(&here, &there).unwrap();

        let mut mine = Vault::open(&here, PASSWORD).unwrap();
        mine.set_device(Some("desk".to_owned()));
        let mut theirs = Vault::open(&there, PASSWORD).unwrap();
        theirs.set_device(Some("laptop".to_owned()));
        let their_id = theirs
            .find_by_uuid(&mine.summary(id).unwrap().uuid)
            .unwrap()
            .unwrap();
        (directory, mine, theirs, id, their_id)
    }

    fn text(vault: &Vault, id: i64) -> String {
        match vault.get(id).unwrap().payload {
            Payload::Note { text } => text,
            other => panic!("expected a note, got {other:?}"),
        }
    }

    #[test]
    fn two_edits_in_the_same_second_are_a_conflict_not_a_silent_overwrite() {
        // Timestamps are whole seconds, so two machines editing one item within
        // the same second carry the same one — ordinary rather than exotic once
        // a sync runs shortly after both edits. A tie must not be read as "the
        // incoming copy is newer": the local edit stays current, and the other
        // is kept rather than lost.
        let (_directory, mut mine, mut theirs, id, their_id) = two_copies();
        theirs
            .update_at(
                their_id,
                None,
                Some(note("changed there")),
                None,
                SAME_MOMENT,
            )
            .unwrap();
        mine.update_at(id, None, Some(note("changed here")), None, SAME_MOMENT)
            .unwrap();

        let report = merge(&mut mine, &theirs).unwrap();

        assert!(report.updated.is_empty(), "a tie is not an update");
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].current, Side::Here);
        assert_eq!(text(&mine, id), "changed here", "the local edit survives");

        let history = mine.history(id).unwrap();
        let kept = history
            .iter()
            .find(|version| version.conflict)
            .expect("the incoming edit is in the history");
        assert_eq!(kept.payload, note("changed there"), "and so does the other");
        assert_eq!(
            kept.device.as_deref(),
            Some("laptop"),
            "marked with the machine it came from"
        );
    }

    #[test]
    fn a_conflict_won_by_the_other_side_keeps_this_side_in_history() {
        // The case the old merge got wrong: both sides changed, the incoming
        // one was newer, and the local edit was overwritten with nothing kept.
        let (_directory, mut mine, mut theirs, id, their_id) = two_copies();
        mine.update_at(id, None, Some(note("mine, older")), None, 10)
            .unwrap();
        theirs
            .update_at(their_id, None, Some(note("theirs, newer")), None, 20)
            .unwrap();

        let report = merge(&mut mine, &theirs).unwrap();

        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].current, Side::There);
        assert_eq!(text(&mine, id), "theirs, newer");
        let history = mine.history(id).unwrap();
        let kept = history.iter().find(|version| version.conflict).unwrap();
        assert_eq!(kept.payload, note("mine, older"));
        assert_eq!(kept.device.as_deref(), Some("desk"));
    }

    #[test]
    fn an_older_version_from_the_other_side_is_kept_in_history_not_beside() {
        let (_directory, mut mine, mut theirs, id, their_id) = two_copies();
        theirs
            .update_at(their_id, None, Some(note("theirs, older")), None, 10)
            .unwrap();
        mine.update(id, None, Some(note("mine, newer")), None)
            .unwrap();

        let report = merge(&mut mine, &theirs).unwrap();

        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(text(&mine, id), "mine, newer");
        assert_eq!(
            mine.list().unwrap().len(),
            1,
            "no second item: the losing version lives in the history"
        );
    }

    #[test]
    fn a_label_change_on_one_side_and_an_edit_on_the_other_are_no_conflict() {
        // Only contents make versions. A rename over there and a new password
        // here are two different things changing, and both are kept.
        let (_directory, mut mine, mut theirs, id, their_id) = two_copies();
        mine.update_at(id, None, Some(note("new text")), None, 10)
            .unwrap();
        theirs
            .update_at(their_id, Some("renamed".to_owned()), None, None, 20)
            .unwrap();

        let report = merge(&mut mine, &theirs).unwrap();

        assert!(report.conflicts.is_empty(), "{report:?}");
        assert_eq!(report.updated.len(), 1);
        assert_eq!(text(&mine, id), "new text");
        assert_eq!(mine.summary(id).unwrap().title, "renamed");
    }

    #[test]
    fn an_edit_by_an_older_build_is_not_taken_for_a_version_already_seen() {
        // An older build rewrites contents without naming a new version, so
        // the incoming side still carries the identity this side has already
        // been past. Matching on the identity alone would call the other side
        // behind and drop its edit.
        let (_directory, mut mine, theirs, id, their_id) = two_copies();
        mine.update_at(id, None, Some(note("mine")), None, 10)
            .unwrap();
        theirs
            .connection_for_tests()
            .execute(
                "UPDATE notes SET text = 'rewritten by an older build' WHERE item_id = ?1",
                [their_id],
            )
            .unwrap();
        theirs
            .connection_for_tests()
            .execute("UPDATE items SET updated_at = 20 WHERE id = ?1", [their_id])
            .unwrap();

        let report = merge(&mut mine, &theirs).unwrap();

        assert_eq!(report.conflicts.len(), 1, "{report:?}");
        assert_eq!(text(&mine, id), "rewritten by an older build");
        let history = mine.history(id).unwrap();
        assert!(
            history
                .iter()
                .any(|version| version.payload == note("mine")),
            "and this side's edit is kept"
        );

        // The rewrite came under the identity of the original, which is kept
        // here already; one identity must not name two different contents.
        let mut identities: Vec<&str> = history
            .iter()
            .map(|version| version.uuid.as_str())
            .collect();
        identities.sort_unstable();
        identities.dedup();
        assert_eq!(identities.len(), history.len(), "{history:?}");
    }

    #[test]
    fn a_record_that_diverged_keeps_both_passwords() {
        let directory = tempfile::tempdir().unwrap();
        let here = directory.path().join("here.bak");
        let there = directory.path().join("there.bak");
        let login = |password: &str| {
            Payload::fields(
                ItemKind::Login,
                [
                    Field::public("login", "ada"),
                    Field::secret("password", password),
                ],
            )
        };

        let mut mine = Vault::create(&here, PASSWORD).unwrap();
        let id = mine.add(NewItem::new("mail", login("first"))).unwrap();
        mine.save().unwrap();
        std::fs::copy(&here, &there).unwrap();
        let mut theirs = Vault::open(&there, PASSWORD).unwrap();

        mine.update_at(id, None, Some(login("from here")), None, 10)
            .unwrap();
        theirs
            .update_at(id, None, Some(login("from there")), None, 20)
            .unwrap();

        merge(&mut mine, &theirs).unwrap();

        let kept: Vec<Payload> = mine
            .history(id)
            .unwrap()
            .into_iter()
            .map(|version| version.payload)
            .collect();
        for password in ["first", "from here", "from there"] {
            assert!(kept.contains(&login(password)), "{password} was lost");
        }
    }

    /// Two copies that have drifted: one item added over there, the shared
    /// one changed on both sides, so every part of a report has something in
    /// it.
    fn drifted() -> (tempfile::TempDir, Vault, Vault, i64) {
        let (directory, mut mine, mut theirs, id, their_id) = two_copies();
        mine.update_at(id, None, Some(note("mine")), None, 10)
            .unwrap();
        mine.save().unwrap();
        theirs
            .update_at(their_id, None, Some(note("theirs")), None, 20)
            .unwrap();
        theirs
            .add(NewItem::new("wifi", note("from there")))
            .unwrap();
        (directory, mine, theirs, id)
    }

    fn without_the_copy(mut report: MergeReport) -> MergeReport {
        report.kept_copy = None;
        report
    }

    #[test]
    fn a_preview_is_the_merge_and_leaves_the_vault_as_it_was() {
        let (_directory, mine, theirs, id) = drifted();
        let before = std::fs::read(mine.path()).unwrap();

        let previewed = preview(&mine, &theirs).unwrap();

        // Nothing moved: not on disk, and not in the open vault either.
        assert_eq!(std::fs::read(mine.path()).unwrap(), before);
        assert_eq!(mine.writes(), 1, "only the save made by the fixture");
        assert_eq!(text(&mine, id), "mine");
        assert_eq!(mine.list().unwrap().len(), 1);
        assert!(
            copies::list(mine.path()).is_empty(),
            "a preview keeps no copy"
        );

        // And what it said is what the merge then does, item for item.
        let (_again, mut mine_again, theirs_again, _) = drifted();
        let merged = merge(&mut mine_again, &theirs_again).unwrap();
        assert_eq!(previewed.kept_copy, None);
        assert_eq!(previewed, without_the_copy(merged));
        assert_eq!(previewed.added.len(), 1, "{previewed:?}");
        assert_eq!(previewed.added[0].title, "wifi");
        assert_eq!(previewed.conflicts.len(), 1);
    }

    #[test]
    fn a_merge_that_changes_the_vault_keeps_it_as_it_was_first() {
        let (_directory, mut mine, theirs, _) = drifted();
        let before = std::fs::read(mine.path()).unwrap();

        let report = merge(&mut mine, &theirs).unwrap();

        let kept = report.kept_copy.expect("a copy was kept");
        assert_eq!(kept, copies::path(mine.path(), 1));
        assert_eq!(
            std::fs::read(&kept).unwrap(),
            before,
            "the copy is the file as it was before the merge wrote it"
        );
        assert_ne!(std::fs::read(mine.path()).unwrap(), before);
    }

    #[test]
    fn a_merge_that_changes_nothing_writes_nothing_and_keeps_no_copy() {
        // Otherwise every sync that found the two in step would push a useful
        // copy out of the rotation with one identical to the vault.
        let (_directory, mut mine, theirs, _, _) = two_copies();
        let before = std::fs::read(mine.path()).unwrap();

        let report = merge(&mut mine, &theirs).unwrap();

        assert!(!report.changed(), "{report:?}");
        assert_eq!(report.kept_copy, None);
        assert_eq!(mine.writes(), 0);
        assert_eq!(std::fs::read(mine.path()).unwrap(), before);
        assert!(copies::list(mine.path()).is_empty());
    }

    #[test]
    fn a_merge_that_cannot_keep_a_copy_does_not_write() {
        let (_directory, mut mine, theirs, _) = drifted();
        let before = std::fs::read(mine.path()).unwrap();
        // The copy is written through a temporary file beside it; a directory
        // in that place makes the write fail the way a full disk would.
        let mut blocker = copies::path(mine.path(), 1).into_os_string();
        blocker.push(".sefy-tmp");
        std::fs::create_dir(&blocker).unwrap();

        merge(&mut mine, &theirs).unwrap_err();

        assert_eq!(
            std::fs::read(mine.path()).unwrap(),
            before,
            "no copy, no write: the merge must not cost the only state there was"
        );
    }
}
