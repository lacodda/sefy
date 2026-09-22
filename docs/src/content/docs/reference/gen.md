---
title: "gen"
description: Generate a password or a passphrase and copy it to the clipboard.
---

Makes a new secret and puts it **on the clipboard**, the way
[`get`](/sefy/reference/get/) hands over a stored one. No vault is needed — unless
the result is kept with `--save`, which stores it as a new login in the same
gesture.

## Usage

```sh
sefy gen [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `-n, --length <N>` | Length in characters. Default `20`. |
| `--no-uppercase` | Leave out upper-case letters. |
| `--no-digits` | Leave out digits. |
| `--no-symbols` | Leave out symbols, for the sites that refuse them. |
| `--pronounceable` | Alternate consonants and vowels, so it can be read out and typed. |
| `-w, --words <N>` | Make a passphrase of this many words instead of a password. |
| `--lang <en\|ru>` | Which word list a passphrase comes from. Default `en`. |
| `--separator <TEXT>` | What goes between the words. Default `-`. |
| `--save <TITLE>` | Store the result as a new login under this title. |
| `-l, --login <LOGIN>` | The saved record's login. |
| `-u, --url <URL>` | Where the saved account lives. |
| `--tag <TAG>` | Tags for the saved record; repeat or separate with commas. |
| `--stdout` | Print the result instead of copying it. |
| `--clear-after <SECONDS>` | Clear the clipboard again after this long. Default `45`; `0` leaves it. |

```console
$ sefy gen
generated 20 characters: 130 bits of entropy, strength 4/4
copied it to the clipboard; clearing in 45s
clipboard cleared
```

## Signing up in one gesture

Creating an account is the moment a new password is needed, and the moment it
most needs keeping. `--save` does both: the password is stored as a login and
then copied, ready to paste into the sign-up form.

```console
$ sefy gen --save forum --login someone@example.com --url https://forum.example.com
added "forum" as 1
generated 20 characters: 130 bits of entropy, strength 4/4
copied it to the clipboard; clearing in 45s
clipboard cleared
```

The record is written **before** the clipboard is touched. A clipboard that
cannot be reached must not cost a password the site has already accepted — and
a vault that refuses the master password stops the command before anything is
generated at all.

The saved record is an ordinary [`login`](/sefy/reference/add/): change it with
[`edit`](/sefy/reference/edit/), open its site with
[`open`](/sefy/reference/open/).

## Three recipes

**Random characters** — the default. Lower-case letters are always in;
upper-case, digits and symbols are in unless left out, and **each class that is
in appears at least once**, because that is what the sites asking for "a digit
and a symbol" check. The symbols are `!#$%&()*+,-./:;<=>?@[]^_{|}~` — no quotes,
backtick, backslash or space, the characters that shells, config files and
careless forms mangle.

```console
$ sefy gen --length 32 --no-symbols --stdout
generated 32 characters: 191 bits of entropy, strength 4/4
c8ayV9vpyE6NZuql2IgPNxIqZO37O3tx
```

**Pronounceable** — consonants and vowels in turn, easier to read out over the
phone or type from a screen. It carries fewer bits per character, so it wants
more characters for the same strength:

```console
$ sefy gen --pronounceable --length 24 --stdout
generated 24 characters: 79 bits of entropy, strength 4/4
canedazacimijowiwegedaxa
```

**Words** — a passphrase, for the secrets typed by hand rather than pasted: a
master password, a disk encryption key.

```console
$ sefy gen --words 6 --stdout
generated 6 words: 78 bits of entropy, strength 4/4
agreeably-cape-valid-unwary-widget-prozac

$ sefy gen --words 8 --lang ru --stdout
generated 8 words: 83 bits of entropy, strength 4/4
улей-суббота-жетон-клумба-май-ленивец-пить-йога
```

| List | Words | Bits per word | Source |
| --- | --- | --- | --- |
| `en` | 7776 | 12.9 | [EFF large wordlist](https://www.eff.org/dice), CC BY 3.0 US |
| `ru` | 1296 | 10.3 | [Russian Diceware 4d6](https://github.com/igor-malinyak/Russian-Diceware-4d6), CC0 1.0 |

Both lists are inside the binary; nothing is fetched. In the Russian list `ё` is
written as `е`, so a keyboard that makes `ё` awkward cannot turn a correctly
remembered passphrase into a failed sign-in — no two words collide because of
it.

## What the numbers mean

**Bits of entropy** is exact, not estimated: it is counted from how the secret
was drawn. Every draw comes from the operating system's random source, and
every choice is uniform. A password that must hold every class is drawn whole
and drawn again until it does, so no class sits at a predictable place.

**Strength** is [zxcvbn](https://github.com/dropbox/zxcvbn)'s 0–4 scale, worked
out offline: how many guesses an attacker who knows how people pick passwords
would need, with the record's title, login and URL counted against it. zxcvbn
judges a string by how it looks, so for a generated secret it is held to the
entropy — two words from the list look long, but they are 26 bits, and sefy
scores them that way:

```console
$ sefy gen --words 2 --stdout
generated 2 words: 26 bits of entropy, strength 2/4
under 64 bits: fine behind a site that limits sign-in attempts, too few for a master password; add length or words
overbid-gaining
```

The scale measures guessing through a sign-in form. A master password or a disk
key is attacked offline, at whatever speed the attacker's hardware allows, and
wants at least 64 bits — sefy says so whenever a result falls short.

## Scripts

With `--stdout` the secret is the only thing on stdout; the description goes to
stderr, so a pipe receives the secret alone:

```sh
sefy gen --stdout 2>/dev/null | some-command --password-stdin
```

## Related

- [`get`](/sefy/reference/get/) — copy a stored secret to the clipboard
- [`add`](/sefy/reference/add/) — store a login with a password you already have
- [`edit`](/sefy/reference/edit/) — replace a stored password: `--set-secret password`
