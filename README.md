<p align="center"><img src="https://github.com/lacodda/sefy/raw/main/assets/banner.svg" alt="sefy - an inconspicuous encrypted vault" width="720"></p>

<p align="center">
  <a href="https://crates.io/crates/sefy"><img src="https://img.shields.io/crates/v/sefy?style=flat-square" alt="crates.io"></a>
  <a href="https://www.npmjs.com/package/sefy-cli"><img src="https://img.shields.io/npm/v/sefy-cli?style=flat-square" alt="npm"></a>
  <a href="https://github.com/lacodda/sefy/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/lacodda/sefy/ci.yml?branch=main&style=flat-square" alt="CI"></a>
  <a href="https://github.com/lacodda/sefy/blob/main/LICENSE"><img src="https://img.shields.io/crates/l/sefy?style=flat-square" alt="MIT"></a>
</p>

> An encrypted store for notes, credentials and files that does not announce itself: one file with no header, no extension it must keep, and no name that gives it away.

Every way of storing secrets announces itself. A `.kdbx` file says "password
database". age and gpg write a header. VeraCrypt wants a container and a mount.
Whoever looks at your disk, your backup drive or your cloud folder can tell
exactly where the interesting file is.

**sefy is a secret store whose file looks like nothing.** Notes, logins, cards,
ssh keys and files live in an encrypted SQLite database sealed into a single
blob with no magic bytes, no header and no extension convention. Call it
`notes.bak`, leave it among your other backups, and there is nothing to
notice.

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

Put things in - notes, logins, cards, ssh keys, and files kept byte for byte:

```
$ sefy add note "bank card" --text "PIN 4815" --tag money
added "bank card" as 1

$ sefy add login mail --login someone@example.com --url https://mail.example.com --tag mail
Password for this item:
added "mail" as 2

$ sefy add file ~/.ssh/id_ed25519 --tag keys
added "id_ed25519" as 3
```

Bare `sefy` opens a picker over everything in the vault:

```
$ sefy
? Item >
> bank card   note   [money]
  mail        login  [mail]
  id_ed25519  file   [keys]
```

Take a secret out. It goes to the clipboard and is taken back off after 45
seconds - and only if the secret is still what is sitting there, so anything
you copied meanwhile is left alone.

```
$ sefy get mail
copied password of "mail" to the clipboard; clearing in 45s
clipboard cleared
```

Items are addressed by title, by an exact id, or by text to search for; when
the words could mean more than one thing, sefy shows what they could mean
rather than guessing. `sefy open` loads the site and puts the password on the
clipboard in one step, `sefy sync` carries the vault through a transport and
folds what comes back into this one, and `sefy status` says what you are
holding without saying what is in it.

Every command, with its flags: **[the reference](https://lacodda.github.io/sefy/reference/commands/)**.

## What you get

- **A file that looks like nothing.** No magic bytes, no header, no extension
  convention - salt and nonce fresh on every save, so two saves of identical
  content share no prefix.
- **Notes, logins, cards, ssh keys and files** in one vault, tagged and
  searchable, with files kept byte for byte.
- **Transports, not lock-in.** `sefy sync` carries the vault through a
  `sefy-plugin-*` executable - github and sftp ship today - and folds what
  comes back in without a credential ever passing through the transport.
- **Merge instead of overwrite.** Where two copies disagree about an item,
  both are kept rather than letting a timestamp pick a winner.
- **A vault that is never a trap.** `export` writes plaintext JSON only after
  you say so explicitly.

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

## Documentation

Full command reference, concepts and guides:
**[lacodda.github.io/sefy](https://lacodda.github.io/sefy/)** - including
[how the vault works](https://lacodda.github.io/sefy/concepts/vault-format/)
and the [plugin reference](https://lacodda.github.io/sefy/reference/plugin/)
for transports.

## Status

The vault file format is stable at version 1, and the plugin protocol at
version 1 since 0.3.0; both promises hold across every release below. The
database schema inside the ciphertext moves between releases, and older
builds keep reading a vault safely even when they cannot see everything in
it - the [versions and compatibility](https://lacodda.github.io/sefy/concepts/versions/)
page has the detail.

Released versions and what landed in each: [CHANGELOG on the Releases page](https://github.com/lacodda/sefy/releases).

## Contributing

Building the workspace, repository layout and commit conventions:
[CONTRIBUTING.md](https://github.com/lacodda/sefy/blob/main/CONTRIBUTING.md).

## License

MIT (c) [Kirill Lakhtachev](https://lacodda.com)
