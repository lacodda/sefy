---
title: Threat model
description: What sefy protects against, what it does not, and why it says so plainly.
---

sefy is **inconspicuous, not deniable**. That phrase is the whole promise, and
everything below is an expansion of it.

## What it protects against

- **A passing glance.** Someone scrolling through your files sees `notes.bak`
  among other backups and has no reason to look twice.
- **A file listing.** There is no extension, no name convention and no
  structure that marks the file as a secret store.
- **A cloud-side scanner.** Services that index or classify uploads look for
  known formats. A `.kdbx` announces itself; this does not.
- **Anyone without the password.** The contents are sealed with
  XChaCha20-Poly1305 under an Argon2id-derived key. Without the password there
  is nothing to read and nothing to tamper with undetected.

## What it does not protect against

- **Forensic analysis.** A file of uniformly high entropy with no header is
  itself a signal to an examiner: it is *some* kind of encrypted container.
  sefy hides *which* tool made it and *what* is inside — not the fact that
  something encrypted exists.
- **Coercion.** If someone can compel you to give up the password, the
  encryption is irrelevant. sefy has no duress password, no hidden volume and
  no plausible-deniability layer, and does not pretend to.
- **A compromised machine.** A keylogger sees your master password; malware
  with your privileges can read the decrypted database out of sefy's memory
  while it runs.
- **Your own clipboard, beyond the timer.** `sefy get` clears the clipboard
  after 45 seconds by default, but anything that reads it during that window —
  including clipboard managers that keep history — gets the secret.

## Why not deniability

Plausible deniability is a much stronger claim: that an examiner cannot prove
encrypted data exists at all. It needs hidden volumes, decoy content and very
careful handling of everything around the file — filesystem timestamps, backup
copies, editor swap files, shell history.

Half-implemented deniability is worse than none, because people rely on it.
sefy makes the smaller, honest promise instead: your secrets are encrypted, and
the file carrying them does not advertise what it is.

## The one deliberate exception

Two features write plaintext on purpose, and both say so before doing it:

- **`sefy export`** produces a JSON file with every secret in the clear. It
  exists so a vault is never a trap — you can always move your data elsewhere —
  and it refuses to run until you acknowledge what the file is.
- **`--editor`** puts a note in a temporary file while your editor is open.
  sefy overwrites and removes that file on exit, but an editor's own swap, undo
  and backup files are its business, not sefy's.
