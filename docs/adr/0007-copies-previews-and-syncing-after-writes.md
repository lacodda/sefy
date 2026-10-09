# ADR-0007: Copies before a merge, previews, and syncing after writes

- **Status:** accepted
- **Date:** 2026-10-09

## Context

By 0.13 a vault on several machines moved through four commands that change
many items at once with contents nobody typed here: `merge`, `pull`, `sync` and
`import`. Each is built to lose nothing — what a merge replaces becomes a
version in the item's history (ADR-0005) — but three things were still missing
for living with it every day:

1. a way to see what a sync would do before it did it;
2. a way back if one of these commands, through a bug, left the vault wrong;
3. a sync that happens without being remembered, on the machine where the
   change was made.

And one question kept coming up with no single place to answer it: does
everything sefy needs on this machine actually work?

Two invariants constrain every answer. The decrypted database never touches the
disk (ADR-0001), and nothing sefy writes may mark the vault as what it is — no
header, no fixed location, no telling file beside it (ADR-0002).

## Decision

### A preview is the merge, run on a copy nothing writes

`merge --dry-run`, `pull --dry-run` and `sync --dry-run` do not predict what the
merge would do. They run it — the same `fold` — on a copy of the database held
in memory, with no file behind it, and drop it. A second implementation of the
merge rules as a "planner" would sooner or later disagree with the merge; this
cannot.

A sync previews both legs: what the pull would fold in here, and what the
remote copy would gain from the push — the merged result folded into the
fetched remote copy, also in memory. A push replaces the remote file whole and
the merged result already holds everything the remote had, so what that fold
adds or moves on is exactly what the remote gains.

A vault with no file of its own — the preview copy, or a remote copy opened
from the bytes a transport fetched — cannot be saved: `save` asserts it has a
file. Neither ever leaves the core crate.

### A copy of the file before a change from elsewhere

Before `merge`, `pull`, `sync` or `import` writes a vault it changed, the file
as it was is kept beside it as `FILE.1`, older copies moving to `FILE.2` and
`FILE.3`. Three, numbered the way logrotate numbers rotated files, with no
setting.

- **Beside the vault, not in sefy's data directory.** Copies of the vault at a
  predictable path under `%APPDATA%\sefy` would be exactly the giveaway the
  format avoids. A number after the vault's own name says "an older copy of
  that file" and nothing about sefy.
- **The sealed bytes, as they were.** A copy is ciphertext under the vault's
  password; the plaintext invariant is untouched.
- **Only when something changed.** A merge that changes nothing writes nothing
  and keeps nothing; otherwise syncs that find both sides in step would rotate
  a useful copy out with copies identical to the vault.
- **No copy, no write.** A copy that cannot be made stops the change before the
  vault is written.
- **Re-sealed on a password change.** Copies under a retired password would be
  three files it still opens. `change-password` re-seals each copy that opens
  under the old password, keeping the time it was taken, and names any that do
  not instead of touching them.

The copy lives in the core library — inside `merge` and the import — so every
interface over it, the GUI included, gets it without remembering to.

### Syncing after a write: an environment switch, not a file

`--auto-sync on`, or `SEFY_AUTO_SYNC=on`, makes a command that wrote the vault
end with what `sefy sync` would do here with no options: the same transport,
the same remote name, read from the same `SEFY_TRANSPORT` and
`SEFY_REMOTE_NAME`.

- **Not a configuration file.** sefy has none on purpose: a file naming the
  vault's path and its transport would be a signpost to the vault. The
  environment already carries `SEFY_VAULT` and every transport's settings.
- **Not a setting inside the vault.** It would not travel: a merge folds items,
  not the vault's own metadata, so the setting would differ between copies with
  no rule for which wins. Settings that travel need merge rules of their own,
  which is work for when the GUI has a settings screen.
- **What counts as a write** is counted where it happens: the vault counts its
  own saves, and a command that saved nothing syncs nothing. `push`, `pull` and
  `sync` are transfers already and are not followed by another.
- **The change comes first.** It is on disk before the sync starts. A failed
  sync is a warning on stderr and the command still exits `0`: a failure status
  would invite a script to make the change again.
- **stderr only**, so `gen --save --stdout` still hands a pipe nothing but the
  password.
- **The remote is opened with the password the vault was opened with.** After
  `change-password` that is the old one — which is what the remote copy is
  still sealed under until this sync replaces it.

### `sefy doctor`: a pull into scratch, not a new protocol operation

The transport check fetches the remote copy into a scratch file, opens it with
the vault's password and compares the two both ways. That answers "would a
sync work?" end to end — transport, network, credentials, a copy under that
name, the password — and it changes nothing.

A `check` operation in the plugin protocol was the alternative. It would have
answered less (reachable, not "opens and is in step"), and every transport
would have had to implement it; protocol version 1 stays as it is. A transport
that only pushes is skipped, because the only way to check it would be to
replace the remote copy.

The clipboard is opened and read, never written: a test value would land in
the clipboard history some desktops keep.

## Consequences

- Up to three more files beside the vault, each opening with its password and
  holding what the vault held before a merge — items removed since included.
  The threat model says so; `status` and `doctor` show them.
- A future `wipe` has to cover the copies as well as the vault.
- `MergeReport` names the items it added and updated, which the previews print
  and the GUI will need; the CLI's ordinary merge output stays counts.
- `Vault::change_password` returns what it did to the copies.
- An empty merge no longer rewrites the vault, and an import that added nothing
  no longer saves it.
