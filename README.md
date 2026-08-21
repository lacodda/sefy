<p align="center"><img src="https://github.com/lacodda/sefy/raw/main/assets/banner.svg" alt="sefy - an inconspicuous encrypted vault" width="720"></p>

<p align="center">
  <a href="https://crates.io/crates/sefy"><img src="https://img.shields.io/crates/v/sefy?style=flat-square" alt="crates.io"></a>
  <a href="https://www.npmjs.com/package/sefy-cli"><img src="https://img.shields.io/npm/v/sefy-cli?style=flat-square" alt="npm"></a>
  <a href="https://github.com/lacodda/sefy/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/lacodda/sefy/ci.yml?branch=main&style=flat-square" alt="CI"></a>
  <a href="https://github.com/lacodda/sefy/blob/main/LICENSE"><img src="https://img.shields.io/crates/l/sefy?style=flat-square" alt="MIT"></a>
</p>

# sefy

Every way of storing secrets announces itself. A `.kdbx` file says "password
database". age and gpg write a header. VeraCrypt wants a container and a mount.
Whoever looks at your disk, your backup drive or your cloud folder can tell
exactly where the interesting file is.

**sefy is a secret store whose file looks like nothing.** Notes, credentials and
files live in an encrypted SQLite database sealed into a single blob with no
magic bytes, no header and no extension convention. Call it `notes.bak`, leave
it among your other backups, and there is nothing to notice.

```
$ head -c 32 notes.bak | xxd
00000000: 65b0 1375 a933 361d 89e2 9338 1b8d cb76  e..u.36....8...v
00000010: 0749 7515 ad12 dd40 1c3e 3e93 9282 4b5a  .Iu....@.>>...KZ
```

## Threat model, honestly

- **Protects against:** a passing glance, a curious file listing, a cloud-side
  scanner looking for known formats, anyone who does not already know the file
  is a vault.
- **Does not protect against:** forensic analysis - a high-entropy headerless
  file is recognizable as *some* container to an examiner - or anyone who can
  compel you to give up the password.

sefy is **inconspicuous, not deniable**, and it will not pretend otherwise.

## A day with it

Point sefy at a file. There is no default location: a vault at a predictable
path would undo the whole point.

```
$ export SEFY_VAULT=~/backups/notes.bak

$ sefy init
Master password:
created /home/you/backups/notes.bak
```

Put things in. Notes, logins, and files kept byte for byte.

```
$ sefy add note "bank card" --text "PIN 4815" --tag money
added "bank card" as 1

$ sefy add credential mail --login someone@example.com --url https://mail.example.com --tag mail
Password for this item:
added "mail" as 2

$ sefy add file ~/.ssh/id_ed25519 --tag keys
added "id_ed25519" as 3
```

Look around.

```
$ sefy ls
3  id_ed25519  file        [keys]
2  mail        credential  [mail]
1  bank card   note        [money]

$ sefy show mail
id:       2
title:    mail
kind:     credential
tags:     mail
login:    someone@example.com
password: <hidden — use sefy get>
url:      https://mail.example.com
```

Take a secret out. It goes to the clipboard and is taken back off after 45
seconds - sefy clears it only if the secret is still what is sitting there, so
anything you copied meanwhile is left alone.

```
$ sefy get mail
copied password of "mail" to the clipboard; clearing in 45s
clipboard cleared

$ sefy get "bank card" --stdout
PIN 4815
```

Items are addressed by title, by an exact id, or by text to search for. When
your words could mean more than one thing, sefy shows what they could mean
instead of guessing:

```
$ sefy get ma
error: 2 items match "ma":
     4  mailing list                    note
     2  mail                            credential
narrow the text, or use an id
```

Several machines? `sefy sync` carries the vault through a transport and folds
what comes back into this one:

```
$ sefy sync
Master password:
synced "vault" through github
merged: 2 added, 0 updated, 14 unchanged
```

Or fold in a copy you carried across by hand. Either way, where the two disagree
about the same item, sefy keeps both versions rather than letting a timestamp
decide which password you get to keep:

```
$ sefy merge ~/from-laptop.bak
Password for /home/you/from-laptop.bak:
merged: 1 added, 1 updated, 1 unchanged

1 item changed on both sides and could not be resolved here.
This vault's version was kept; the incoming one is beside it:
  "mail" → also kept as "mail (conflicted copy)"
Compare them, keep the right one, and remove the other.
```

A vault is never a trap. `sefy export` writes everything back out as plain
JSON - which is exactly as sensitive as the vault and protects nothing, so the
command makes you say so out loud:

```
$ sefy export -o backup.json
error: export writes every secret in this vault in the clear
the resulting file protects nothing — encrypt it, or delete it when done
pass --i-know-this-writes-plaintext to go ahead
```

Full command reference: **[lacodda.github.io/sefy](https://lacodda.github.io/sefy/)**.

## Transports

Carrying a vault to another machine is the job of a **plugin**: any executable
named `sefy-plugin-*`, found in sefy's data directory or on `PATH`. sefy knows
nothing about git, FTP or any cloud drive - it asks a plugin what it can do and
what happened.

Two transports ship alongside the CLI, and neither stores a credential of its
own - each authenticates the way the machine already does:

- **github** keeps the vault in a git repository. Point `SEFY_GITHUB_REPO` at
  one; version history comes free with it.
- **sftp** keeps it on a server you control, over OpenSSH. Point
  `SEFY_SFTP_DESTINATION` at `you@server:/path`; nothing lands there but the
  blob.

```
$ sefy plugin list
future  9.0.0     unusable: it speaks protocol 99 and this build speaks 1
github  0.5.0     pull, push
sftp    0.5.0     pull, push
```

With more than one installed, sefy asks which rather than choosing where your
vault goes: `sefy sync --transport sftp`, or `SEFY_TRANSPORT=sftp` once.

A transport is handed the **path of the sealed file** and nothing else - no
master password, no key, no item. What it carries is what anyone would find on
your disk: a blob it cannot read. That is also why a plugin cannot merge: it
moves the other copy to a file, and sefy folds the two together itself, where
both sides can actually be read.

Broken plugins are listed with the reason rather than skipped: an omitted line
would look exactly like a plugin that was never installed.

The protocol is small enough to implement in a shell script - manifest on
`--manifest`, one JSON request on stdin for `run`. Writing one:
[plugin reference](https://lacodda.github.io/sefy/reference/plugin/) ·
[ADR-0002](https://github.com/lacodda/sefy/blob/main/docs/adr/0002-plugin-protocol.md).

## How it works

- The master password is stretched into a key with **Argon2id**.
- The whole SQLite database is sealed with **XChaCha20-Poly1305** (AEAD).
- The file on disk is `salt ‖ nonce ‖ ciphertext`. Salt and nonce are fresh on
  every save, so two saves of identical content share no prefix - and the
  format version lives *inside* the ciphertext, because a version byte in the
  clear would be the signature the format exists to avoid.
- The decrypted database exists **only in memory**. SQLite is never given a
  path, so no page, journal or temporary file lands on disk.
- Saves are atomic: ciphertext goes to a temporary file, is synced, and is
  renamed over the vault. A crash leaves either the old vault or the new one,
  and never plaintext.

Details and rationale: [ADR-0001](https://github.com/lacodda/sefy/blob/main/docs/adr/0001-vault-file-format-and-cryptography.md).

## Install

**One-line installers.** Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/lacodda/sefy/main/tools/install.ps1 | iex
```

macOS / Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/lacodda/sefy/main/tools/install.sh | sh
```

On Windows use the PowerShell line above: `install.sh` carries the macOS and Linux builds only, and run from Git Bash it stops with a pointer back here.

**cargo** - `cargo install sefy`, plus `cargo install sefy-plugin-github` or
`sefy-plugin-sftp` for a transport

**npm** - `npm install -g sefy-cli`

**Binary releases** - grab the archive for your platform from [Releases](https://github.com/lacodda/sefy/releases/latest)
(Windows x86_64, Linux x86_64, macOS arm64), unpack and put `sefy` on your
`PATH`.

Both installers take the newest release by default; set `SEFY_VERSION` to a tag
to pin one, and `SEFY_INSTALL_DIR` to choose where the binary lands. They also
place any transport the archive carries into sefy's plugins directory, so
`sefy plugin list` finds it without a second step.

Shell completions: `sefy completions bash` (also `zsh`, `fish`, `powershell`,
`elvish`).

## Stability

The **vault file format is stable at version 1**. Files written by this release
will stay readable: any future change to the format arrives as version 2, able
to read version 1 and migrate it. The Argon2 parameters are part of that
promise, not a tuning knob.

The database inside the ciphertext is versioned separately, and it does move:
0.2.0 added an identity to items so two copies of a vault can be merged. A vault
from 0.1.x opens, is migrated on the way in, and is still readable by 0.1.x
afterwards - the file on disk did not change shape.

The **plugin protocol is at version 1** as of 0.3.0. 0.4.0 put it to work and
0.5.0 added a second transport of a different shape without changing a field of
it, which is the evidence that it was not built around the first one. Optional
fields may be added to the manifest without breaking a plugin that predates
them; changing what an existing field means would arrive as version 2, with both
accepted for a time.

Released versions and what landed in each: [CHANGELOG on the Releases page](https://github.com/lacodda/sefy/releases).

## Building

```
cargo build --release   # workspace: sefy-core (library) + sefy (CLI)
cargo test              # unit, integration and doc tests
```

Use a release build for daily work: Argon2id is deliberately expensive, and an
unoptimized build makes it several times slower still.

The library is published separately as [`sefy-core`](https://crates.io/crates/sefy-core)
if you want vaults from your own code. The documentation site lives in
[`docs/`](https://github.com/lacodda/sefy/tree/main/docs); architecture decision
records are in [`docs/adr/`](https://github.com/lacodda/sefy/tree/main/docs/adr).

## License

[MIT](https://github.com/lacodda/sefy/blob/main/LICENSE)
