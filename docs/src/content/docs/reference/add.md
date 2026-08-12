---
title: "add"
description: Add a note, a credential or a file to the vault.
---

Adds an item. Three kinds, three subcommands — an item's **kind cannot change**
later, so this choice is made once.

## `sefy add note <TITLE>`

| Option | Meaning |
| --- | --- |
| `-t, --text <TEXT>` | The note body. Omit to read it from stdin. |
| `-e, --editor` | Write the note in `$EDITOR` instead. |
| `--tag <TAG>` | Tags; repeat the flag or separate with commas. |

```sh
sefy add note "bank" --text "vault code 4815" --tag money,home
pbpaste | sefy add note "meeting notes"
sefy add note "journal" --editor
```

`--editor` opens `$VISUAL`, or `$EDITOR` if that is unset; a value carrying its
own arguments (`EDITOR="code --wait"`) works. There is no built-in default —
with none set, sefy says so rather than opening something you did not ask for.

While the editor is open, the note sits in a temporary file **in the clear**.
sefy overwrites and deletes that file as soon as the editor exits, but an
editor's own swap, undo and backup files are its business and outside sefy's
reach. If that matters for a particular note, use `--text`.

## `sefy add credential <TITLE>`

| Option | Meaning |
| --- | --- |
| `-l, --login <LOGIN>` | Username, email, whatever the service calls it. Required. |
| `-u, --url <URL>` | Where the account lives. |
| `--totp <SECRET>` | Shared secret for one-time passwords. |
| `--notes <TEXT>` | Anything else worth remembering. |
| `--item-password-env <VAR>` | Read the account password from this variable instead of prompting. |
| `--tag <TAG>` | Tags. |

```console
$ sefy add credential mail --login someone@example.com --url https://mail.example.com --tag mail
Password for this item:
added "mail" as 2
```

The account password is prompted for separately. Note that
`--item-password-env` is deliberately distinct from the global
`--password-env`: with one variable for both, the master password would end up
stored as the account's password.

## `sefy add file <PATH>`

| Option | Meaning |
| --- | --- |
| `-T, --title <TITLE>` | What to call it; defaults to the file name. |
| `--tag <TAG>` | Tags. |

```console
$ sefy add file ~/.ssh/id_ed25519 --tag keys
added "id_ed25519" as 3
```

Contents are stored byte for byte and come back identical. File **permissions
and timestamps are not** — sefy keeps contents, not metadata, so a restored key
needs its mode set again.

## Related

- [`edit`](/sefy/reference/edit/) — change an item afterwards
- [`extract`](/sefy/reference/extract/) — write a stored file back to disk
- [`import`](/sefy/reference/import/) — add many items at once
- [Keeping ssh keys in a vault](/sefy/guides/ssh-keys/)
