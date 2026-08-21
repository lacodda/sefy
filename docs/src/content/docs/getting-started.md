---
title: Getting Started
description: Install sefy, create a vault and put your first secrets in it.
---

## Install

One line on Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/lacodda/sefy/main/tools/install.ps1 | iex
```

One line on macOS / Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/lacodda/sefy/main/tools/install.sh | sh
```

Via npm:

```sh
npm install -g sefy-cli
```

Via cargo:

```sh
cargo install sefy
```

Or take a prebuilt archive from [Releases](https://github.com/lacodda/sefy/releases/latest)
(Windows x86_64, Linux x86_64, macOS arm64), unpack it and put `sefy` on your
`PATH`.

Both one-line installers also place any transport the archive carries into
sefy's plugins directory, so [`sefy sync`](/sefy/reference/sync/) has something
to call. Installing through cargo or npm gets the CLI alone; add the git
transport with `cargo install sefy-plugin-github` and see
[Syncing through a transport](/sefy/guides/syncing/).

:::caution[On Windows, use the PowerShell line]
`install.sh` carries the macOS and Linux builds only. Running it from Git Bash,
MSYS2 or Cygwin stops with a pointer to `install.ps1` rather than installing
anything.
:::

Shell completions:

```sh
sefy completions bash > /etc/bash_completion.d/sefy
sefy completions zsh  > ~/.zfunc/_sefy
```

`fish`, `powershell` and `elvish` work the same way.

## Point sefy at a file

sefy has **no default vault location**. A file at a predictable path would undo
the point of one that looks like nothing, so you say where it lives:

```sh
export SEFY_VAULT=~/backups/notes.bak
```

Every command also takes `--vault <FILE>` if you would rather be explicit, or
keep several vaults.

The name is yours. `notes.bak`, `archive-2019.dat`, anything at all — there is
no extension sefy expects and none it writes.

## Create the vault

```console
$ sefy init
New master password:
Repeat it:
created /home/you/backups/notes.bak
```

The password is asked for twice, because a typo here would lock you out of the
vault forever. It is never echoed, and it cannot be passed as an argument: that
would put it in your shell history and in every process listing.

## Put things in

```console
$ sefy add note "bank card" --text "PIN 4815" --tag money
added "bank card" as 1

$ sefy add credential mail --login someone@example.com --url https://mail.example.com --tag mail
Password for this item:
added "mail" as 2

$ sefy add file ~/.ssh/id_ed25519 --tag keys
added "id_ed25519" as 3
```

A long note is easier in your editor:

```sh
sefy add note "journal" --editor
```

## Get things out

```console
$ sefy ls
3  id_ed25519  file        [keys]
2  mail        credential  [mail]
1  bank card   note        [money]

$ sefy get mail
copied password of "mail" to the clipboard; clearing in 45s
clipboard cleared
```

For pipes and scripts, print it instead — knowing that the secret then lives in
your terminal scrollback:

```console
$ sefy get "bank card" --stdout
PIN 4815
```

Files come back exactly as they went in:

```sh
sefy extract id_ed25519 -o ~/.ssh/id_ed25519
```

## Naming what you want

Commands take an item's **title**, an **id**, or **text to search for**. An
exact title always wins; when several items still match, sefy shows them rather
than picking one:

```console
$ sefy get ma
error: 2 items match "ma":
     4  mailing list                    note
     2  mail                            credential
narrow the text, or use an id
```

## In scripts

Passwords come from environment variables you name yourself:

```sh
export VAULT_PW='…'
sefy --password-env VAULT_PW ls
```

Without a terminal, sefy refuses to prompt rather than hanging — and `sefy rm`
refuses to assume "yes" unless you pass `--yes`.

## Where next

- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — copying it,
  syncing services, and what to do when two copies drifted apart.
- [Keeping ssh keys in a vault](/sefy/guides/ssh-keys/) — keys and passphrases
  together, and putting them back on a new machine.
- [Commands](/sefy/reference/commands/) — every command, one page each.
- [How the vault works](/sefy/concepts/vault-format/) — the file format and the
  cryptography behind it.
- [Threat model](/sefy/concepts/threat-model/) — what sefy protects against,
  and what it does not.
