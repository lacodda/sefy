---
title: Versions and compatibility
description: What happens when two machines run different versions of sefy, and what is promised across them.
---

A vault is one file, and files travel. Once you
[sync](/sefy/guides/syncing/) between two machines, or restore a backup onto a
new one, sooner or later two different versions of sefy are looking at the same
vault. This page says what holds across that gap.

## The file format does not move

The vault format is **version 1**, and it is frozen. A vault created by 0.1.0
opens in the current release, and that is checked on every build against a real
file written by an older published binary rather than against the current code's
opinion of itself.

Changing the format — different Argon2 parameters, a different layout — would
mean a new format version, and it would be announced as a breaking change with a
migration path. Nothing does that quietly.

The database *inside* the encrypted blob is a different promise, and it does
move: 0.7.0 took it from version 2 to 3, replacing the `credentials` table with
a `fields` table so a record can hold named fields instead of one fixed set,
and 0.8.0 took it to 4, adding a small table for facts about the vault itself —
the first being when it last reached a remote, which
[`status`](/sefy/reference/status/) reports. A vault written by an earlier
release is migrated on open, exactly as the uuid migration was — the file
format stays v1 throughout, so the migration happens in memory and the vault
remains readable by the same rule as ever.

The 3 → 4 move takes nothing away, so a vault it has touched stays usable by
0.7.1: that build does not know the new table, writes into the ones it does
know, and leaves the rest as it found it. Checked with the published binary in
both directions, the way every schema move here is.

0.12.0 took it to 5, adding [history](/sefy/reference/history/): a table of
earlier versions, and a note on each item of which version its contents are.
The same rule holds — a vault it has touched stays usable by 0.11.x — with
three things worth knowing about that older build:

- **its edits keep nothing.** 0.11.x rewrites contents in place, as it always
  did; the value it replaced is gone. The history already kept stays as it was,
  and the next edit made by 0.12.0 or later is kept again.
- **its removals take the history with them**, the same as a removal here. The
  cascade is part of the table, not of the code, so no old value of a removed
  item is left behind in a table the older build does not know.
- **its merges make copies.** A conflict merged by 0.11.x still ends as a second
  item called `… (conflicted copy)`, which is how that build settles one. Merge
  from the newer side to have it end in the history instead.

You can see which version a vault carries:

```console
$ sefy status
schema   5
```

## Items from a newer sefy

Inside the encrypted file, the contents can grow: a newer sefy may add a kind of
item this one has never heard of — a 0.6.0 build meeting a `card` written by
0.7.0, say. When an older build meets one, it:

- **lists it**, under the name the kind was stored as, marked so you can tell
  why it looks different;
- **finds it** by title and tags;
- lets you **retitle and retag** it, because those live beside the contents;
- **exports it**, flagged as an entry whose contents this build could not read;
- **leaves it alone** during a merge, and says that it did.

What it will not do is guess. Reading such an item, editing its contents, or
importing one into a vault are all refused with an explanation, because an item
that silently arrived empty would be worse than one that was honestly not read.

```console
$ sefy ls
1  shed     note
2  my visa  card        (needs a newer sefy)

$ sefy get "my visa"
error: "my visa" is a card, which this version of sefy does not know
it was written by a newer sefy — upgrade to read it
(the item is safe: it is listed, exported and synced as it is)
```

Nothing is lost in the meantime: the item and its contents stay in the vault
untouched, and upgrading makes them readable again.

:::caution[Before 0.6.0]
sefy 0.5.0 and earlier did not do this. A single item of an unknown kind made
`ls`, `find`, `export` and `merge` fail outright, reporting that an item plainly
present in the file did not exist. If you sync between machines, upgrade the
older one before a newer sefy writes a kind it does not know.
:::

## Syncing between different versions

A [transport](/sefy/reference/plugin/) moves the sealed file and never looks
inside it, so mixed versions are a question about the contents, not the
transport. The rule that matters: **the machine with the older sefy can carry a
vault it cannot fully read** — it will not corrupt or drop what it does not
understand — but it cannot merge those items into another vault. Merge from the
newer side, or upgrade.

## What a plugin can rely on

The [plugin protocol](/sefy/reference/plugin/) is at version 1, and a plugin
written against 0.3.0 keeps working. New manifest fields are added as optional
ones; changing what an existing field means would require protocol version 2,
with both accepted for a time.
