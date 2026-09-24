---
title: "find"
description: Search items by text, kind and tags.
---

The same listing as [`ls`](/sefy/reference/ls/), narrowed by text.

## Usage

```sh
sefy find [TEXT] [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--kind <KIND>` | `note`, `login`, `card`, `ssh-key`, `wifi`, `api-token`, `bank` or `file`. |
| `--tag <TAG>` | Keep only items carrying **every** listed tag. |

```console
$ sefy find mail
2  mail  login  [mail]
```

```sh
sefy find bank --kind login
```

## What is searched

Titles, note bodies and a record's public fields. The **contents of stored
files are not**: a match inside a binary would say nothing useful, and
searching them would mean decompressing and scanning every attachment on every
query. Secret fields — a login's password and TOTP, a card's number and cvv,
an ssh key's private key and passphrase — are deliberately not searched
either: the same reasoning that keeps `find` out of file contents keeps it out
of anything meant to stay hidden until you ask for it by name.

Finding nothing is a normal result, not an error:

```console
$ sefy find zzz
no items
```

This is where `find` differs from a `<REFERENCE>`: commands that act on one item
refuse to guess between several matches, while `find` exists precisely to show
you all of them.

## Related

- [`ls`](/sefy/reference/ls/) — the unfiltered listing
- [`show`](/sefy/reference/show/) — one item in full
