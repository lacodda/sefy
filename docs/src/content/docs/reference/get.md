---
title: "get"
description: Copy a secret to the clipboard, or print it with --stdout.
---

Copies a secret **to the clipboard**, so it does not end up in the terminal
scrollback.

## Usage

```sh
sefy get <REFERENCE>
```

| Option | Meaning |
| --- | --- |
| `--field <NAME>` | Which field of a record to take. Omit for the kind's own secret. |
| `--stdout` | Print the secret instead of copying it. |
| `--clear-after <SECONDS>` | Clear the clipboard again after this long. Default `45`; `0` leaves it. |

```console
$ sefy get mail
copied password of "mail" to the clipboard; clearing in 45s
clipboard cleared
```

```sh
sefy get mail --field login
sefy get visa --field expiry
sefy get bank --clear-after 0       # leave it there
sefy get bank --stdout | wl-copy    # for pipes and scripts
```

## Which field, when you do not say

`--field` takes any field the record carries, by name. Omitted, sefy takes the
one the kind is mostly about — its first secret field:

| Kind | Default field |
| --- | --- |
| `login` | `password` |
| `card` | `number` |
| `ssh-key` | `private-key` |
| `note` | the text |

A record whose fields are all public has no such default, and sefy says so
rather than guessing which value you meant. A note has no fields at all, so
`--field` on one is refused rather than quietly handing over its text. A name
the record does not carry is an error that lists what it does carry:

```console
$ sefy get mail --field pin
error: "mail" has no "pin"; it holds: login, password, url, totp, notes
```

`--stdout` is what scripts want, but the secret then lives in the scrollback and
— if the command is recalled — in the shell history.

## The clipboard timeout

sefy waits for the timeout before exiting, so the command sits there until the
secret is taken back off. It clears the clipboard **only if the secret is still
what is on it** — anything copied in the meantime is left alone.

On Linux this works differently, because X11 and Wayland make the *owning
process* serve the clipboard: sefy keeps serving the value for the timeout and
then lets go, so the secret disappears when sefy exits either way. There
`--clear-after 0` means "hold it for a long while" rather than "leave it
forever", since letting go immediately would make the value unpastable.

A clipboard manager that keeps history gets the secret regardless — the timer
clears the clipboard, not someone else's copy of it.

## Stored files

`get` will not hand back a file, not even with `--stdout`:

```console
$ sefy get id_ed25519 --stdout
error: "id_ed25519" is a file; write it to disk with: sefy extract 3
```

A key or a binary dumped into a terminal is a key in the scrollback, and pasting
one through the clipboard would corrupt it. Files leave through
[`extract`](/sefy/reference/extract/).

## Related

- [`show`](/sefy/reference/show/) — an item's surroundings, without its secrets
- [`extract`](/sefy/reference/extract/) — stored files
- [Versions and compatibility](/sefy/concepts/versions/) — items a newer sefy wrote
