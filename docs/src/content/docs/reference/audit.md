---
title: "audit"
description: Find weak, reused and old passwords, secrets kept twice, and records past or near the day they stop working - offline, without showing a value.
---

What in the vault deserves a second look: passwords that are easy to guess,
shared between items or unchanged for years; a card, a token or a key kept in
two items; records that have stopped working or are about to. Everything is
worked out on this machine from the vault alone.

## Usage

```sh
sefy audit
```

```console
$ sefy audit
Master password:
looked at 5 passwords and 4 dates in 9 items

weak        3 passwords are easy to guess
            5  home       wifi       password  strength 1/4
            2  forum      login      password  strength 2/4
            3  shop       login      password  strength 2/4

reused      1 password is shared by more than one item
         1) 2  forum      login      password
            3  shop       login      password

duplicated  1 card, token or key is in more than one item, likely a leftover copy
         1) 6  visa       card       number
            7  visa 2029  card       number

old         1 password has been unchanged for over a year
            4  bank       login      password  since 2023-07-01 (1200 days)

expired     1 record is past the last day it works
            9  deploy     api-token  expires   2026-09-30 (13 days ago)

expiring    2 records stop working within 30 days
            8  github     api-token  expires   2026-10-20 (in 7 days)
            6  visa       card       expires   2026-10 (in 18 days)

error: 9 findings
```

Each line is an item by id and title, the field the finding is about, and what
is wrong with it. A shared secret is one finding, numbered, with every item
that holds it beneath.

## What it looks for

| Finding | What it means |
| --- | --- |
| `weak` | A password zxcvbn scores below 3 of 4: guessable by someone who can try passwords offline. The record's title, login and address count against it, so `quillfeather1815` for `quillfeather.net` is weak however long it is. |
| `reused` | The same password in more than one item. One site leaking it opens the others. |
| `duplicated` | The same card number, token, private key, account or one-time password key in more than one item - almost always an old item left behind when a new one was added. |
| `old` | A password that has had its value for over a year. |
| `expired` | A record past the last day it works. |
| `expiring` | A record with 30 days or fewer left. |
| `unreadable` | An `expires` value sefy cannot count from, such as one another tool wrote. |

## Which secrets are checked

A secret is checked for what it is, and the [kind's fields](/sefy/reference/add/)
say what that is.

- **Passwords** - a login's and a Wi-Fi network's `password`, an SSH key's
  `passphrase` - are chosen by a person: checked for strength, reuse and age.
  A secret field named `password` or `passphrase` added to any other record is
  a password too.
- **Issued secrets** - a card's `number`, a token, a private key, an account,
  a one-time password key - are not chosen, so they are not weak or old in the
  same sense. Two items holding the same one is what is checked.
- **A PIN and a CVV** are not checked: every PIN is four digits, and two cards
  sharing a CVV is chance.

Notes and files are not looked into.

## How old is a password

Not how old the item is. Every change to a record keeps what it replaced as a
[version](/sefy/reference/history/), so the audit walks back through them and
finds when the password took the value it has now. A new address for a login is
a new version with the same password in it, and does not make the password any
younger; a password brought back with [`restore`](/sefy/reference/restore/) is
as old as its return.

A vault [imported](/sefy/reference/import/) from elsewhere starts with the time
the other tool last changed each entry, so a password from an old KeePass
database is as old as KeePass says.

## When a record stops working

Any record can say when it stops working, in its `expires` field: a card as
printed, a token from what the site said when it issued it.

```sh
sefy add card visa --expires 10/26
sefy edit github --set expires=2026-10-20
```

It is kept in one of two shapes. `2026-10-20` is the **last day it works**;
`2026-10` works through the end of that month, which is how a card prints it.
A card's `10/26` and `10/2026` are read as `2026-10`, and anything that is not a
date is refused with what to type instead - an audit cannot count from "next
spring".

Taking a secret from a record that has stopped working, or has 30 days or fewer
left, says so on stderr - [`get`](/sefy/reference/get/),
[`open`](/sefy/reference/open/), [`fill`](/sefy/reference/fill/),
[`otp`](/sefy/reference/otp/) and [`run`](/sefy/reference/run/) alike - so a
pipe still receives the secret alone:

```console
$ sefy get deploy --stdout
warning: "deploy" expired 13 days ago (2026-09-30)
ghp_...
```

Dates are counted in whole days, UTC, like every date sefy keeps.

## Nothing leaves this machine

The audit never looks anything up. It does not check passwords against a list of
breached ones, not even by sending part of a hash: a feature that phoned out
would be the first thing about sefy to put a vault's contents, in any form, on a
network.

It never prints a value either, nor why zxcvbn thought little of a password:
"dates are easy to guess" is already a fact about what the password contains.
It does print titles, as [`ls`](/sefy/reference/ls/) does.

## Exit status

`0` when there is nothing to look at, `1` when there is, after the whole report
is printed. A scheduled `sefy audit` is a check that says when to look:

```sh
sefy audit > /dev/null || notify-send "sefy: something to look at"
```

## Related

- [`gen`](/sefy/reference/gen/) - a password that is neither weak nor anyone else's
- [`edit`](/sefy/reference/edit/) - put the new one in with `--set-secret password`
- [`history`](/sefy/reference/history/) - the versions a password's age is read from
- [`status`](/sefy/reference/status/) - what and where the vault is, without its contents
