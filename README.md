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

## Status

Early stage. The current code is a 2024 prototype; the core is being rewritten
from scratch:

- password → Argon2id key derivation
- XChaCha20-Poly1305 (AEAD) over the whole database
- file format `salt ‖ nonce ‖ ciphertext` — still a signature-free blob
- the decrypted database lives only in memory, never on disk
- core library first, then CLI, then GUI and sync plugins

## License

[MIT](LICENSE)
