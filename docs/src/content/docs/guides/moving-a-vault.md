---
title: Moving a vault between machines
description: Carry a vault to another computer, keep several in sync by hand, and merge two that drifted apart.
---

A vault is one file. Moving it is copying that file — there is no database
directory, no lock file and no configuration that has to travel with it.

Everything below is what people actually run into once they try: syncing
services, two machines editing the same vault, and the question of what to do
when both changed.

## Copying it as it is

The file is self-contained and portable across operating systems. A vault
written on Windows opens on Linux and macOS with the same password and no
conversion step.

```sh
scp ~/backups/notes.bak you@laptop:~/backups/notes.bak
```

Nothing about the file identifies the machine that made it, so nothing has to be
adjusted on the other side. Point sefy at it and carry on:

```sh
export SEFY_VAULT=~/backups/notes.bak
sefy ls
```

The password travels in your head, not in the file. There is no key file to
forget on the old machine.

## Through a syncing service

Because the file has no extension convention and no header, cloud storage treats
it as an opaque blob — which is the point. Drop it in the synced folder and let
the service move it:

```sh
export SEFY_VAULT=~/Dropbox/archive-2019.dat
```

Two things are worth knowing before you rely on this.

**Every save rewrites the whole file.** Salt and nonce are fresh each time, so
the ciphertext changes completely even when you edited one tag. Syncing services
that deduplicate or delta-compress get no benefit here, and every save uploads
the whole vault. For a vault of a few megabytes this is not worth thinking
about; for a vault full of large stored files it is.

**Saves are atomic, but sync is not.** sefy writes to `<vault>.sefy-tmp` and
renames it over the vault, so the file on disk is never half-written. What a
syncing service does with a rename mid-upload is up to the service — do not save
on two machines at the same moment and expect it to work out.

Some services also sync the `.sefy-tmp` file if a write is interrupted. It is
safe to delete; the next save reuses that name anyway.

## Two machines, one vault

sefy has **no locking**. The vault is loaded into memory, changed, and written
back whole. If two machines both load the same file and both save, the second
save wins completely and the first machine's change is gone.

Nothing warns you while it happens, because there is nothing in the clear for a
second process to inspect — checking would mean decrypting, and a file that
announced "vault modified at 14:02" would be carrying exactly the kind of
metadata this format refuses to write.

So the working rule is one machine at a time:

1. Finish what you are doing and let the file sync.
2. Only then edit it elsewhere.

What that rule protects against is one file being **overwritten** by the other —
and that is the one loss nothing can undo. If instead you keep a separate vault
per machine and let them drift on purpose, the two are reconciled afterwards
with [`merge`](/sefy/reference/merge/), below.

## Merging two vaults that drifted apart

When both copies changed, fold one into the other:

```console
$ sefy --vault ./desktop.bak merge ./laptop.bak
Password for ./laptop.bak:
merged: 1 added, 1 updated, 1 unchanged
```

Items are matched on the identity each carries, so this is safe to repeat: a
second merge of the same file reports everything as unchanged rather than
doubling it. The file being merged from is only read.

Where both sides changed the same item, sefy keeps **both** and says so, rather
than letting a timestamp decide which password you get to keep:

```console
1 item changed on both sides and could not be resolved here.
This vault's version was kept; the incoming one is beside it:
  "mail" → also kept as "mail (conflicted copy)"
```

Nothing is deleted by a merge: an item missing from the other copy stays here,
because "removed there" and "added here" are indistinguishable from this side.
[`merge`](/sefy/reference/merge/) has the full rules.

Two things it does not do. It has no idea which copy is "the real one" — merge
runs in whichever vault you point `--vault` at, and only that one changes. And
it cannot repair a copy you have already overwritten: if a sync service replaced
one file with the other, what was in it is gone, and this guide's advice about
one machine at a time is what keeps that from happening.

### The older way, and when it still applies

Before identities existed, moving items between vaults meant export and import
by hand — and that is still the path for a vault written by sefy 0.1.x, whose
items carry no identity to match on, or for moving a *subset* of items rather
than everything:

```sh
sefy --vault ./laptop.bak export --i-know-this-writes-plaintext \
  | sefy --vault ./desktop.bak import
```

Mind what that pipe carries: an export holds **every secret in the clear**.
Piping keeps it off disk; writing it to a file does not, so delete the file as
soon as the import succeeds.

## Backups

Copy the file. That is the whole procedure — a vault is a single file, and any
copy of it is a complete backup.

Old copies stay readable with the password they were written under. If you run
`sefy change-password`, earlier backups still need the **old** password: the
command rewrites the file under a new key with a fresh salt and nonce, and does
not reach into copies you made before.

```console
$ sefy change-password --new-password-env NEW
password changed
```

Keeping a couple of dated copies is worth it for a reason that has nothing to do
with attackers: an authenticated file has no partial recovery. A vault damaged
by a failing disk or a truncated upload does not open at all, and there is no
salvage tool that could read half of it.

```sh
cp ~/backups/notes.bak ~/backups/notes.bak.2026-01-31
```

## What to do about the old copy

Deleting a vault file removes the ciphertext, not the password's reach: anyone
holding an older copy and the password can still read what was in it at the
time. If a machine is being handed on, delete the file *and* change the password
on the copy you keep, so the two are no longer opened by the same secret.

## Having something else carry it

Everything above is done by hand. A **plugin** does the carrying instead — an
executable named `sefy-plugin-*` that moves the sealed file to a git remote, an
FTP server or a cloud drive, without ever seeing what is inside it.

```console
$ sefy plugin list
github  0.1.0     pull, push
```

Moving a vault with one arrives in a later release; `sefy plugin list` is how
you check what is installed today. See the
[`plugin` reference](/sefy/reference/plugin/), including how to write one.
