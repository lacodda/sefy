---
title: "history"
description: List the earlier versions of an item's contents, or compare one with what it holds now.
---

Every change to an item's contents keeps what it replaced. `history` lists
those versions, and compares any one of them with what the item holds now.

## Usage

```sh
sefy history <REFERENCE> [VERSION]
```

| Argument | Meaning |
| --- | --- |
| `<REFERENCE>` | The item: an id, an exact title, or text to search for. |
| `[VERSION]` | A version, by the number the listing gives it, to compare with the current contents. |

## The listing

Oldest first, the current contents last. Each line says when the version was
written, on which machine, and which parts of the item it changed:

```console
$ sefy history mail
"mail" (login), 4 versions

  1  2026-08-02 09:14 UTC  desk    created
  2  2026-09-18 21:40 UTC  laptop  password, url
  3  2026-09-20 08:01 UTC  desk    notes  (lost a merge conflict)
  4  2026-09-20 08:03 UTC  laptop  notes  (current)

compare one with now: sefy history 2 VERSION
bring one back:       sefy restore 2 VERSION [--field NAME]
```

The machine is the one the version was written on, as it names itself — the
first part of its host name. A version kept from before sefy 0.12.0 has no
machine, and the earliest one of an item that was already edited then reads
`earliest kept` rather than `created`.

A version marked **lost a merge conflict** is one a
[`merge`](/sefy/reference/merge/), [`pull`](/sefy/reference/pull/) or
[`sync`](/sefy/reference/sync/) had to choose against: both machines changed
the item, the other one's change was more recent, and this one was kept here
instead of being dropped.

## Comparing a version with now

```console
$ sefy history mail 2
version 2 of "mail", written 2026-09-18 21:40 UTC on laptop, against what it holds now (version 4):

login     same
password  changed (secret, not shown)
url       same
notes     only now
  + recovery codes in the drawer

bring it back: sefy restore 2 2 [--field NAME]
```

Text is compared line by line: `-` for what the version had, `+` for what is
there now, and long unchanged stretches of a note folded into a count. A field
the version had and the record no longer has reads `only in version 2`; one
added since reads `only now`.

**A secret is only ever said to differ.** Neither the listing nor a comparison
prints a secret value — not the current one, and not an old one. To take an
old password back, [`restore`](/sefy/reference/restore/) it; `get` then hands
it over the usual way.

## What makes a version

A version is one state of an item's **contents**: a note's text, a record's
fields, a file's bytes. What makes one:

- [`edit`](/sefy/reference/edit/) of a note's text or a record's fields;
- [`otp --set`](/sefy/reference/otp/), which replaces a one-time password key;
- [`restore`](/sefy/reference/restore/) itself - so a restore can be undone;
- a merge that takes the other side's contents, or has to choose between two.

A new **title or tags** make no version. They are labels rather than values:
"newest wins" is the right rule for a name, and a history of renames would bury
the one change that matters. Saving contents identical to what is there makes
no version either.

## How long it is kept

For as long as the item exists. There is no limit and no pruning: an old
password is kept because it is the one thing that cannot be looked up again.

[`rm`](/sefy/reference/rm/) removes an item's history with it. That is also the
one way to be rid of an old value — and like any removal, a merge brings the
item back from a copy that still has it.

The history lives inside the sealed file, like everything else. See
[Threat model](/sefy/concepts/threat-model/) for what that means for a value
you changed because it leaked.

## Related

- [`restore`](/sefy/reference/restore/) — bringing a version back
- [`edit`](/sefy/reference/edit/) — the change that makes most versions
- [`merge`](/sefy/reference/merge/) — where conflicts end up here
- [`show`](/sefy/reference/show/) — says when an item has a history
