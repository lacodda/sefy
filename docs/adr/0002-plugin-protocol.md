# ADR-0002: Plugin protocol for transports

- **Status:** accepted
- **Date:** 2026-08-16

## Context

A vault is one file, and the obvious way to have it on two machines is to let
something carry it between them. sefy needs a way to reach a git remote, an FTP
server or a cloud drive without knowing about any of them, and without those
integrations dragging their SDKs into a binary whose selling point is that it is
one unremarkable executable.

Two constraints shape everything below.

**The plaintext invariant.** The decrypted database exists only inside sefy's
own memory and never reaches the disk. Anything that hands item contents to a
third-party process breaks that invariant just as thoroughly as writing a
temporary file would — the plaintext is simply somewhere else.

**Inconspicuousness.** The vault file carries no header, no extension
convention and no fixed location, because any of those would identify it. That
constraint does not stop at the file: a directory of plugins beside it would
say the same thing the header does not.

## Decision

### A plugin is an executable named `sefy-plugin-*`

Following the line's convention (kilna, kasl): a subprocess, discovered by name,
speaking JSON. Not a dynamic library — Rust has no stable ABI — and not WASM,
because a sandbox that forbids network access forbids the only thing a transport
does.

A plugin can therefore be written in any language, and a shell script is a
legitimate implementation.

### Transports carry the sealed blob, never items

sefy hands a plugin a **path to the encrypted file** and nothing else:

```json
{ "operation": "push", "file": "/tmp/vault.bin", "name": "vault" }
```

No password, no derived key, no item. The alternative — handing over decrypted
items so a plugin could store them in the remote's own format — was rejected
outright: it would put every secret in the vault into a third-party binary's
address space, which is precisely the invariant this product exists to keep.

The cost is real and accepted: a transport cannot merge, because what it carries
is opaque to it. It fetches the other copy to a path sefy chose, and sefy folds
the two together with `merge` (0.2.0), where both sides can be read. This also
means conflict handling stays in one place, under the rule that no secret is
discarded on a timestamp.

`name` exists because a transport needs a handle for the thing it stores, and
the local file name is a poor one: a vault is deliberately named like anything
else, and on a shared remote two machines' `notes.bak` would collide.

### Two operations, declared in the manifest

`push` and `pull`. A plugin lists what it implements, and sefy refuses anything
else *before* running it. A transport that can only publish declares `["push"]`,
and asking it to pull produces sefy's own explanation rather than an error
message written for somebody else's tool.

A plugin declaring no operations is listed as unusable: it cannot do anything,
and saying so is more useful than a line that looks fine until it is called.

### Everything found is listed, working or not

`sefy plugin list` shows broken plugins with the reason — an unreadable
manifest, a protocol mismatch, a refusal to start. Omitting them would make a
broken installation indistinguishable from a missing one, and the two call for
opposite fixes.

`protocol_version` must match exactly. A mismatch is reported with both numbers
rather than guessed at.

### Plugins live in the application's data directory, then `PATH`

`%APPDATA%\sefy\plugins`, `~/Library/Application Support/sefy/plugins` or
`$XDG_DATA_HOME/sefy/plugins`, then every directory on `PATH`; first copy of a
name wins.

Deliberately **not** beside the vault file. sefy has had no config file and no
data directory until now, and this is the first thing it puts on disk beyond the
binary — so it goes where every other application keeps its own things, not
where it would annotate a file that gives nothing away.

The path is resolved from environment variables directly rather than through a
crate: it is the only path sefy needs, and a dependency for it would cost more
than it saves in a project that keeps its dependency list short on purpose.

### A plugin's own output is never quoted back

When a reply cannot be parsed, the error says so without including the reply. A
transport may well print a signed URL or a token, and this message can end up in
a log or a pasted issue.

## Consequences

- Sync is split cleanly: plugins move bytes, sefy decides meaning. The rules
  about what wins and what is kept twice stay in one place.
- A plugin cannot resolve conflicts, offer per-item sync, or do anything
  incremental. For a format with no cleartext header, that was never available
  anyway: the file has to move whole.
- The protocol is small enough to reimplement in a shell script, which is what
  makes "any executable" true rather than nominal.
- sefy now has a per-user directory. It holds plugins only; the vault keeps its
  no-default-location rule.
- Protocol version 1 is now a promise to plugin authors. Widening the manifest
  with optional fields stays compatible; changing what a field means requires
  version 2, with both versions accepted for a time.
