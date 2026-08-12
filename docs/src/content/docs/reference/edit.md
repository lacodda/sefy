---
title: "edit"
description: Change an item's title, contents or tags.
---

Changes one item.

## Usage

```sh
sefy edit <REFERENCE> [OPTIONS]
```

| Option | Applies to | Meaning |
| --- | --- | --- |
| `--title <TITLE>` | all | New title. |
| `-t, --text <TEXT>` | notes | New body. |
| `-e, --editor` | notes | Open the current body in `$EDITOR`. |
| `-l, --login <LOGIN>` | credentials | New login. |
| `--password` | credentials | Prompt for a new account password. |
| `--item-password-env <VAR>` | credentials | Take the new account password from this variable. |
| `-u, --url <URL>` | credentials | New URL. |
| `--totp <SECRET>` | credentials | New TOTP secret. |
| `--notes <TEXT>` | credentials | New notes. |
| `--tag <TAG>` | all | Replace the item's tags. |
| `--clear-tags` | all | Remove every tag. |

```console
$ sefy edit bank --title "bank card"
updated 1
```

## What cannot change

An item's **kind is fixed for its lifetime**: a note stays a note. To change
kind, add a new item and remove the old one.

Flags meant for another kind are an error rather than a silent no-op:

```console
$ sefy edit bank --login someone
error: this item is a note; --login, --password, --url, --totp and --notes apply to credentials
```

An edit that would change nothing is an error too, so a mistyped flag cannot
look like a successful save:

```console
$ sefy edit "bank card"
error: nothing to change; pass --title, --tag, or a field to edit
```

## Tags are replaced, not added

`--tag` sets the item's tags to exactly what you list — it does not append. To
add one tag to an item that has two, name all three.

A tag left on no items disappears from [`tags`](/sefy/reference/tags/) by
itself; there is nothing to clean up by hand.

## Editing in `$EDITOR`

`--editor` opens the current body in `$VISUAL`, or `$EDITOR` if that is unset;
a value carrying its own arguments (`EDITOR="code --wait"`) works. With none
set, sefy says so rather than opening something you did not ask for.

While the editor is open, the note sits in a temporary file **in the clear**.
sefy overwrites and deletes that file as soon as the editor exits, but an
editor's own swap, undo and backup files are its business and outside sefy's
reach. If that matters for a particular note, use `--text`.

## Related

- [`add`](/sefy/reference/add/) — the fields each kind carries
- [`rm`](/sefy/reference/rm/) — removing an item instead
