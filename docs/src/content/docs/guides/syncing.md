---
title: Syncing through a transport
description: Keep a vault on several machines with the git transport, and understand what each side sees.
---

A **transport** carries the vault file between this machine and somewhere else.
It is an ordinary executable named `sefy-plugin-*`, and sefy hands it the sealed
file — never the password, never an item.

This guide sets up `sefy-plugin-github`, which keeps the vault in a git
repository. The other transport that ships with sefy puts it on a server over
SSH — see [Syncing to your own server](/sefy/guides/syncing-over-ssh/).

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
github  0.7.0     pull, push
sftp    0.7.0     pull, push
```

With two installed, sefy will not choose between them: name one with
`--transport github`, or set `SEFY_TRANSPORT` once per shell.

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
and a history of commits all saying `update`, authored by `sefy`. Both are fixed
on purpose: a subject naming what changed, or your real name and address on
every commit, would say more about the owner than the blobs do. It also means
the transport works on a machine where git has no identity configured, instead
of failing with git's "tell me who you are".

`--name` decides which file this machine writes:

```sh
sefy push --name work-laptop
```

Two machines sharing one name share one file, which is the ordinary setup — they
are copies of one vault, and `sync` merges rather than overwrites. Use distinct
names when you want distinct copies at the remote.

## What happens when both machines changed

The same thing that happens with [`merge`](/sefy/reference/merge/), because it
*is* merge: items missing on one side are copied across, contents one machine
moved on from are brought up to date, and an item changed on both sides keeps
the copy changed more recently — with the other one in its
[history](/sefy/reference/history/), marked with the machine it came from.

```console
1 item changed on both sides.
The copy changed more recently is current; the other is kept in the item's history:
  "bank" (the other copy's is current): sefy history 4
Compare with sefy history ID VERSION; bring one back with sefy restore ID VERSION.
```

The history travels with the vault, so the other machines receive the settled
item and the version that lost with it: a conflict is settled once, not once
per machine. To choose the other version after all,
[`restore`](/sefy/reference/restore/) it and sync again.

Nothing is ever deleted by a sync. "Removed over there" and "added over here"
are indistinguishable from this side, so a removal does not propagate — remove
an item on both machines, or a later sync brings it back from the copy that
still has it.

## Looking before syncing

`--dry-run` fetches the remote copy, works the merge out in memory, and names
every item it would touch — on both sides:

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

It is the merge itself, run on a copy nothing writes, so it cannot disagree
with what the real sync then does. Nothing is written, nothing is sent, and no
transfer is recorded. [`pull`](/sefy/reference/pull/) and
[`merge`](/sefy/reference/merge/) take `--dry-run` too.

It is also how to see a removal about to come back: an item removed here and
still present over there shows up as `add`.

## Syncing after every change

On a machine that is one of several, the sync after a change is easy to forget,
and the other machine finds out at the worst moment. Turn it on once:

```sh
export SEFY_AUTO_SYNC=on
```

From then on, every command that changes the vault — `add`, `edit`, `rm`,
`restore`, `import`, `merge`, `otp --set`, `gen --save`, `change-password` —
ends with what `sefy sync` would do here: the same transport, the same remote
name.

```console
$ sefy add login github --login ada
Master password:
Password for this item:
added "github" as 12
synced "vault" through github
```

`--auto-sync on` and `--auto-sync off` say the same for one command, and win
over the variable — `--auto-sync off` is how a script adding fifty items skips
fifty syncs and runs one at the end.

Three things to know:

- **The change comes first.** It is on disk before the sync starts. A sync that
  fails — no network, a remote under another password — prints a warning and
  the command still succeeds: the change is made, and the next sync carries it.
  A failure status would invite a script to make it twice.
- **It speaks on stderr.** `sefy gen --save --stdout | ...` still hands the pipe
  the password and nothing else.
- **A password change goes up too.** The remote copy is opened with the old
  password and replaced with one under the new password. The other machines then
  need the new password to pull — the same as after any password change.

Only commands that wrote something sync; `ls`, `get` and the rest never touch
the network. `push`, `pull` and `sync` are transfers already and are not
followed by another one.

There is no configuration file to keep the setting in — sefy keeps nothing on
disk but the vault, its copies and its plugins — so, like `SEFY_VAULT` and
`SEFY_TRANSPORT`, it lives in the environment.

## A copy before every merge

A merge is built to lose nothing: what it replaces goes to the item's history.
But that is a property of code, and code has bugs. So before a merge, a pull, a
sync or an import changes the vault, the file as it was is kept beside it:

```
notes.bak
notes.bak.1    before the most recent merge
notes.bak.2
notes.bak.3    the oldest; the next copy pushes it out
```

Three, rotated the way logrotate numbers files. A merge that changes nothing
keeps no copy, so syncs that find the two sides in step never push a useful copy
out. A copy that cannot be made stops the merge before it writes anything.

Each copy is the sealed file byte for byte, under the same password, named after
the vault with a number — nothing in the name says sefy, and nothing in it is
readable without the password. [`change-password`](/sefy/reference/change-password/)
re-seals the copies along with the vault, so the old password opens none of
them.

To go back, replace the vault with a copy:

```sh
cp notes.bak.1 notes.bak
```

Or open a copy as it is, to take one thing from it, without touching the vault:

```sh
sefy --vault notes.bak.1 get bank --stdout
```

A copy holds what the vault held then — including items removed since. They
rotate out after three more merges; delete them by hand when you want them gone
sooner.

[`status`](/sefy/reference/status/) says how many copies there are and how old
the newest is; [`doctor`](/sefy/reference/doctor/) also checks that each one
opens.

## What the transport can and cannot see

It is handed a path and a name:

```json
{ "operation": "push", "file": "/tmp/.sefy-a1b2c3/blob", "name": "vault" }
```

No master password, no derived key, no item. It carries exactly what an onlooker
would find on disk. That is why it cannot merge, and why sefy does the folding
itself with both sides open.

On a pull, the fetched copy lands in a scratch directory that is removed as soon
as it has been read — on the failure path as well as the successful one.

## When something goes wrong

[`sefy doctor`](/sefy/reference/doctor/) checks the whole chain at once — the
vault, the plugins, the transport reaching the remote copy and that copy
opening — without changing or sending anything.

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
- [`doctor`](/sefy/reference/doctor/) — check the whole chain on this machine
- [`merge`](/sefy/reference/merge/) — the rules a pull applies
- [Syncing to your own server](/sefy/guides/syncing-over-ssh/) — the same thing over SSH
- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — doing it by hand
