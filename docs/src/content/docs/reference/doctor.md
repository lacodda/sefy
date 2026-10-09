---
title: "doctor"
description: Check that what sefy needs on this machine works, without changing or sending anything.
---

The question after a new machine, a new transport, or a sync that complained:
does everything sefy depends on here actually work? Every check runs, whatever
the ones before it found, so one problem never hides the next.

## Usage

```sh
sefy doctor [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `-p, --transport <NAME>` | Which transport to check; omit when only one is installed. Also `SEFY_TRANSPORT`. |
| `--name <NAME>` | What the remote copy is called. Default `vault`. Also `SEFY_REMOTE_NAME`. |

The transport and the name are chosen exactly as [`sync`](/sefy/reference/sync/)
chooses them, so the answer is about the sync you would run.

```console
$ sefy doctor
Master password:
ok    vault      /home/you/Documents/.notes.db opens: 42 items, schema 5
ok    copies     2 copies beside the vault, newest 2026-10-09 07:49 UTC (3 hours ago)
ok    device     versions written here are marked "laptop"
ok    plugins    github 0.14.0, sftp 0.14.0
ok    transport  github fetched "vault" (84.1 KB); it opens, and a sync would change 2 items here and 1 item there
                 see which with `sefy sync --dry-run`
skip  auto-sync  off; --auto-sync on or SEFY_AUTO_SYNC=on syncs after every change
ok    clipboard  reachable
```

## The checks

| Check | What it does |
| --- | --- |
| `vault` | Opens the vault with its password and counts what is inside. A vault a newer sefy wrote is a warning: update before changing it here. |
| `leftover` | Shown only when a write was cut short and left `FILE.sefy-tmp` beside the vault. It is sealed like the vault, and the next change replaces it. |
| `copies` | The [copies kept before a merge](/sefy/guides/syncing/#a-copy-before-every-merge), and whether each one opens under this vault's password. A copy that would not open is a safety net with a hole in it. |
| `device` | The machine name versions written here carry, which a [conflict](/sefy/reference/history/) uses to say where a version came from. |
| `plugins` | Every transport found, with the reason any of them cannot be used. |
| `transport` | Fetches the remote copy into a scratch file, opens it with this vault's password, and compares the two. |
| `auto-sync` | Whether [syncing after every change](/sefy/guides/syncing/#syncing-after-every-change) is on, and whether the transport it would use works. |
| `clipboard` | Whether the clipboard can be reached. |

Each line starts with `ok`, `warn`, `fail` or `skip`. `skip` means there was
nothing to check and that is not a problem — no vault given, no transport
installed. A line that goes on below says what to do about it.

## Nothing is changed and nothing is sent

The transport check is a **pull into a scratch file**, removed as soon as it has
been read. A pull is the one way to know that a sync would work — that the
transport runs, reaches the remote, finds a copy under that name, and that the
copy opens — without doing a sync. Nothing is folded into the vault and no
transfer is recorded.

A transport that can only push is skipped: the only way to check it would be to
replace the remote copy.

The clipboard is opened and read, never written. A test value would land in the
clipboard history some desktops keep, and a check that leaves traces is not a
check.

## A remote under another password

```console
warn  transport  github fetched "vault" (84.1 KB), which does not open under this vault's password
                 pull and sync need --ask-remote-password; syncing after a write will fail
```

Legitimate — a pull folds in any vault sharing items with this one — but
unusual, and [syncing after every change](/sefy/guides/syncing/#syncing-after-every-change)
cannot work across it: it opens the remote with this vault's own password.

## Exit status

`0` when no check failed, `1` when one did. Warnings and skips do not fail it, so
`sefy doctor` can stand at the end of a setup script.

## Related

- [`status`](/sefy/reference/status/) — what and where the vault is, without checking anything
- [`plugin`](/sefy/reference/plugin/) — transports in full
- [`sync`](/sefy/reference/sync/) — what the transport check stands in for
- [Syncing between machines](/sefy/guides/syncing/)
