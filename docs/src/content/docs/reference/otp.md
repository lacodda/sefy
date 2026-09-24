---
title: "otp"
description: Copy a record's one-time password to the clipboard, store its key, or draw it for a phone.
---

Makes the short code a site asks for after the password, from the key stored
on the record, and puts it **on the clipboard** the way
[`get`](/sefy/reference/get/) hands over a password.

## Usage

```sh
sefy otp <REFERENCE> [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--set` | Store a key on the record first. It is asked for and never echoed. |
| `--key-env <VAR>` | Read the key to store from this variable instead of asking. Implies `--set`. |
| `--qr` | Draw the key as a QR code for an authenticator app on a phone. |
| `--stdout` | Print the code instead of copying it. |
| `--clear-after <SECONDS>` | Clear the clipboard again after this long. Default `45`; `0` leaves it. |

```console
$ sefy otp github
copied the one-time code of "github" to the clipboard; valid for 19s, clearing in 45s
clipboard cleared
```

"Valid for" is how long the code has before the next one replaces it. A site
usually accepts the previous code for a few seconds more, but pasting one with
two seconds left is a race worth not running: wait for the next.

## Turning on two-factor sign-in

The site shows a QR code and, beside it, a text key for when scanning is not an
option. Either one is what `--set` takes: the `otpauth://` link inside the QR
code, or the text key, spaces and all.

```console
$ sefy otp github --set
Setup key or otpauth:// link:
stored the one-time password key of "github"
copied the one-time code of "github" to the clipboard; valid for 24s, clearing in 45s
```

The first code follows straight away, because that is what the site asks for
next — proof the key was taken. Storing and answering are one command so the
key is in the vault **before** the site switches two-factor sign-in on.

What is not a key is refused on the way in, while the setup page is still open
and the key can still be fetched again:

```console
$ sefy otp github --set
Setup key or otpauth:// link:
error: not a one-time password key: the key is not base32 (letters A-Z and digits 2-7)
```

The message never quotes what was typed. A counter-based (HOTP) link is refused
by name: sefy makes time-based codes only.

## How the key is kept

In the record's `totp` field, secret, as it came:

- **A link** is kept word for word. Its issuer, account, number of digits,
  period and hash are the site's own, and codes are made with exactly those.
- **A text key** loses the spaces and dashes it was printed with and is
  upper-cased, so the same key is always stored the same way. It means what
  every site means by one: six digits, thirty seconds, SHA-1.

[`show`](/sefy/reference/show/) hides the field and points here. The key itself
can still be taken out with `sefy get <REFERENCE> --field totp` — to move it to
another tool, for instance.

## Onto a phone

```console
$ sefy otp github --qr
▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
…
scan it with the authenticator app, then press Enter to clear the screen
cleared
```

The picture **is** the key — anyone who can see it or photograph it has what the
phone has. So it is drawn only when both ends are a terminal, and the screen and
the scrollback are wiped as soon as Enter is pressed. Its colours are set
explicitly, so it reads the same on a light and a dark terminal; a terminal
that cannot draw colour (`NO_COLOR` set, or a very old console) gets a refusal
rather than an inverted code a phone may not read.

A key stored as text has no issuer or account of its own; the QR code names it
after the record's title and its `login` field, which is what the phone will
show.

## Related

- [`fill`](/sefy/reference/fill/) — login, password and this code, one after another
- [`get`](/sefy/reference/get/) — any field, including the key itself
- [`add`](/sefy/reference/add/) — `add login --totp` stores a key when the login is made
- [`edit`](/sefy/reference/edit/) — `--set-secret totp` replaces it
