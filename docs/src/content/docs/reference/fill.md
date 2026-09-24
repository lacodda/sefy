---
title: "fill"
description: Hand over a record's fields one at a time, in the order a form asks for them.
---

Signing in is three pastes: the login, the password, then the one-time code.
`fill` puts each one on the clipboard in turn, and **Enter** moves to the next.

## Usage

```sh
sefy fill <REFERENCE> [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--clear-after <SECONDS>` | Clear the last field from the clipboard after this long. Default `45`; `0` leaves it. |

```console
$ sefy fill github
1/3 login is on the clipboard; paste it, then press Enter
2/3 password is on the clipboard; paste it, then press Enter
3/3 one-time code (valid for 21s) is on the clipboard; clearing in 45s
clipboard cleared
```

Each field replaces the one before it, so only the last is left for the timer
to clear. The one-time code is made **when its turn comes**, not when the
command starts: time spent on the login and the password does not eat into it.

## Which fields, in what order

The kind decides, the same way it decides what [`show`](/sefy/reference/show/)
hides. Only the fields a record actually holds are offered.

| Kind | Fills |
| --- | --- |
| `login` | `login`, `password`, a code from `totp` |
| `card` | `number`, `holder`, `expiry`, `cvv` |
| `ssh-key` | `passphrase` |
| `wifi` | `ssid`, `password` |
| `api-token` | `token` |
| `bank` | `account`, `holder`, `bank`, `routing` |

A card's PIN is not among them: it is typed at a terminal, never into a form.
A record with none of these fields says what it does hold:

```console
$ sefy fill box
error: "box" holds nothing a ssh-key fills in; it holds: private-key, public-key
take a field with: sefy get 5 --field NAME
```

Notes and files have no fields to fill and are refused.

## Only at a terminal

`fill` waits for a key press between fields, so it needs a terminal to wait on.
Run from a script it stops before touching the clipboard and points at the
command that suits a script:

```console
$ sefy fill github < /dev/null
error: fill waits for Enter between fields, and input is not a terminal
take one field at a time with: sefy get 1 --field NAME --stdout
```

If input closes part-way through, the field on the clipboard is taken back off
before sefy stops. Interrupting with Ctrl+C is the one way to leave a field
there: sefy has no chance to act on it.

## Related

- [`otp`](/sefy/reference/otp/) — the one-time code on its own, and storing the key
- [`open`](/sefy/reference/open/) — open the site and copy the password
- [`get`](/sefy/reference/get/) — any single field
