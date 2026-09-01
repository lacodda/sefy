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
| `--set <NAME=VALUE>` | records | Set a field. Repeat for several. |
| `--set-secret <NAME>` | records | Prompt for a field's value and mark it secret. |
| `--unset <NAME>` | records | Remove a field. |
| `--tag <TAG>` | all | Replace the item's tags. |
| `--clear-tags` | all | Remove every tag. |

```console
$ sefy edit bank --title "bank card"
updated 1
```

## Changing a record's fields

A record — a login, a card, an SSH key — is a set of named fields, and `--set`
writes one of them:

```sh
sefy edit mail --set url=https://mail.example.com
sefy edit mail --set login=someone@example.com --set notes="work account"
```

A field the record does not have yet is **added**, which is how a record grows
past what its kind suggested:

```sh
sefy edit "deploy key" --set fingerprint=SHA256:…
```

A value passed this way is **public**: it has already been through the shell
history, so calling it secret would be a promise sefy cannot keep. A field that
already exists keeps whatever secrecy it was stored with — changing a password's
value does not expose it.

For a value that must stay off the command line, `--set-secret` asks for it the
way the master password is asked for, and marks the field secret:

```console
$ sefy edit mail --set-secret password
New value for password:
updated 2
```

`--unset` removes a field. A record cannot be left with none — an item with no
contents is a `rm` in disguise:

```console
$ sefy edit mail --unset url
updated 2
```

## What cannot change

An item's **kind is fixed for its lifetime**: a note stays a note. To change
kind, add a new item and remove the old one.

Flags meant for another kind are an error rather than a silent no-op:

```console
$ sefy edit bank --set login=someone
error: this item is a note; --set, --set-secret and --unset apply to records
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
