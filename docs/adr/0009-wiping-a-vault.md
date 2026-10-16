# ADR-0009: Wiping a vault

- **Status:** accepted
- **Date:** 2026-10-16

## Context

By 0.15 a vault could be made, filled, synced, audited and moved elsewhere, but
not ended. Deleting the file by hand leaves its bytes in the blocks it used, and
leaves the copies beside it (ADR-0007) — three more files under the same
password, or under a password since retired. The temporary file of a save that
crashed is a fourth.

The question asked was narrow: when a vault must not outlive the next few
minutes, what can a program honestly do, and what must it say it cannot?

Three things constrain the answer. A vault has no header (ADR-0001), so
nothing but the password tells it from any other file. A wipe cannot be undone,
so its confirmation has to be one a reflex cannot give. And sefy promises
"inconspicuous, not deniable": it must not claim to defeat forensics.

## Decision

### A wipe needs the password: the vault opens first

`Vault::wipe(self)` exists only on an open vault, and consumes it. Opening is
the proof that the path holds a vault and that whoever asks holds its password.
Without it, `sefy wipe` would be a shredder for whatever path `SEFY_VAULT` held —
and a typo there would be as final as a correct path. The password is not a
defence against an attacker, who can delete the file anyway; it is a defence
against the wrong file.

Consuming the vault leaves nothing in memory that a later `save` could write
back. A vault with no file of its own — a preview's scratch copy carries the
real path — refuses to wipe by assertion, as it refuses to save.

### The confirmation is the vault's name, typed, on a terminal

The command lists every file it will destroy and asks for the vault's file
name, the way a hosting service asks for a repository's name before deleting
it. Enter alone declines; anything else that is not the name exactly is an
error. There is no `--yes`, and without a terminal the command refuses before
the password is asked for, even if the name arrives on stdin.

A wipe in a script runs again each time the script does, and a line in a shell
history is one key from being repeated. `rm` keeps its `--yes` because one item
is recoverable from a copy; a wipe takes the copies too.

### What is destroyed: the names sefy writes beside a vault

`wipe::parts` lists the vault, `FILE.1`–`FILE.3`, and the `.sefy-tmp` file of
each. A copy is destroyed whether or not the current password opens it: these
names are sefy's (rotation already overwrites them without looking inside), and
a copy under a retired password is exactly what an emergency must not leave.
Nothing else beside the vault is touched — `FILE.4` and `FILE.old` are not
sefy's.

### How: random bytes, in place, once, synced, then delete

Each file is opened for writing **without truncation**, overwritten with
random bytes chunk by chunk, synced, and deleted. Truncating first — what
`File::create` does — hands the blocks back to the file system and lets the
new bytes land wherever it pleases. The draft `--editor` writes went through
exactly that before this release; it now uses the same `wipe::destroy`.

Random rather than zeros: the overwritten blocks look like the ciphertext they
replace, rather than announcing a wipe. One pass: NIST SP 800-88 finds a
second pass adds nothing on current drives.

### Carry on past a failure; never delete what was not overwritten

A file that cannot be overwritten (read-only, held open elsewhere) is reported
and left as it was — deleting it would turn a recoverable state into one where
the bytes are still on disk but the name to reach them is gone. The wipe goes
on with the remaining files: in an emergency, stopping at the first failure
would leave more behind. The command exits `1` and says how to finish:
`sefy wipe --vault FILE` wipes one remaining file as a vault of its own.

### The remote copy is named, not touched

A transport's copy, and other machines' copies, are out of reach: deleting at
a git remote is not wiping (its history keeps every version), and the plugin
protocol has no delete. The wipe reads the vault's last-sync stamp before
destroying it and says, before and after, which transport holds a copy.

## Consequences

- `sefy wipe` and `sefy_core::wipe` (`parts`, `destroy`, `Wiped`).
- The overwrite reaches only the blocks the file system shows. SSD wear
  leveling, copy-on-write file systems, snapshots, backups, synced folders'
  version history and every earlier save (each hands its blocks back) can keep
  old ciphertext. The reference page and the threat model say so; what
  survives still needs the password, or an earlier one.
- A script cannot wipe a vault. A dead man's switch built on sefy would have to
  delete the file itself.
- The vault format and the schema are unchanged.
