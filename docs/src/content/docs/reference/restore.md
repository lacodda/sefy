---
title: "restore"
description: Bring back an earlier version of an item's contents, or one field of it.
---

Brings back the contents an item had at an earlier version — whole, or one
field of a record.

## Usage

```sh
sefy restore <REFERENCE> <VERSION> [--field NAME]
```

| Argument / option | Meaning |
| --- | --- |
| `<REFERENCE>` | The item: an id, an exact title, or text to search for. |
| `<VERSION>` | The version to bring back, by the number [`history`](/sefy/reference/history/) gives it. |
| `--field <NAME>` | Bring back only this field of a record; the others keep their current values. |

```console
$ sefy restore mail 2
restored "mail" from version 2 (written 2026-09-18 21:40 UTC on laptop)
what it replaced is kept as version 4; sefy history 2 lists them
```

## One field

The usual case is a password changed in the vault that the site never accepted:
the old one still opens the account, and the rest of the record — a new URL, a
note added since — should stay as it is.

```console
$ sefy restore mail 2 --field password
restored the password of "mail" from version 2 (written 2026-09-18 21:40 UTC on laptop)
what it replaced is kept as version 4; sefy history 2 lists them
$ sefy get mail
copied password of "mail" to the clipboard; clearing in 45s
```

The field comes back with the secrecy it had. A field the record no longer has
is added back at the end; one the version never had is refused, with what it did
have:

```console
$ sefy restore mail 1 --field pin
error: version 1 of "mail" has no "pin"; it held: login, password, url
```

A note or a file is one value rather than a set of fields, and is restored
whole.

## A restore can be undone

Restoring is an edit like any other. The contents it replaces become a version
of their own, so going back is one more `restore` — nothing is lost by trying
the wrong one.

Contents that already match the version are left alone rather than recorded
again:

```console
$ sefy restore mail 4
version 4 is what "mail" holds now; nothing to restore
```

## What it does not touch

The **title and tags** stay as they are now: they are not part of a version.
Restoring a password from before a rename does not bring the old name back.

## Related

- [`history`](/sefy/reference/history/) — the versions, and how each differs from now
- [`get`](/sefy/reference/get/) — taking the restored value
- [`edit`](/sefy/reference/edit/) — changing contents by hand
