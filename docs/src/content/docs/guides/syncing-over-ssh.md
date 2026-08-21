---
title: Syncing to your own server
description: Keep a vault on a machine you control, over SSH, with the sftp transport.
---

`sefy-plugin-sftp` keeps the vault on a server you already have — a home
machine, a VPS, a NAS. It carries the sealed file with OpenSSH and stores it as
one plain file in a directory of your choosing.

Compared with [the git transport](/sefy/guides/syncing/): no repository, no
history, nothing on the server but the blob. That last part is the trade — see
[What you give up](#what-you-give-up) below.

## What you need

- **OpenSSH**, and a server you can already reach without typing a password.
  Windows 10 and 11 ship the client; on Linux and macOS it is the `openssh`
  client package. The transport runs `ssh` and `scp` and nothing else, so a key,
  an agent or an entry in `~/.ssh/config` is what it uses.
- **A directory on the server.** Any directory you can write to.

If `ssh you@server true` works from your shell, the transport works.

## Setting it up

```sh
export SEFY_SFTP_DESTINATION=you@server.example:/home/you/backups
```

The same shape scp takes: `[user@]host:/path`. The username may be left out when
`~/.ssh/config` supplies it.

```console
$ sefy plugin list
github  0.5.0     pull, push
sftp    0.5.0     pull, push

$ sefy sync --transport sftp
Master password:
synced "vault" through sftp
fetched "vault" (52.1 KiB)
pushed "vault" (52.1 KiB)
merged: 1 added, 0 updated, 12 unchanged
```

With two transports installed sefy will not choose between them — say which with
`--transport`, or set `SEFY_TRANSPORT=sftp` once per shell.

## What the server ends up holding

One file per remote name, in the directory you named:

```console
$ ls -l ~/backups
-rw-r--r-- 1 you you 53305 Aug 21 17:51 vault

$ file ~/backups/vault
/home/you/backups/vault: data
```

No extension, no header, nothing that says what it is — the same file the format
describes, sitting in an ordinary directory.

## Interrupted uploads

The bytes land under `<name>.incoming` and are moved into place with `mv` once
the transfer is complete. scp writes straight into its destination, so uploading
over the live file would leave a truncated blob if the connection dropped — and
a truncated vault does not open at all. If a push fails, the staging file is
cleared and the copy already on the server is untouched.

## What you give up

**There is no earlier version.** A push replaces what was there, and the
previous contents are gone from the server. The git transport gets version
history for free; here there is none.

That matters in one specific way. `sync` merges rather than overwrites, so a
push after a pull cannot lose what another machine wrote. What it cannot save
you from is a mistake made locally and then published — `sefy rm` on the wrong
item, followed by a sync. With git you could recover the file from the
repository's history; here the copy on the server has already been replaced.

If that worries you, keep an occasional dated copy of the vault file, which is a
complete backup on its own:

```sh
cp ~/backups/notes.bak ~/backups/notes.bak.2026-08-21
```

## Names the server will not take

The remote name becomes a file name on the far side, and the transport runs `mv`
and `test` through the server's shell. Names that would need quoting are refused
before anything runs:

```console
$ sefy push --transport sftp --name "my vault"
error: plugin sftp failed: "my vault" cannot be used as a file name on the server
pick a remote name without a slash or a leading dot: sefy push --name <NAME>
```

Letters, digits, dashes and underscores work. The default, `vault`, is fine.

## When something goes wrong

| Message | What it means |
| --- | --- |
| `SEFY_SFTP_DESTINATION is not set` | Nowhere to put the vault. Set it to `host:/path`. |
| `…which looks like a Windows path rather than a server` | `C:\vaults` splits into host `C`; the destination is a *remote* path. |
| `…which carries no directory` | The destination needs `host:/path`, not just `host`. |
| `the server holds no copy called "vault" yet` | Nothing has been pushed under that name. Push from a machine that has the vault first. |
| `Permission denied (publickey)` | ssh could not authenticate. The transport never prompts — it would hang, having no terminal — so fix the key or agent and try again. |
| `ssh is not installed, or not on PATH` | This transport carries the vault with OpenSSH, so it needs one. |

## Related

- [Syncing through a transport](/sefy/guides/syncing/) — the git transport, and what every transport can and cannot see
- [`sync`](/sefy/reference/sync/), [`push`](/sefy/reference/push/), [`pull`](/sefy/reference/pull/)
- [`plugin`](/sefy/reference/plugin/) — what is installed, and how to write one
