# ADR-0013: Writing from the window

- **Status:** accepted
- **Date:** 2026-11-06

## Context

Until now the window only read. It opened a vault, kept the database in memory,
and could go on showing what it had read for as long as it stayed unlocked, while
the command line wrote the same file through the window's own keys (ADR-0012).
The two never collided, because only one of them wrote.

The Sync and Settings screens write: a merge, a restored version, a new password,
a setting. Now there are two writers holding the same file, and one of them holds
a database in memory for hours.

What decides this ADR is what a save does when the file moved on after it was
read. Until now, it overwrote it: the file was sealed from what was in memory, and
whatever was written meanwhile was gone, without a word. Nothing in the format can
warn about it afterwards.

The window is only the newest way to get there. `sefy edit` with a note open in an
editor for twenty minutes, two terminals, a sync client replacing the file: each
already lost a change this way. So the question is not how the window should
behave, but what a save is allowed to do.

## Decision

### A save refuses to write over a file it did not read

`Vault` remembers which version of the file it holds, and `save` compares that with
the file before sealing anything. A file that moved on, or is no longer there, is
left exactly as it is, with `Error::ChangedOnDisk`.

The version is named by **the salt at the front of the file**. ADR-0001 already
samples a new one for every write, so it is exact where a modification time is not,
costs sixteen bytes to read, and adds nothing to the format. No counter in the
clear, no second file.

A file that is gone stays gone: a value still open after a wipe elsewhere must not
bring the vault back.

The comparison and the rename are two steps, so there remains a window of
microseconds between them. Closing it would take a lock, and a lock file beside the
vault says what the vault is, which ADR-0001 exists to prevent. Rejected.

The alternative was to check only in the window. That protects against the one case
being added and leaves every other, and it leaves a correct window to be overwritten
by a stale `sefy edit`. Rejected.

### Whoever waits on a person catches up first

`Vault::reload` reads the file again if it moved on. Refusing is the right default
for a command that took milliseconds: it says nothing was written, and running it
again is free. It is the wrong answer to someone who has just spent twenty minutes
typing, so `sefy edit` catches up after the editor closes and applies the edit on
top of the file as it is. If something else changed the same item meanwhile, that
becomes a version like any other: nothing is lost, and history says what happened.

### The window looks at the file, and catches up before it writes

Every two seconds the window reads the sixteen bytes. When they differ, it reads the
file again and tells the page, which asks for what it shows.

- **Looking is not using.** The read goes through the keeper's `peek`: a file that
  changes in an empty room must not keep the vault unlocked.
- **The stretching happens with the vault free**, so the window stays usable. When
  the command line wrote through the window's keeper, the key is already known and
  no stretching happens at all.
- **A file that will not open is said once and not retried** until it is something
  else. It may be a sync client halfway through, or another password: either way
  the window keeps showing the last version it read, says so, and writes nothing.
  Trying again every two seconds would burn a stretched password each time.

A write from the window reads the file first as well, whatever the two-second look
has seen, and the save holds the line for the moment between.

A filesystem watcher would notice sooner. It would also be a new dependency with a
different failure on each system and on network drives, for a difference a person
cannot see.

### What a vault lets out is set inside the vault

How long a vault stays unlocked and how long a copy stays on the clipboard were
fixed in the window and given by flags on the command line. So two behaviours were
already wanted, and the approved design offers them as settings.

They live in the `meta` table, inside the sealed file. A settings file would be the
first thing sefy keeps on disk beyond the vault, its copies and its plugins, in a
predictable place, saying that sefy is used here. In the vault it needs no schema
change, since an older build does not read keys it does not know, and every front
end obeys one answer: `agent --ttl` and `--clear-after` still mean what they say, and
without them the command line goes by the vault.

A merge folds items, not facts about a vault, so the setting stays with the copy it
was made on. That is a feature: a laptop and a desktop can lock differently.

Values are chosen from a short list (`policy::LOCK_AFTER_CHOICES`,
`CLIPBOARD_CHOICES`). `set_policy` refuses anything else, so a vault that locks at
once or a secret that never leaves the clipboard cannot be set by accident. What is
read back is not held to the list, so a newer sefy may offer more.

### A transfer holds the vault only for the merge

A sync takes as long as the network does. Holding the vault through it would leave
every other request in the window waiting, so `sync_through` fetches, opens the
remote copy, then takes the vault only to merge and stamp, and lets go before
sending. A counter says which opening of the vault this is: a transfer that took a
minute will not fold into another vault unlocked meanwhile.

A result that cannot be sent back up is not a failed sync. What came down is in the
vault, and the outcome says what is left to do.

What a sync did is shown by title while the vault stays unlocked, kept in memory and
dropped at the lock. A conflict is a link to the version that was kept: the merge has
already settled it, with both versions in the item's history (ADR-0005), and the
screen's job is to say so and show the way there.

### The window's dangerous gestures are the command line's

Changing the password and wiping both take the password typed again, not a vault
found unlocked (ADR-0012), and a wipe takes the file's name, character for character,
as `sefy wipe` does (ADR-0009). There is no setting that skips either. Everything is
locked before the first file is overwritten.

### The Kit screen of the design is not a screen

The approved design has a tab of palette, type and controls. It was a reference for
building, not something a person with a vault has any use for, and that reference
now exists as dowel, the line's design system. Shipping it would be a second copy of
the design system inside a secret store.

## Consequences

- A command can now fail with "changed on disk, run it again" where it used to erase
  someone's change. The failure is the point.
- Nothing about the file format, the schema or what older builds can read changes.
  An older build still overwrites without checking; that is what it always did.
- Settings cannot be set from the command line yet, only obeyed and read
  (`sefy status`). A vault used only from a terminal keeps the defaults and the flags.
- The window has no **Configure** for a transport, unlike the design: a transport's
  configuration is its own environment, and the plugin protocol has nothing to say
  about it. Window and command line see the same variables.
