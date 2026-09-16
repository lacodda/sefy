---
title: "open"
description: Open an item's site and copy its password to the clipboard.
---

The two halves of signing in, in the order they are used: the browser loads
while the password waits to be pasted.

## Usage

```sh
sefy open <REFERENCE> [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--no-password` | Open the site without touching the clipboard. |
| `--clear-after <SECONDS>` | Seconds before the clipboard is cleared; `0` leaves it there. Default 45. |

```console
$ sefy open github
opened https://github.com/login
copied password of "github" to the clipboard; clearing in 45s
clipboard cleared
```

The clipboard is the same path [`get`](/sefy/reference/get/) uses, timeout and
all. A browser being open does not make a password on the clipboard any less
exposed, so the timer runs the same way.

## What it opens

The record's `url` field. Add one to a record that has none:

```console
$ sefy open mail
error: "mail" has no "url" field; it holds: login, password
add one with: sefy edit 3 --set url=https://…
```

Only `http` and `https` addresses are opened. A `file:///` path,
`javascript:` or a `ms-settings:` handler is something a launcher would act on
and is not a site a login belongs to, so sefy refuses and shows what is stored:

```console
$ sefy open odd
error: "file:///C:/Windows/System32/calc.exe" is not an http(s) address; sefy only opens web addresses
```

A note or a stored file has no site at all, and says so.

## The password it copies

Whatever the kind's main secret is: a login's password, a card's number, an SSH
key's private key. A record with no secret field opens its site and stops there
— the site is open, which is what was asked.

To reach a different field, use [`get --field`](/sefy/reference/get/).

## Related

- [`get`](/sefy/reference/get/) — a secret on its own, any field
- [`show`](/sefy/reference/show/) — an item without its secrets
- [`edit`](/sefy/reference/edit/) — add or change the `url` field
