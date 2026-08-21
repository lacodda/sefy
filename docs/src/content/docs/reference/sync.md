---
title: "sync"
description: Pull, then push — take what is at the remote, then publish the result.
---

The everyday gesture on a machine that is one of several: bring back what the
other machines published, fold it in, and send the combined vault back up.

## Usage

```sh
sefy sync [OPTIONS]
```

Takes the same options as [`pull`](/sefy/reference/pull/):

| Option | Meaning |
| --- | --- |
| `-p, --transport <NAME>` | Which transport to use; omit when only one is installed. |
| `--name <NAME>` | What the remote copy is called. Default `vault`. |
| `--remote-password-env <VAR>` | Read the remote copy's password from this variable. |
| `--ask-remote-password` | Ask for the remote copy's password instead of reusing this vault's. |

```console
$ sefy sync
Master password:
synced "vault" through github
downloaded 12.4 KiB
uploaded 12.6 KiB
merged: 2 added, 0 updated, 14 unchanged
```

## Why pull comes first

Not a preference. Pushing first would replace the remote copy with one that
never saw its contents — every secret added on another machine would vanish from
the only copy that had it. Pulling first means the file that goes up already
holds both sides.

## The push always runs

Even when the pull brought nothing new. Whether the local file differs from the
remote one is not knowable from here — the format carries no counter in the
clear, by design — so a sync that changed nothing costs one upload rather than a
guess about whether it was needed.

## Conflicts

A sync merges, so it reports conflicts exactly as
[`pull`](/sefy/reference/pull/) and [`merge`](/sefy/reference/merge/) do: both
versions are kept, and you decide. Note that the conflicted copy is published
along with everything else, so the other machines will see it too — resolve it
on one machine and sync again.

## Related

- [`pull`](/sefy/reference/pull/) — just the first half
- [`push`](/sefy/reference/push/) — just the second half
- [`merge`](/sefy/reference/merge/) — folding in a local file instead
- [Moving a vault between machines](/sefy/guides/moving-a-vault/)
