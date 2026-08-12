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

sefy has **no locking and no merge**. The vault is loaded into memory, changed,
and written back whole. If two machines both load the vault and both save, the
second save wins completely and the first machine's change is gone.

There is no conflict detection to warn you, because there is nothing in the
clear for a second process to inspect — checking would mean decrypting, and a
file that announced "vault modified at 14:02" would be carrying exactly the kind
of metadata this format refuses to write.

So the working rule is one machine at a time:

1. Finish what you are doing and let the file sync.
2. Only then edit it elsewhere.

If that discipline is not realistic for you, keep one vault per machine and move
individual items across with `export`/`import` as below.

## Merging two vaults that drifted apart

When both copies changed, put one into the other:

```console
$ sefy --vault ./laptop.bak export --i-know-this-writes-plaintext -o transfer.json
wrote transfer.json in the clear

$ sefy --vault ./desktop.bak import transfer.json
imported 1 item
```

The intermediate file holds **every secret in the clear**. Write it somewhere
that is not synced or backed up, move it in one step, and delete it as soon as
the import succeeds. On a shared machine, prefer a pipe so it never reaches
disk at all:

```sh
sefy --vault ./laptop.bak export --i-know-this-writes-plaintext \
  | sefy --vault ./desktop.bak import
```

Import **appends and never merges**: items that already exist in the destination
come out as duplicates rather than overwriting anything. That is deliberate —
silently replacing a secret you still needed is a worse outcome than a duplicate
you can see and remove with `sefy rm`.

For that reason a full export/import of a whole vault is a poor way to sync
repeatedly. Trim `transfer.json` to the items that are actually new before
importing; the format is plain enough to edit:

```json
{
  "sefy_export": 1,
  "items": [
    { "title": "bank", "kind": "note", "tags": ["money"], "text": "code 4815" }
  ]
}
```

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
