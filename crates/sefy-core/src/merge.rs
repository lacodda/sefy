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

use crate::error::Result;
use crate::model::{NewItem, Payload};
use crate::vault::Vault;

/// What a merge did, and what it could not decide on its own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Items the destination did not have, copied across.
    pub added: usize,
    /// Items whose incoming copy was newer, updated in place.
    pub updated: usize,
    /// Items already identical or already newer here, left alone.
    pub unchanged: usize,
    /// Items where both sides changed and one version had to be kept aside.
    pub conflicts: Vec<Conflict>,
}

/// One item that changed on both sides since the copies parted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Title the item carries here.
    pub title: String,
    /// Title the losing copy was kept under.
    pub kept_as: String,
}

impl MergeReport {
    /// Whether the merge changed anything at all.
    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.updated == 0 && self.conflicts.is_empty()
    }
}

/// Folds the contents of `source` into `destination`.
///
/// Item by item, matched on identity:
///
/// - not here yet → copied across, keeping its identity and timestamps;
/// - here and identical → left alone;
/// - here and the incoming copy is newer → this one is updated;
/// - here, changed on both sides → this one wins, and the incoming version is
///   kept as a separate item rather than dropped.
///
/// That last case is the one that matters. "Newest wins" is a reasonable rule
/// for a title or a tag and a terrible one for a password: the older copy may
/// be the one that still opens the account. Nothing here throws a secret away.
///
/// Items present here but absent there are never removed. A merge cannot tell
/// "deleted over there" from "added over here" — the two look identical from
/// this side — and deleting someone's secret on a guess is not a trade worth
/// making.
pub fn merge(destination: &mut Vault, source: &Vault) -> Result<MergeReport> {
    let mut report = MergeReport::default();

    for incoming_summary in source.list()? {
        let incoming = source.get(incoming_summary.id)?;
        let uuid = &incoming.summary.uuid;

        let Some(existing_id) = destination.find_by_uuid(uuid)? else {
            destination.add_existing(
                NewItem {
                    title: incoming.summary.title,
                    payload: incoming.payload,
                    tags: incoming.summary.tags,
                },
                uuid,
                incoming.summary.created_at,
                incoming.summary.updated_at,
            )?;
            report.added += 1;
            continue;
        };

        let existing = destination.get(existing_id)?;
        if same_contents(&existing.payload, &incoming.payload)
            && existing.summary.title == incoming.summary.title
            && existing.summary.tags == incoming.summary.tags
        {
            report.unchanged += 1;
            continue;
        }

        // Both carry the identity, and their contents differ. Whether that is a
        // conflict depends on whether this side moved on since the incoming
        // copy was last written.
        //
        // Strictly older, not "older or the same". Timestamps here are whole
        // seconds, so two machines editing the same item within one second —
        // ordinary once a sync runs after both — carry the same one. Treating
        // that as "the incoming copy is newer" would discard the local edit on
        // a tie, which is the one thing this function promises not to do.
        if existing.summary.updated_at < incoming.summary.updated_at {
            destination.update(
                existing_id,
                Some(incoming.summary.title),
                Some(incoming.payload),
                Some(incoming.summary.tags),
            )?;
            report.updated += 1;
        } else {
            let kept_as = conflict_title(&incoming.summary.title);
            destination.add_existing(
                NewItem {
                    title: kept_as.clone(),
                    payload: incoming.payload,
                    tags: incoming.summary.tags,
                },
                &crate::db::new_uuid()?,
                incoming.summary.created_at,
                incoming.summary.updated_at,
            )?;
            report.conflicts.push(Conflict {
                title: existing.summary.title,
                kept_as,
            });
        }
    }

    destination.save()?;
    Ok(report)
}

/// Title the losing side of a conflict is kept under.
fn conflict_title(title: &str) -> String {
    format!("{title} (conflicted copy)")
}

/// Whether two payloads hold the same thing.
///
/// Kinds cannot change over an item's life, so payloads of different kinds
/// under one identity mean the vaults disagree about what the item is; that
/// counts as different, and the timestamps decide as usual.
fn same_contents(left: &Payload, right: &Payload) -> bool {
    match (left, right) {
        (Payload::Note { text: a }, Payload::Note { text: b }) => a == b,
        (Payload::Credential(a), Payload::Credential(b)) => a == b,
        (
            Payload::File {
                filename: a,
                bytes: left,
            },
            Payload::File {
                filename: b,
                bytes: right,
            },
        ) => a == b && left == right,
        _ => false,
    }
}
