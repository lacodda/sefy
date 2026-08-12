---
title: "merge"
description: Fold another vault file into this one, item by item.
---

Folds another vault into this one — for two copies that drifted apart on
different machines.

## Usage

```sh
sefy merge <FILE> [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--other-password-env <VAR>` | Read the other vault's password from this variable. |

```console
$ sefy merge ~/from-laptop.bak
Password for /home/you/from-laptop.bak:
merged: 1 added, 1 updated, 1 unchanged
```

The other vault's password is asked for separately, because a copy from another
machine may well be under a different one. `--other-password-env` is the script
form; the global `--password-env` still carries this vault's own password.

The other file is only ever read. Everything happens in this vault, which is
saved once at the end.

## What it does, item by item

Items are matched on the identity each one carries — not on its title, since two
accounts can share a name and renaming an item must not turn it into a different
one.

| On the other side | Here | Result |
| --- | --- | --- |
| present | missing | copied across, with its identity and dates |
| present | identical | left alone |
| newer | older | this one is updated |
| older | newer, also changed | **both are kept** — see below |
| missing | present | left alone |

## When both sides changed

The interesting case. If an item changed here *and* there since the copies
parted, sefy does not pick a winner:

```console
$ sefy merge ~/from-laptop.bak
merged: 0 added, 0 updated, 0 unchanged

1 item changed on both sides and could not be resolved here.
This vault's version was kept; the incoming one is beside it:
  "mail" → also kept as "mail (conflicted copy)"
Compare them, keep the right one, and remove the other.
```

Both versions are now in the vault, and you decide:

```sh
sefy get mail                      # this vault's version
sefy get "mail (conflicted copy)"  # the one that arrived
sefy rm "mail (conflicted copy)"   # once you have chosen
```

"Newest wins" is a fine rule for a title and a ruinous one for a password: the
older copy may be the one that still opens the account. Nothing here throws a
secret away on a timestamp.

## Nothing is ever deleted

An item that exists here but not in the other vault stays. A merge cannot tell
"deleted over there" from "added over here" — from this side the two look
identical — and guessing wrong would destroy a secret silently.

So removals do not propagate. Remove an item in both places, or accept that a
merge will bring it back from a copy that still has it.

## Why this exists

Nothing in a vault file can warn you that two copies drifted: the format carries
no header, no timestamp and no counter in the clear, because any of those would
be the signature it deliberately avoids. There is no locking either — two
machines saving the same file means the second save wins whole.

`merge` is the answer to that, applied afterwards with both passwords in hand.

## Related

- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — how copies drift in the first place
- [`import`](/sefy/reference/import/) — bringing in contents from a plain JSON export
- [`rm`](/sefy/reference/rm/) — clearing up after a conflict
