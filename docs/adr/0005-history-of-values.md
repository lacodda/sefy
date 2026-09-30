# ADR-0005: History of values, and a merge that decides by versions

- **Status:** accepted
- **Date:** 2026-09-29

## Context

Up to 0.11, an edit overwrote. A password changed in the vault that the site
never accepted was gone; so was a one-time password key replaced before the
site confirmed the new one. And a merge that found an item changed on both
sides kept the incoming version as a second item, `… (conflicted copy)`, which
grew the list and left the choosing to the user — while the case where the
*incoming* side was newer overwrote the local edit with nothing kept at all.

0.12.0 keeps what an edit replaces, and makes a merge settle a conflict into
that history instead of beside it.

## Decision

### A version is a state of the contents, not of the item

A version is what an item holds: a note's text, a record's fields, a file's
bytes. Title and tags are labels. A rename makes no version, and restoring an
old password does not bring an old name back. "Newest wins" is the right rule
for a label and the wrong one for a value; keeping labels out of versions is
what lets each get its rule. Saving contents identical to the current ones
makes no version either.

### Every version has its own identity, and a number

A version carries a random UUID of its own and a `seq`: one more than the
version it replaced.

The item's UUID with the number was the obvious key and does not work. Two
copies that parted at version 2 each go on to write a version 3 with different
contents; the pair `(item, 3)` would then name two different things, and a
merge keyed on it would keep one and drop the other. The version's own UUID is
what a merge matches on, so history arriving on every sync is kept once. `seq`
orders a line of edits the way they were made, whatever the clocks of the
machines that made them said.

A version also records when it was written and on which machine — the host
name up to its first dot, passed in by the program around the library
(`Vault::set_device`) rather than guessed by it.

### The current contents stay where they are

Current contents keep living in `notes`, `fields` and `files`. `items` gains
four nullable columns naming the version they are (`version`, `version_seq`,
`version_at`, `version_device`); earlier versions are rows in a new `versions`
table, each holding its contents whole as JSON, with a foreign key to its item
and `ON DELETE CASCADE`.

Rejected: keeping the current version in `versions` too. That is a second copy
of every secret, and an older build — which knows nothing of the table — would
leave it stale at its first edit. Rejected: a row per field of every version.
An earlier version is only ever read whole, and a frozen copy has no business
being queried, or matched by search.

New contents are put in place first and the replaced ones kept after. In the
other order the replaced contents are still current when they are kept, and
cannot be told apart from the current version they are.

### A merge decides by versions, not by clocks

For an item both sides hold, what each side has been through settles it:

- the other side's contents are a version this side has kept → it is behind,
  nothing moves;
- this side's contents are a version the other side has kept → this side is
  behind, the other side's become current, these are kept;
- neither → both moved on: a conflict.

A version counts as "been through" only if **both** its identity and its
contents match. An older build rewrites contents without naming a new version,
and identity alone would take its edit for a version already seen and drop it.

In a conflict the side changed more recently becomes current — on a tie, this
one, since timestamps are whole seconds and a tie is ordinary — and the other
is kept as a version marked `conflict`. The other side's earlier versions come
across first, so after one sync both machines hold the same history, and a
conflict settled on one machine arrives on the other as history it is simply
behind.

### Contents from before history get a derived identity

An item with no version — from a vault written before 0.12.0, or inserted by
an older build since — has its contents named on the next open. The UUID is
derived from the item's UUID (SHA-256 into a version-8 UUID), not drawn at
random: two copies of one old vault, migrated separately on two machines, must
name the same contents the same way, or the first edit after upgrading both
reads as a conflict on the other. Copies that had already drifted end up with
one name over two contents; a version is matched on identity *and* contents,
and one kept under a name already taken by other contents gets a fresh one.

### Neither the listing nor a comparison prints a secret

`sefy history` lists versions with what each changed, by field name.
`sefy history ITEM N` compares a version with now: text line by line, a secret
only as "changed". A line diff is written in the core (the longest common
subsequence over what is left once a shared beginning and end are set aside,
with a cap past which it reports "all of this went, all of that came") rather
than taken from a crate; it is thirty lines, and the GUI will want it from the
same place. `sefy restore ITEM N [--field NAME]` is an edit like any other, so
it is itself undone by a restore.

## Consequences

- Schema 5; the file format stays at version 1.
- An older build (0.11.x) keeps working on a vault this build has touched: its
  edits keep nothing, its removals take the history with them through the
  cascade, and its merges still make `(conflicted copy)` items.
- History has no limit and is kept as long as the item is. `rm` is the one way
  to be rid of an old value, and like any removal it does not propagate
  through a merge.
- An export carries current contents only; the vault file is the complete
  copy.
- `MergeReport` changes shape: `Conflict` names the item and which side is
  current instead of a title it was copied under, and `versions` counts the
  history brought across.
