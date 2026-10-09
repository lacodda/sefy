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
| `--dry-run` | Say what each side would gain, item by item, and change nothing. |

```console
$ sefy sync
Master password:
synced "vault" through github
downloaded 12.4 KiB
uploaded 12.6 KiB
merged: 2 added, 0 updated, 14 unchanged
the vault as it was is kept as /home/you/Documents/.notes.db.1
```

## Looking first

```console
$ sefy sync --dry-run
here, from "vault" through github:
  add       "their note"
  conflict  "bank" (4) keeps its contents; the other side's go to its history
  14 unchanged
there, once the result is pushed:
  add       "my note"
  update    "bank"
  15 unchanged
dry run: nothing was written here or sent anywhere
```

Both legs, before either happens: what the pull would fold in here, and what the
remote copy would gain from the push that follows. The merge runs for real, on
copies held in memory, so the preview is what the sync then does. Nothing is
written, nothing is sent, and no transfer is recorded.

## Without being asked

With `SEFY_AUTO_SYNC=on`, or `--auto-sync on`, every command that changes the
vault ends with a sync — the same one this command runs with no options. See
[Syncing after every change](/sefy/guides/syncing/#syncing-after-every-change).

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

## A copy before the merge

When the pull leg changes anything, the vault as it was is kept beside it first,
as `FILE.1`; the three most recent are kept. See
[A copy before every merge](/sefy/guides/syncing/#a-copy-before-every-merge).

## Conflicts

A sync merges, so it reports conflicts exactly as
[`pull`](/sefy/reference/pull/) and [`merge`](/sefy/reference/merge/) do: the
copy changed more recently is current, and the other is kept in the item's
[history](/sefy/reference/history/). What goes up carries that history, so the
other machines receive the settled item and the version that lost — the
conflict is settled once, not once per machine. To choose the other version
after all, [`restore`](/sefy/reference/restore/) it and sync again.

## Related

- [`pull`](/sefy/reference/pull/) — just the first half
- [`push`](/sefy/reference/push/) — just the second half
- [`merge`](/sefy/reference/merge/) — folding in a local file instead
- [`doctor`](/sefy/reference/doctor/) — check that a sync would work, without one
- [Moving a vault between machines](/sefy/guides/moving-a-vault/)
