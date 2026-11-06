---
title: "status"
description: Show what and where this vault is, without revealing any of it.
---

The question asked after a new machine, a restore, or a week away: is this the
right file, is everything in it, has it reached the other side lately, and is
it locked.

It never asks for the password.

## Usage

```console
$ sefy status
vault    /home/you/Documents/.notes.db
size     72.1 KB
lock     unlocked by the agent, locks in 14m
items    5 items  (4 login, 1 note)
tags     3 tags
schema   5
sealed   Argon2id (64 MiB, 3 passes, 4 lanes), XChaCha20-Poly1305, format v1
synced   2026-09-16 19:15 UTC (2 hours ago) through github (sync)
copies   2 copies beside the vault, newest 2026-09-16 19:15 UTC (2 hours ago)
policy   locks after 15m idle, the clipboard clears after 45s
plugins  github, sftp
```

| Line | What it says |
| --- | --- |
| `vault` | The file being worked on, after `--vault` and `$SEFY_VAULT` are resolved. |
| `size` | The sealed file on disk. |
| `lock` | Whether something holds the password for this vault, which one, and how long until it locks if nobody uses it: the [agent](/sefy/reference/agent/) or the [desktop app](/sefy/guides/desktop-app/#the-command-line-while-it-is-unlocked). Otherwise `locked`. |
| `items` | How many, and how many of each kind. |
| `tags` | How many distinct tags are in use. |
| `schema` | Version of the database inside the blob — see [Versions](/sefy/concepts/versions/). |
| `sealed` | How the file is sealed: the key derivation and what it costs, the cipher, the file format — see [Vault format](/sefy/concepts/vault-format/). Read from the numbers the derivation runs on, so it cannot describe one thing while the code does another. |
| `synced` | When this vault last reached a remote, through which transport, and by which command. |
| `copies` | The [copies kept beside the vault](/sefy/guides/syncing/#a-copy-before-every-merge) before a merge or an import changed it, and when the newest was taken. |
| `policy` | What this vault says about how long it stays unlocked and how long a copy stays on the clipboard. Kept inside it, set in the [desktop app](/sefy/guides/desktop-app/#settings), and what [`agent`](/sefy/reference/agent/) and [`get`](/sefy/reference/get/) go by unless told otherwise. |
| `plugins` | Transports installed on this machine, with any unusable one marked. |

## On a locked vault

With nothing holding the password and no `--password-env`, it says what can be
said without one, and exits zero:

```console
$ sefy status
vault    /home/you/Documents/.notes.db
size     72.1 KB
lock     locked
the rest takes the password: `sefy agent` unlocks the vault for a while
```

Whether a vault is locked is often asked on the way to walking away, and an
answer should not take the master password. Asking does not keep the vault
unlocked either: a prompt that shows this line does not start the agent's idle
time over.

## It never prints contents

Not a title, not a value, not a tag name. A status is often asked with somebody
else in the room, or pasted into a message asking for help — it answers about
the vault, never about what is in it. The tag *count* is here; the tag *names*
are [`tags`](/sefy/reference/tags/).

## The sync time travels with the vault

`synced` is read from inside the sealed file, not from anything beside it.
sefy keeps nothing on disk but the vault, its copies and its transports, and a state file
next to a vault would annotate the one file that is deliberately unremarkable.

It also means the answer moves with the file. Copy a vault to another machine
and it still knows when it last synced, because that is a fact about the vault
rather than about the computer holding it.

A vault written before 0.8.0 says `never`, which is the honest answer: nothing
recorded it at the time.

## An unknown kind is still counted

A vault written by a **newer** sefy can hold a kind this build has never heard
of. It is counted and named as it was stored, rather than left out:

```console
$ sefy status
items    3 items  (2 login, 1 passport)
schema   5 (written by a newer sefy)
```

A summary that quietly omitted part of the vault would be worse than no summary
at all — see [Versions](/sefy/concepts/versions/).

## Related

- [`ls`](/sefy/reference/ls/) — what is actually in the vault
- [`tags`](/sefy/reference/tags/) — the tag names behind the count
- [`plugin`](/sefy/reference/plugin/) — transports in full, with why one is unusable
- [`sync`](/sefy/reference/sync/) — what updates the `synced` line
- [`doctor`](/sefy/reference/doctor/) — check that all of it actually works
- [`agent`](/sefy/reference/agent/) — what the `lock` line is about
