---
title: "extract"
description: Write a stored file back to disk.
---

Writes a stored file back to disk, byte for byte.

## Usage

```sh
sefy extract <REFERENCE> [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `-o, --output <PATH>` | Where to write it; defaults to the stored file name in the current directory. |
| `--force` | Overwrite the destination if it exists. |

```console
$ sefy extract id_ed25519 -o ~/.ssh/id_ed25519
wrote /home/you/.ssh/id_ed25519 (387 bytes)
```

An existing file is never overwritten silently:

```console
$ sefy extract id_ed25519 -o ~/.ssh/id_ed25519
error: ./id_ed25519 already exists; pass --force to overwrite
```

## Contents, not metadata

The bytes come back identical. Permissions, ownership and timestamps do not —
they were never stored. For anything that cares about its mode, set it after
extracting:

```sh
sefy extract id_ed25519 -o ~/.ssh/id_ed25519
chmod 600 ~/.ssh/id_ed25519
```

Once extracted, the file is a plain file on disk like any other; the vault
protects what is inside it, not the copy you just wrote out.

## Related

- [`add`](/sefy/reference/add/) — storing a file in the first place
- [`get`](/sefy/reference/get/) — why files do not come out through the clipboard
- [Keeping ssh keys in a vault](/sefy/guides/ssh-keys/)
