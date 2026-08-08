# ADR-0001: Vault file format and cryptography

- **Status:** accepted
- **Date:** 2026-08-08

## Context

sefy stores notes, credentials and file attachments in a single file that must
not announce what it is. Every established option announces itself: `.kdbx`
carries a magic number, age and gpg write textual headers, VeraCrypt needs a
container and a mount. A recognizable header defeats the product's only
distinguishing promise.

At the same time the file must be genuinely encrypted with primitives worth
trusting, and the plaintext database must never touch the disk — that is the one
invariant the whole product rests on.

The prototype from 2024 failed on every count: AES-256-CBC with a hard-coded IV
(`"unique_initializ"`), a hex key typed in with no key derivation, no
authentication tag, and decryption into a plaintext temporary file next to the
vault.

## Decision

### File layout

```text
salt (16 B) ‖ nonce (24 B) ‖ ciphertext (payload + 16 B Poly1305 tag)
```

Nothing else. No magic bytes, no version byte in the clear, no length prefix, no
extension convention. Salt and nonce are sampled fresh from the OS random source
on every write, so two saves of identical content share no prefix.

The sealed payload is:

```text
format version (1 B) ‖ serialized SQLite database
```

The version lives *inside* the ciphertext. A version byte in the clear would be
the signature the format exists to avoid; inside, it costs one byte and is
readable the moment the password is known.

### Cryptography

- **Key derivation:** Argon2id, 64 MiB memory, 3 iterations, parallelism 4,
  32-byte output (`argon2` crate).
- **Encryption:** XChaCha20-Poly1305 (`chacha20poly1305` crate), 24-byte random
  nonce, authenticating the entire payload.
- **Randomness:** `getrandom` — the OS source, never a userspace PRNG.

Argon2 parameters are compiled in rather than stored. Storing them would mean
either a parameter block in the clear (a signature) or a negotiation protocol
inside the ciphertext (complexity with no user today). Changing the parameters
means minting format version 2, which can read version 1 and rewrite it.

The extended 24-byte nonce of XChaCha20 is what makes random nonces safe here:
collisions are negligible without any counter state to persist, which suits a
format that must carry no state in the clear.

### Wrong password and corruption are one error

The Poly1305 tag cannot distinguish a wrong password from a corrupted file from
a file that was never a vault, and neither does the API: all three surface as
`Error::WrongPasswordOrNotAVault`. A file shorter than the minimum envelope is
the one exception, reported as `Error::TooSmall`, since that needs no key.

### Database in memory only

The SQLite database is opened with `Connection::open_in_memory` and moved in and
out through SQLite's `serialize` / `deserialize_read_exact` interface. SQLite is
never given a path, and `journal_mode` and `temp_store` are set to `MEMORY`, so
no page, journal or temporary file can appear on disk.

### Atomic writes

Saving writes the ciphertext to a sibling file named `<vault>.sefy-tmp`, flushes
and `sync_all`s it, then renames it over the vault. `fs::rename` replaces an
existing destination on both Unix and Windows, so the swap is one step: a crash
leaves either the old vault or the new one, and never plaintext. The temporary
name is derived from the target rather than randomized, so a crashed write
leaves one predictable file that the next save reuses instead of accumulating
debris.

## Consequences

- The file is indistinguishable from random data to anyone without the password,
  which is exactly the promise made in the README — and no more. A forensic
  examiner still sees a high-entropy headerless file and can reasonably guess it
  is *some* container. sefy is inconspicuous, not deniable.
- Every save rewrites the whole file. For a personal secret store this is
  irrelevant; it would not scale to a multi-gigabyte vault, and that is an
  accepted limit.
- The entire database is held in memory while open, so a vault with large
  attachments costs its own size in RAM.
- Format version 1 is fixed. Any change to the envelope or to the Argon2
  parameters requires version 2 plus a migration path that reads version 1.

## Alternatives considered

- **SQLCipher / rusqlite with encryption.** Page-level encryption keeps a
  recognizable file structure and a plaintext-adjacent working file. Rejected:
  it defeats the inconspicuousness the product is built on.
- **AES-256-GCM instead of XChaCha20-Poly1305.** Sound, but its 12-byte nonce
  makes random nonces a counting exercise, and it leans on hardware
  acceleration that a pure-Rust build cannot assume. Rejected in favour of the
  extended-nonce construction.
- **A header carrying KDF parameters.** Standard practice elsewhere, and exactly
  the signature this format must not have. Rejected; the version byte moved
  inside the ciphertext instead.
- **Keeping the prototype's AES-CBC and repairing it.** The IV, the missing KDF,
  the missing authentication and the plaintext temporary file are four
  independent defects. Rejected in favour of a rewrite.
