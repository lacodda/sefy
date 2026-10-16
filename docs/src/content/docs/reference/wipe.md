---
title: "wipe"
description: Destroy the vault - overwrite it and every copy of it beside it, and delete them.
---

Destroys the vault: the file, the copies kept beside it and any save a crash
left unfinished. Each is overwritten in place with random bytes, synced to the
disk and deleted.

## Usage

```sh
sefy wipe
```

It has no options of its own. It asks for the master password, lists every
file it will destroy, and goes ahead only once the vault's file name is typed
back:

```console
$ sefy wipe
Master password:
This overwrites with random bytes and deletes, in /home/me/backups:
  notes.bak           the vault           84.1 KB
  notes.bak.1         copy 1              84.1 KB
  notes.bak.3         copy 3              84.1 KB
  notes.bak.sefy-tmp  an unfinished save  84.1 KB
Not touched: the remote copy, last reached through github 2026-10-12 08:40 UTC (4 days ago).
An SSD, a snapshot, a backup or a synced folder's history can keep
older blocks of these files out of a wipe's reach: see sefy wipe --help.
Nothing brings it back. Type the vault's name to wipe it (notes.bak): notes.bak
wiped  notes.bak
wiped  notes.bak.1
wiped  notes.bak.3
wiped  notes.bak.sefy-tmp
the remote copy is not touched: delete it through github
```

Enter on its own changes nothing:

```console
Nothing brings it back. Type the vault's name to wipe it (notes.bak):
nothing was wiped
```

Anything else that is not the name exactly - `y`, `yes`, `Notes.bak` - is a
refusal as well, and ends with an error so that it is never mistaken for a
wipe that happened:

```console
error: that is not "notes.bak"; nothing was wiped
```

## Why the password, and why the name

The password is what says the file is a vault, and this one. A vault has no
header, so to anything without the password it is a file like any other - and
a wipe aimed at the wrong path by a typo in `SEFY_VAULT` would be just as
final. A wrong password stops the wipe before a single file is touched.

The name is what says you are looking at what you are about to destroy. A `y`
is a reflex, and so is `--yes`; typing `notes.bak` is not.

## No `--yes`, no script

Every other question sefy asks has a `--yes` for scripts. This one has none,
and without a terminal `wipe` refuses before it asks for the password - even
when the name arrives through a pipe:

```console
$ echo notes.bak | sefy wipe
error: wipe asks for the vault's name on a terminal, and input is not one
there is no --yes: what a wipe destroys does not come back
```

A wipe in a script runs again the next time the script does, and a line in a
shell history is one key away from being repeated. `--password-env` still
works for the password; the name is always typed.

## What is destroyed

Every file sefy writes beside a vault, and nothing else:

| File | What it is |
| --- | --- |
| `FILE` | the vault |
| `FILE.1`, `FILE.2`, `FILE.3` | the [copies](/sefy/guides/syncing/#a-copy-before-every-merge) kept before a merge, a pull, a sync or an import |
| `FILE.sefy-tmp`, `FILE.1.sefy-tmp` ... | a save that crashed before it was put in place |

A copy goes whether or not the current password opens it: a copy under a
retired password is exactly what a wipe must not leave behind. Files that only
look related - `FILE.4`, `FILE.old` - are not sefy's and are left alone.

Each file is overwritten once, with random bytes rather than zeros: what is
left where the vault was looks like what was there, not like a wipe. One pass
is enough - on drives built this century a second one finds nothing the first
left.

## What a wipe cannot reach

The overwrite reaches the blocks the file system shows for each file, and no
others. Older blocks of a vault can outlive a wipe where no program reaches:

- **An SSD** writes a changed block to a fresh place and keeps the old one
  until it needs the room. The drive decides when, not the file system.
- **A copy-on-write file system** - APFS, btrfs, ZFS, ReFS - does the same a
  level up.
- **Earlier saves.** Every save writes the vault anew and hands the old file's
  blocks back to the file system, long before any wipe.
- **Snapshots and backups** - Time Machine, File History, Volume Shadow
  Copies, a backup of the folder.
- **A synced folder.** OneDrive, Dropbox, Google Drive and the like keep
  earlier versions of a file, and a recycle bin of their own.
- **The remote copy** a transport holds, and every other machine the vault was
  synced to. `wipe` names the transport the vault last went through; delete the
  copy there yourself. A git remote keeps every version it was given in its
  history.

What survives in any of these is the sealed vault: it still needs the
password, or an earlier one for a save from before the password changed. A
wipe is the end of the vault on this disk as far as software can see, not a
guarantee against a lab. If the password itself may be in the wrong hands, a wipe cannot
take back what an old block still opens to.

## When it does not finish

A file that cannot be written - read-only, or held open by another program -
is not deleted: it stays as it was, so a second attempt can still overwrite
it. `wipe` carries on with the rest, names what it could not destroy, and ends
with status `1`:

```console
wiped  notes.bak
wiped  notes.bak.2
not wiped: cannot open /home/me/backups/notes.bak.1 to overwrite it: Permission denied (os error 13)
error: 1 file still on disk; once a file can be written, wipe it on its own:
sefy wipe --vault FILE
```

The vault itself may be gone by then. Pointed at a copy, `wipe` destroys it as
a vault of its own, under whichever password opens it.

## Related

- [Threat model](/sefy/concepts/threat-model/) - what sefy promises and what it does not
- [`change-password`](/sefy/reference/change-password/) - retire a password without destroying the vault
- [Syncing](/sefy/guides/syncing/) - the copies beside the vault, and the remote one
- [`rm`](/sefy/reference/rm/) - remove one item instead
