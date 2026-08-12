---
title: "export"
description: Write the vault's contents out as plain, unencrypted JSON.
---

Writes the whole vault out as plain, **unencrypted** JSON. This exists so a
vault is never a trap: contents can be migrated, kept in another form, or moved
to a different tool.

## Usage

```sh
sefy export --i-know-this-writes-plaintext [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `-o, --output <PATH>` | Where to write it; omit to print to stdout. |
| `--i-know-this-writes-plaintext` | Required. |
| `--force` | Overwrite the destination if it exists. |

```console
$ sefy export --i-know-this-writes-plaintext -o backup.json
wrote backup.json in the clear
```

```sh
sefy export --i-know-this-writes-plaintext | gpg -c > backup.json.gpg
```

## Why the flag is required

The acknowledgement flag is required rather than a printed warning: a warning
arrives after the file is already on disk, and scripts do not read them at all.

The resulting file is exactly as sensitive as the vault and protects nothing.
Every password, note and stored file is in it in the clear. Write it somewhere
that is not synced or backed up, and delete it when you are done — or pipe it
straight into whatever consumes it, so it never reaches disk.

## Format

```json
{
  "sefy_export": 1,
  "items": [
    { "title": "bank", "kind": "note", "tags": ["money"], "text": "code 4815" },
    { "title": "mail", "kind": "credential", "login": "someone",
      "password": "…", "url": "…", "totp": "…", "notes": "…" },
    { "title": "key", "kind": "file", "filename": "id_ed25519",
      "bytes_base64": "…" }
  ]
}
```

Notes need `text`; credentials need `login` and `password`; files need
`filename` and `bytes_base64`. Everything else is optional. This is a plain
enough shape to generate from another tool by hand.

## Related

- [`import`](/sefy/reference/import/) — reading one back in
- [Moving a vault between machines](/sefy/guides/moving-a-vault/)
- [Threat model](/sefy/concepts/threat-model/) — the one deliberate exception
