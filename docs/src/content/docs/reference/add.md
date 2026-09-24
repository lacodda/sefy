---
title: "add"
description: Add a note, a login, a card, an SSH key, a Wi-Fi network, an API token, a bank account or a file to the vault.
---

Adds an item. One subcommand per kind — an item's **kind cannot change** later,
so this choice is made once.

A note holds text and a file holds bytes. Everything else — a login, a card, an
SSH key, a Wi-Fi network, an API token, a bank account — is a **record**: a set of named fields, each of which is either
public or secret. The kind decides which fields `add` offers and which of them
`show` hides; it does not fence the record in, and
[`edit --set`](/sefy/reference/edit/) can add a field the kind never mentioned.

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

## `sefy add login <TITLE>`

Fields: `login`, **`password`**, `url`, **`totp`**, `notes` (secret ones in
bold).

| Option | Meaning |
| --- | --- |
| `-l, --login <LOGIN>` | Username, email, whatever the service calls it. Required. |
| `-u, --url <URL>` | Where the account lives. |
| `--totp <KEY>` | Key for one-time passwords: the text key or the `otpauth://` link. Checked on the way in. |
| `--notes <TEXT>` | Anything else worth remembering. |
| `--item-password-env <VAR>` | Read the account password from this variable instead of prompting. |
| `--tag <TAG>` | Tags. |

```console
$ sefy add login mail --login someone@example.com --url https://mail.example.com --tag mail
Password for this item:
added "mail" as 2
```

The account password is prompted for separately. `--totp` passes the key on
the command line, where it reaches the shell history; the quieter way is to add
the login without it and store the key with
[`sefy otp <TITLE> --set`](/sefy/reference/otp/), which asks for it. Note that
`--item-password-env` is deliberately distinct from the global
`--password-env`: with one variable for both, the master password would end up
stored as the account's password.

This kind was called `credential` up to 0.6.0, and that spelling is still
accepted — by `add`, by `--kind`, and in vaults and exports written back then.
Everything sefy writes from now on says `login`.

## `sefy add card <TITLE>`

Fields: **`number`**, `holder`, `expiry`, **`cvv`**, **`pin`**, `notes`.

| Option | Meaning |
| --- | --- |
| `--holder <NAME>` | Name embossed on the card. |
| `--expiry <DATE>` | Expiry date, as printed. |
| `--notes <TEXT>` | Bank, account, anything else. |
| `--no-cvv` | Do not ask for the CVV. |
| `--no-pin` | Do not ask for the PIN. |
| `--tag <TAG>` | Tags. |

```console
$ sefy add card "visa" --holder "A LOVELACE" --expiry 01/29 --tag money
Card number:
CVV:
PIN:
added "visa" as 4
```

The number, the CVV and the PIN are **prompted for**, never passed as options:
an argument lands in the shell history and is visible in every process listing
on the machine while the command runs. A card you do not know the PIN of takes
`--no-pin`, and one with no CVV takes `--no-cvv`; answering a prompt with
nothing does the same. Nothing typed is nothing stored — the field is left out
rather than kept empty, since "there is none" and "it is the empty string" are
different claims.

## `sefy add ssh-key <TITLE>`

Fields: **`private-key`**, **`passphrase`**, `public-key`, `host`, `notes`.

| Option | Meaning |
| --- | --- |
| `--private-key <PATH>` | Private key file to read. Required. |
| `--public-key <PATH>` | Public key file; defaults to the private key's path with `.pub` appended, when there is such a file. |
| `--host <HOST>` | Where the key is used. |
| `--notes <TEXT>` | Anything else worth remembering. |
| `--no-passphrase` | Do not ask for the passphrase; for a key that has none. |
| `--tag <TAG>` | Tags. |

```console
$ sefy add ssh-key "deploy key" --private-key ~/.ssh/id_ed25519 --host example.com --tag keys
Passphrase for the key:
added "deploy key" as 5
```

The private key is read from a file rather than typed — it is multi-line and
would not survive a prompt — and stored as a **field**, not as an attachment,
so it can be piped straight into a command:

```sh
sefy get "deploy key" --field private-key --stdout | ssh-add -
```

Storing the key as a file instead is still a reasonable choice when what you
want back is a file on disk with its own path; see
[Keeping ssh keys in a vault](/sefy/guides/ssh-keys/).

## `sefy add wifi <TITLE>`, `api-token`, `bank`

Three kinds that are nothing but their fields, and take them the same way:
public fields with `--set NAME=VALUE`, secret ones asked for one after another.

| Kind | Fields (secret ones in bold) |
| --- | --- |
| `wifi` | `ssid`, **`password`**, `security`, `notes` |
| `api-token` | **`token`**, `url`, `scopes`, `expires`, `notes` |
| `bank` | **`account`**, `holder`, `bank`, `routing`, `notes` |

| Option | Meaning |
| --- | --- |
| `--set <NAME=VALUE>` | Set a public field. Repeat for several. |
| `--secret-env <NAME=VAR>` | Read a secret field from this variable instead of asking. |
| `--skip <NAME>` | Do not ask for this secret field. |
| `--tag <TAG>` | Tags. |

```console
$ sefy add wifi home --set ssid=HomeNet --set security=WPA3 --tag house
password (the network key; empty to leave out):
added "home" as 2

$ sefy add api-token ci --set url=https://example.com/settings/tokens --set scopes=read
token (the token itself; empty to leave out):
added "ci" as 3
```

A secret field passed with `--set` is **refused**, not quietly accepted: it
would already be in the shell history, and calling it secret after that would
be a promise sefy cannot keep. A prompt answered with nothing, like `--skip`,
leaves the field out. A name the kind does not have — `--set branch=Main street`
on a bank account — is kept as an extra field, the way
[`edit --set`](/sefy/reference/edit/) keeps one.

[`get`](/sefy/reference/get/) with no `--field` takes the first secret: the
network key, the token, the account number.

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

- [`edit`](/sefy/reference/edit/) — change an item afterwards, and add fields
- [`get`](/sefy/reference/get/) — take one field out
- [`extract`](/sefy/reference/extract/) — write a stored file back to disk
- [`import`](/sefy/reference/import/) — add many items at once
- [Keeping ssh keys in a vault](/sefy/guides/ssh-keys/)
