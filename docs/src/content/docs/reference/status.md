---
title: "status"
description: Show what and where this vault is, without revealing any of it.
---

The question asked after a new machine, a restore, or a week away: is this the
right file, is everything in it, and has it reached the other side lately.

## Usage

```console
$ sefy status
vault    /home/you/Documents/.notes.db
size     72.1 KB
items    5 items  (4 login, 1 note)
tags     3 tags
schema   4
synced   2026-09-16 19:15 UTC (2 hours ago) through github (sync)
plugins  github, sftp
```

| Line | What it says |
| --- | --- |
| `vault` | The file being worked on, after `--vault` and `$SEFY_VAULT` are resolved. |
| `size` | The sealed file on disk. |
| `items` | How many, and how many of each kind. |
| `tags` | How many distinct tags are in use. |
| `schema` | Version of the database inside the blob — see [Versions](/sefy/concepts/versions/). |
| `synced` | When this vault last reached a remote, through which transport, and by which command. |
| `plugins` | Transports installed on this machine, with any unusable one marked. |

## It never prints contents

Not a title, not a value, not a tag name. A status is often asked with somebody
else in the room, or pasted into a message asking for help — it answers about
the vault, never about what is in it. The tag *count* is here; the tag *names*
are [`tags`](/sefy/reference/tags/).

## The sync time travels with the vault

`synced` is read from inside the sealed file, not from anything beside it.
sefy keeps nothing on disk but the vault and its transports, and a state file
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
