---
title: Syncing through a transport
description: Keep a vault on several machines with the git transport, and understand what each side sees.
---

A **transport** carries the vault file between this machine and somewhere else.
It is an ordinary executable named `sefy-plugin-*`, and sefy hands it the sealed
file — never the password, never an item.

This guide sets up the transport that ships with sefy, `sefy-plugin-github`,
which keeps the vault in a git repository.

## What you need

- **git**, installed and able to reach the repository on its own. The transport
  runs `git` and nothing else, so whatever authentication already works —
  an SSH key, a credential helper — is what it uses. No token is stored here.
- **A repository** to keep the vault in. A private one, with at least one commit
  in it so it has a branch. It does not have to be on GitHub: anything `git
  clone` accepts works.

## Installing the transport

Put the executable where sefy looks:

| System | Directory |
| --- | --- |
| Windows | `%APPDATA%\sefy\plugins` |
| macOS | `~/Library/Application Support/sefy/plugins` |
| Linux | `$XDG_DATA_HOME/sefy/plugins`, or `~/.local/share/sefy/plugins` |

Anywhere on `PATH` works too. Check that sefy sees it:

```console
$ sefy plugin list
github  0.4.0     pull, push
```

A plugin that is present but unusable is listed with the reason, because a
broken installation and a missing one call for opposite fixes.

## Pointing it at the repository

```sh
export SEFY_GITHUB_REPO=git@github.com:you/vault.git
```

That is the whole configuration. The transport keeps a working copy of the
repository in its own data directory, cloned on first use and brought up to date
on every call — deliberately not beside the vault, where a directory would
annotate a file that otherwise gives nothing away.

## Everyday use

```console
$ sefy sync
Master password:
synced "vault" through github
fetched "vault" (52.1 KiB)
pushed "vault" (52.1 KiB)
merged: 2 added, 0 updated, 14 unchanged
```

`sync` pulls first, folds what came back into this vault, and publishes the
result. [`push`](/sefy/reference/push/) and [`pull`](/sefy/reference/pull/) are
the halves, for when you want only one of them.

Set it up the same way on the second machine and run `sefy sync` there. The
first sync brings everything across; after that each one carries whatever
changed since.

## What the repository ends up holding

One file per machine name, with no extension and no directory of its own:

```
vault
work-laptop
```

The contents are the sealed blob — the same headerless file the format
describes. Anyone with access to the repository sees files of high-entropy bytes
and a history of commits all saying `update`. The commit message is fixed on
purpose: a subject naming what changed would annotate the file the format spends
its effort keeping anonymous.

`--name` decides which file this machine writes:

```sh
sefy push --name work-laptop
```

Two machines sharing one name share one file, which is the ordinary setup — they
are copies of one vault, and `sync` merges rather than overwrites. Use distinct
names when you want distinct copies at the remote.

## What happens when both machines changed

The same thing that happens with [`merge`](/sefy/reference/merge/), because it
*is* merge: items missing on one side are copied across, newer contents replace
older ones, and an item changed on both sides is kept twice.

```console
1 item changed on both sides and could not be resolved here.
This vault's version was kept; the incoming one is beside it:
  "bank" → also kept as "bank (conflicted copy)"
Compare them, keep the right one, and remove the other.
```

Note that a sync publishes the conflicted copy along with everything else, so
the other machines will see it too. Resolve it on one machine and sync again.

Nothing is ever deleted by a sync. "Removed over there" and "added over here"
are indistinguishable from this side, so a removal does not propagate — remove
an item on both machines, or a later sync brings it back from the copy that
still has it.

## What the transport can and cannot see

It is handed a path and a name:

```json
{ "operation": "push", "file": "/tmp/.sefy-a1b2c3/blob", "name": "vault" }
```

No master password, no derived key, no item. It carries exactly what an onlooker
would find on disk. That is why it cannot merge, and why sefy does the folding
itself with both sides open.

On a pull, the fetched copy lands in a scratch directory that is removed as soon
as the merge is done — on the failure path as well as the successful one.

## When something goes wrong

| Message | What it means |
| --- | --- |
| `no usable transport installed` | Nothing named `sefy-plugin-*` was found. `sefy plugin list --paths` shows where sefy looked. |
| `several transports are installed` | Say which with `--transport <NAME>`; sefy will not choose where your vault goes. |
| `SEFY_GITHUB_REPO is not set` | The transport has no repository to use. |
| `the repository holds no copy called "vault" yet` | Nothing has been pushed under that name. Push from a machine that has the vault first. |
| `git is not installed, or not on PATH` | This transport carries the vault with git, so it needs one. |
| `it reported success but wrote no file` | The transport claimed to pull and produced nothing. sefy says so rather than blaming your password. |

## Writing another transport

The protocol is two commands and a JSON object, small enough to implement in a
shell script. See [`plugin`](/sefy/reference/plugin/) for the full contract.

## Related

- [`sync`](/sefy/reference/sync/), [`push`](/sefy/reference/push/), [`pull`](/sefy/reference/pull/)
- [`merge`](/sefy/reference/merge/) — the rules a pull applies
- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — doing it by hand
