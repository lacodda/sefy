---
title: How the vault works
description: The on-disk format, the cryptography behind it, and why the file carries no header.
---

## The file

```text
salt (16 B) ‖ nonce (24 B) ‖ ciphertext (SQLite bytes + 16 B Poly1305 tag)
```

That is the whole layout. No magic number, no header, no length prefix, no
extension convention. The salt and nonce are uniformly random, and everything
after them is indistinguishable from random without the password.

The **format version lives inside the ciphertext**, as the first byte of the
sealed payload. A version byte in the clear would be exactly the signature the
format exists to avoid; inside, it costs one byte and is readable the moment
the password is known.

Because salt and nonce are sampled fresh on every save, two saves of identical
content share no prefix — there is not even a stable fingerprint to compare
across backups.

## The cryptography

- **Key derivation:** Argon2id, 64 MiB of memory, 3 iterations, parallelism 4,
  producing a 256-bit key.
- **Encryption:** XChaCha20-Poly1305, authenticating the entire payload with a
  24-byte random nonce.
- **Randomness:** the operating system's source, never a userspace PRNG.

The Argon2 parameters are **compiled in, not stored in the file**. Storing them
would mean either a parameter block in the clear — a signature — or a
negotiation protocol inside the ciphertext, which is complexity with no user.
Changing them means minting format version 2.

The extended 24-byte nonce is what makes random nonces safe here: collisions
are negligible without any counter to persist, which suits a format that must
carry no state in the clear.

## Wrong password, corruption and a stranger's file

All three produce the same error:

```console
$ sefy ls
error: wrong password, or /home/you/notes.bak is not a vault
(an encrypted file cannot tell the two apart)
```

This is not vagueness for its own sake. The authentication tag fails identically
whether the key was wrong, a byte was flipped, or the file was never a vault —
there is genuinely nothing to distinguish them. The only exception is a file too
short to hold a salt, a nonce and a tag, which is rejected without needing a key.

## Nothing plaintext on disk

The database is opened with SQLite's in-memory backend and moved in and out
through its `serialize` / `deserialize` interface. SQLite is never given a path,
and `journal_mode` and `temp_store` are both set to `MEMORY`, so no page,
journal or temporary file can appear on disk.

## Atomic saves

Saving writes the ciphertext to a sibling file named `<vault>.sefy-tmp`, flushes
and syncs it, then renames it over the vault. A crash at any point leaves either
the old vault or the new one — never a half-written file, and never plaintext.

The temporary name is derived from the target rather than randomized, so a
crashed write leaves one predictable file that the next save reuses instead of
accumulating debris.

## Stability

The format is **stable at version 1**. Files written by sefy 0.1.0 will stay
readable: any future change arrives as version 2, able to read version 1 and
migrate it. The Argon2 parameters are part of that promise.

Full rationale, including the alternatives that were rejected:
[ADR-0001](https://github.com/lacodda/sefy/blob/main/docs/adr/0001-vault-file-format-and-cryptography.md).
