<p align="center"><img src="assets/banner.svg" alt="sefy — an inconspicuous encrypted vault" width="720"></p>

# sefy

An inconspicuous encrypted vault for your secrets.

Notes, credentials, and files live in an encrypted SQLite database stored as a
single file that looks like nothing: no magic bytes, no telltale extension, no
recognizable structure. Name it `notes.bak`, drop it among your backups or into
any cloud folder — it draws no attention.

## Threat model, honestly

- **Protects against:** casual observers, curious eyes, cloud scanners, and
  anyone who does not know the file is a vault. The file is a uniform blob of
  random-looking data.
- **Does not protect against:** forensic analysis (a high-entropy file with no
  known header is itself a hint) or coercion. sefy is *inconspicuous*, not
  *deniable* — and we will not pretend otherwise.

## How it works

- The master password is stretched into a key with **Argon2id**.
- The whole SQLite database is sealed with **XChaCha20-Poly1305** (AEAD).
- The file on disk is `salt ‖ nonce ‖ ciphertext` — no magic bytes, no header,
  nothing to recognize. Two saves of identical content share no prefix.
- The decrypted database exists **only in memory**. SQLite is never given a
  path, so no page, journal or temporary file lands on disk.
- Saves are atomic: ciphertext goes to a temporary file, is synced, and is
  renamed over the vault. A crash leaves either the old vault or the new one.

Details and rationale: [ADR-0001](docs/adr/0001-vault-file-format-and-cryptography.md).

## Status

Early stage, under active development. The core library (`sefy-core`) is in
place: vault format, cryptography, the data model of notes, credentials, file
attachments and tags, plus search. The `sefy` command-line tool is next, then
release packaging, sync plugins and a GUI.

The file format is not yet frozen — until the first release, a vault may need to
be recreated between versions.

## Building

```sh
cargo build            # workspace: sefy-core (library) + sefy (CLI)
cargo test             # unit, integration and doc tests
```

## License

[MIT](LICENSE)
