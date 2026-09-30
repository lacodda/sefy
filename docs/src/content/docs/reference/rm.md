---
title: "rm"
description: Remove an item.
---

Removes an item, asking first.

## Usage

```sh
sefy rm <REFERENCE> [-y]
```

| Option | Meaning |
| --- | --- |
| `-y, --yes` | Do not ask for confirmation. |

```console
$ sefy rm "work key"
remove "work key" (5)? [y/N] y
removed 5
```

Answering anything but yes leaves the item alone and says so:

```console
$ sefy rm "work key"
remove "work key" (5)? [y/N] n
kept
```

## In scripts

Without a terminal there is nobody to ask, so sefy refuses rather than assuming
yes:

```console
$ sefy rm "work key" < /dev/null
error: cannot ask for confirmation: input is not a terminal; pass --yes
```

`--yes` is then the deliberate way to say it:

```console
$ sefy rm "work key" --yes
removed 5
```

## Removal is final

There is no trash and no undo. The item's [history](/sefy/reference/history/)
goes with it — every earlier version of its contents — and the question says so
when there is one:

```console
$ sefy rm mail
remove "mail" (2) and 3 earlier versions? [y/N]
```

The vault is rewritten without the item, under a fresh salt and nonce, so the
previous contents are not recoverable from the new file.

A **backup copy** made earlier still holds the item — which is either your
safety net or the thing to remember when you remove something on purpose.

## Related

- [`ls`](/sefy/reference/ls/) — check what you are about to remove
- [`history`](/sefy/reference/history/) — what else goes with it
- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — backups
