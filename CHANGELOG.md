# Changelog

All notable changes to this project are documented in this file.

A vault stays readable by every later sefy. The **file format** is frozen at
version 1; the **database schema** inside the ciphertext is versioned separately
and does move — 0.2.0 gave items an identity, 0.7.0 turned records into named
fields, 0.8.0 gave the vault a place to record facts about itself — and an
older vault is migrated in on load.

What an *older build* makes of a migrated vault depends on the change. The
0.2.0 migration left everything readable by 0.1.x. The 0.7.0 one does not: a
login is a kind 0.6.0 has never heard of, so that build lists, exports and syncs
it while saying to upgrade before reading it — the forward-compatibility
contract from 0.6.0, not a break.

Any change that would break an existing file gets its own "Breaking Changes"
section here, with the migration path or a plain statement that there is none.
This paragraph survives regenerating the file.

## [0.11.0] - 2026-09-26

Secrets for a process: a script gets its token from the vault, not from a
`.env` file or an `export` in the shell history.

**`sefy run`** starts a command with secrets in its environment:
`sefy run -e GITHUB_TOKEN=github -e DB_USER=db#login -- ./deploy.sh`. Each
`--env` sets one variable from one item. The value is what `get` would take,
the item's own secret, and `#FIELD` names another field of a record. Every
variable is settled before the command starts. A reference that matches
nothing or several items stops the run, so the command never starts with half
of what it needs. The variable that carried the master password is left out of
the command's environment: the command was given a token, not the key to the
whole vault.

The command behaves as if it had been typed on its own. On Linux and macOS
sefy replaces itself with it, so signals, job control and the exit status are
the command's. On Windows sefy starts it, leaves Ctrl+C to it and exits with its
status. A bare name is found the way a shell finds it, so `npm`, `pnpm` and
other `.cmd` tools work as typed. When the command never ran, the exit status
says why, the way `env` does: 125 when sefy could not set the variables, 126
when the program would not start, 127 when there is no such program.

### Fixed

- **The master password is asked for with stdin piped.** The prompt reads the
  terminal itself, yet sefy refused to ask whenever stdin was not a terminal.
  Now `echo text | sefy add note draft` and `cat data | sefy run ...` ask as
  they should. With no terminal at all, sefy still refuses rather than hanging.
- **`get --field` on a note is refused.** It used to hand back the note's text
  and ignore the field. Someone who names a field expects a record, and the
  text may not be what they meant to take.

## [0.10.0] - 2026-09-24

Two-factor sign-in without a second app, and three more kinds of record.

**`sefy otp`** makes the one-time code a site asks for after the password, and
puts it on the clipboard with the same timeout `get` uses. It says how many
seconds the code has left. `--set` stores the key the site shows when
two-factor sign-in is turned on and answers with the first code in the same
command, because that is what the site asks for next. Either shape works: the
`otpauth://` link inside the QR code, or the text key printed beside it. A
link is kept word for word, so its issuer, digits, period and hash are the
site's own. A text key is kept without the spaces it was printed with.
`--qr` draws the key as a QR code for an authenticator app on a phone. The
picture is the key, so it is drawn only on a terminal and wiped from the screen
and the scrollback once Enter is pressed. Codes follow RFC 6238, with SHA-1,
SHA-256 or SHA-512, and the tests check them against the RFC's own vectors.

**`sefy fill`** hands over a record's fields one at a time, in the order a
form asks for them: a login gives its login, then its password, then a fresh
code. Enter moves to the next. The code is made when its turn comes, so time
spent on the first two fields does not eat into it. The kind decides what is
filled, from the template: a card gives number, holder, expiry and CVV, but
never its PIN.

**Wi-Fi networks, API tokens and bank accounts** join notes, logins, cards,
ssh keys and files: `sefy add wifi`, `add api-token`, `add bank`. They take
public fields as `--set NAME=VALUE` and ask for secret ones in turn, or read
them with `--secret-env NAME=VAR`. A secret passed with `--set` is refused,
because the shell history already has it.

A one-time password key is now checked on every way in: `add login --totp`,
`edit --set`, `edit --set-secret` and `otp --set`. A key that cannot make a
code is found out while the setup page is still open. An empty answer to a
secret prompt now leaves the field out for every kind; cards and ssh keys used
to store an empty string. Two error messages about
items from a newer sefy were printed with stray indentation in the middle;
they no longer are.

### Breaking Changes

**Vault files: none.** The file format is still version 1 and the database
schema is still 4. A Wi-Fi network, an API token or a bank account is a kind
0.9.0 has not heard of: that build lists, exports and syncs one, can retitle
and retag it, and says to upgrade before reading it - the forward-compatibility
contract from 0.6.0. We checked with the published 0.9.0 binary, in both
directions: 0.9.0 created a vault, this build added the new kinds and made a
code from the key 0.9.0 stored, 0.9.0 wrote to it again, and this build read
everything back.

**`sefy-core` library:** `ItemKind` gains `Wifi`, `ApiToken` and `Bank`, and
`ItemKind::known` returns eight kinds. `FieldSpec` gains `fill`, and `Template`
gains `fill_order`. `Error` gains `InvalidOtpKey`. The new `otp` module holds
`Totp`, `Algorithm`, `normalize` and `FIELD`. Code that matches `ItemKind` or
`Error` exhaustively has to follow.

### Documentation
- Document one-time passwords, fill and the new kinds of record

### Features
- Make one-time password codes and add wifi, api-token and bank records
- Add sefy otp, sefy fill and the wifi, api-token and bank kinds

## [0.9.0] - 2026-09-22

New passwords, made where they are kept.

**`sefy gen`** makes a secret and puts it on the clipboard, with the same
timeout `get` uses. It needs no vault. There are three recipes: random
characters (the default: 20 of them, every class that is in appearing at least
once), pronounceable consonant-vowel strings for secrets that are read out, and
**diceware passphrases** (`--words N`) for secrets typed by hand, such as a
master password. The word lists are built into the binary: the EFF large list
for English, and Russian Diceware 4d6 for Russian, where `ё` is written as `е`
so a keyboard layout cannot turn a correctly remembered phrase into a failed
sign-in.

**`--save TITLE`** keeps the result as a new login in the same gesture, with
`--login`, `--url` and `--tag` for the rest of the record. The record is written
before the clipboard is touched, and a wrong master password stops the command
before anything is generated. A password a site has already accepted should
never exist only on a clipboard.

**Each secret reports its strength twice.** The first figure is its entropy in
bits, which is exact because it is counted from how the secret was drawn. The
second is a 0-4 score from **zxcvbn**, worked out offline. zxcvbn judges a
string by how it looks, and it rates two words from the list at the top of its
scale. So for a generated secret the score is capped by the entropy: two words
are 26 bits, and they score as 26 bits. Anything under 64 bits also gets a
warning: that is enough behind a site that limits sign-in attempts, but too few
for a master password, which can be attacked offline.

Every draw comes from the operating system's random source. Each choice is
made by rejection sampling, never by a bare modulo, and the tests are shown to
fail when either guarantee is removed.

### Breaking Changes

**Vault files: none.** The file format is still version 1 and the database
schema is still 4. A login saved by `gen` is an ordinary login. We checked with
the published 0.8.0 binary, which shows a record `gen --save` wrote, hands back
its password and writes to the same vault. The new build then reads everything
back.

**`sefy-core` library:** additions only. The new `generate` module holds
`Recipe`, `Classes`, `Language`, `Generated`, `generate` and `OFFLINE_BITS`.
The new `strength` module holds `Strength`, `estimate` and
`estimate_generated`. `Error` gains `GeneratorLength`.

### Documentation
- Make the readme a shopfront
- Drop the duplicate heading and lead with the promise
- Introduce gen in the readme, landing page and getting started

### Features
- Generate passwords and passphrases, and estimate strength offline
- Add sefy gen

## [0.8.0] - 2026-09-16

Three ways into a vault that do not require remembering exactly what you called
something.

**`sefy` on its own** opens a picker: type a few letters, press Enter. The
command line is exact and remembering is not — `sefy get github-work` only
helps someone who knows the title is not `github (work)`. It appears only when
there is a terminal at both ends; run from a script it says so and stops,
rather than drawing a prompt into a pipe and waiting for a keystroke that never
comes. `find` is unchanged and still always prints a listing, so a script and a
person get the same command.

**`sefy open`** does the two halves of signing in at once: the browser loads
while the password waits on the clipboard, with the same timeout `get` uses.
Only `http` and `https` addresses are opened — a `file:///` path or a
`javascript:` string is something a launcher would act on and is not a site a
login belongs to, so sefy shows what is stored and refuses.

**`sefy status`** answers the question asked after a new machine, a restore or a
week away: the file being used, how many items and of what kinds, how many
tags, the schema version, the installed transports, and when this vault last
reached a remote. It never prints a title or a value — a status is often read
with somebody else in the room.

That last line is new information, so the vault now records it. It lives inside
the sealed file rather than beside it: sefy keeps nothing on disk but the vault
and its transports, and a state file next to a vault would annotate the one
file that is deliberately unremarkable. It also travels — copy a vault to
another machine and it still knows when it last synced, because that is a fact
about the vault rather than about the computer holding it.

### Breaking Changes

**Vault files: none.** The file format is still version 1 and the file on disk
does not change shape. The database schema inside the ciphertext goes from 3 to
4, adding a small table for facts about the vault itself; a vault written by an
earlier release is migrated when it is opened and there is nothing to do by
hand.

**A migrated vault stays usable by 0.7.1.** Unlike the 2 → 3 move, this one
takes nothing away and renames nothing: 0.7.1 does not know the new table,
writes into the ones it does know, and leaves the rest as it found it. Checked
with the published 0.7.1 binary in both directions — it opens a migrated vault,
lists it, writes into it, and the newer build reads back both the new items and
the record of the last sync. A vault written before 0.8.0 reports `never`,
which is the honest answer.

**`sefy-core` library.** `sync::push` now takes `&mut Vault` rather than
`&Vault`: it records the transfer and saves before handing the file over, so
that the copy arriving at the remote carries the note about its own journey.
`Vault` gains `stats`, `last_sync` and `record_sync`, and the crate exports
`Stats` and `SyncStamp`.

### Features
- Record when a vault last reached a remote
- Add sefy status
- Add sefy open
- Open a picker when sefy is run with no command

### Testing
- Hold the docs site's mark to the repository's
- Hold every command to having a reference page

## [0.7.1] - 2026-09-16

A patch about the two things that meet a new user first: the icon on the file
they downloaded, and what the installer does to their machine.

sefy has carried a mark since 0.1.0, exported to a multi-size `.ico` and read
by nothing — the CLI had no build step for it, and a Windows executable with no
icon resource gets the generic one in Explorer, on a pinned shortcut and in the
properties dialog. It now carries the mark, and carries the right drawing at
each size: the filled tile up to 27px, the outlined tile to 63, the full mark
above. The exporter had been taking the smallest master for every size, so a
256px icon was a flat teal lozenge; that is fixed with it, along with the
directory order, which was smallest-first where some readers take the first
entry as the window icon.

The Windows installer no longer damages the user PATH. Writing it through the
.NET environment API — the obvious way, and what the script did — reads entries
like `%JAVA_HOME%\bin` expanded and writes the whole value back as a plain
string, so every such entry freezes at whatever the variable held during the
install and stops following it afterwards. The install succeeds, sefy runs, and
some unrelated program breaks weeks later. It now reads the value unexpanded,
writes it back with its type intact, and tells running shells, so a terminal
opened right after installing finds `sefy` without a sign-out.

Nothing about the vault file, the schema or any command changes.

### Bug Fixes
- Stop the Windows installer flattening the user PATH

### Features
- Give the executable its mark, one level per size

### Testing
- Hold the installers' example tag to the current release

## [0.7.0] - 2026-09-01

A login, a payment card and an SSH key differ in which fields they carry and
which of those are secret — not in how they are stored. From this release every
such record is an ordered set of named fields, and a kind of record is a
template over them rather than a table, a payload variant and a branch in every
function that reads an item. `card` and `ssh-key` arrive as two entries in that
template list; the next kind costs the same.

A field carries its own secrecy rather than having it looked up by name, so a
field no template mentions — one you add with `edit --set`, or one a future sefy
wrote — is still hidden when it should be. `sefy get --field` accordingly takes
any field name instead of four fixed ones, and without it takes what the kind is
mostly about: a login's password, a card's number, an SSH key's private key.

The kind `credential` is renamed to `login`. The old name still **parses**
everywhere it used to appear — in vaults, in exports, as `--kind credential`,
and as `sefy add credential` — but it is no longer **written**.

### Breaking Changes

**Vault files.** The file format is still version 1 and nothing about the file
on disk changes shape. The database schema inside the ciphertext goes from 2 to
3: a vault written by 0.6.0 or earlier is migrated when it is opened, with each
`credentials` row becoming fields in the same order and an absent optional
producing no field rather than an empty one. There is nothing to do by hand.

**A migrated vault is not fully readable by 0.6.0.** A login is a kind that
build has never heard of, so it lists the item, exports it, merges it and says
to upgrade before reading it — the forward-compatibility contract added in
0.6.0, working as intended. Nothing is lost and the newer build reads
everything, but if you sync a vault between machines, upgrade both.

**`sefy edit` lost its per-field credential flags.** `--login`, `--password`,
`--url`, `--totp`, `--notes` and `--item-password-env` are replaced by:

- `--set NAME=VALUE` — set a field; a field the record lacks is added.
- `--set-secret NAME` — prompt for the value and mark the field secret.
- `--unset NAME` — remove a field.

`sefy edit mail --login someone` becomes `sefy edit mail --set login=someone`.
`sefy add login` keeps `--item-password-env`; only `edit` lost it.

**`sefy-core` library.** `Payload::Credential` and the `Credential` struct are
gone, replaced by `Payload::Fields { kind, fields }` over the new `Field { name,
value, secret }`. `ItemKind::Credential` is now `ItemKind::Login`, joined by
`Card` and `SshKey`. `ItemKind::parse` still accepts `"credential"`.

### Documentation
- Describe records, their fields and the 0.7.0 migration

### Features
- Make a record a set of named fields
- Add card and ssh-key, and edit records field by field

### Testing
- Let the transport gate tell ssh-key from ssh

## [0.6.0] - 2026-08-27

### Bug Fixes
- An item from a newer sefy no longer breaks the vault it sits in

### Documentation
- Publishing a crate by hand does not connect it to this repository
- Say what holds when two machines run different versions
- V0.6.0

## [0.5.0] - 2026-08-21

### Documentation
- Note that a new crate's first version goes up by hand

### Features
- A transport that keeps the vault on a server over SSH

## [0.4.0] - 2026-08-21

### Bug Fixes
- Keep both versions when two edits share a timestamp
- Commit as sefy rather than as whoever is at the keyboard

### Documentation
- The landing page lists syncing among what sefy does

### Features
- Move a vault through a transport without opening it to one
- Push, pull and sync a vault through a transport
- A transport that keeps the vault in a git repository

### Refactoring
- One rule for what name addresses a plugin

### Testing
- Find the fixture transport the way a real one is found
- Put the fixture transport where macOS looks for one

## [0.3.1] - 2026-08-19

### Bug Fixes
- Point Windows shells at the PowerShell installer

## [0.3.0] - 2026-08-16

### Documentation
- Changelog and stability note for v0.3.0

### Features
- Let transports carry the vault without seeing it

### Testing
- Make the fixture plugin need nothing on PATH

## [0.2.0] - 2026-08-12

### Bug Fixes
- Declare the MSRV that actually builds
- Collapse a nested if into a let chain
- Give an identity to rows an older build inserted

### Documentation
- Add the Guides section
- Give every command its own page
- Changelog and README for v0.2.0

### Features
- Add one-line installers for Windows and Unix
- Give items an identity and merge two vaults on it

## [0.1.2] - 2026-08-10

### Bug Fixes
- Put the README back in the package

### CI
- Publish on tags again

## [0.1.1] - 2026-08-10

### Bug Fixes
- Publish as sefy-cli, not sefy

## [0.1.0] - 2026-08-09

### Bug Fixes
- Align the labels in sefy show

### Build
- Set up the release pipeline for v0.1.0

### CI
- Run fmt, clippy and tests on Linux, macOS and Windows
- Publish by hand for the first release

### Documentation
- Add MIT license and project readme
- Add lacodda line brand assets and readme banner
- Add the documentation site and brand rasters
- Stop the home page title rendering as "sefy | sefy"

### Features
- Rewrite the vault as a library with modern cryptography
- Resolve items by id, exact title or search text
- Add the sefy command-line tool
- Export and import a vault as plain JSON
- Clear the clipboard after a timeout
- Add export, import and $EDITOR support

### Testing
- Explain why the truncation test uses multi-byte data

