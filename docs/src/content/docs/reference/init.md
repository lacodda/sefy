---
title: "init"
description: Create a new vault.
---

Creates a new vault file and the master password that opens it.

## Usage

```sh
sefy --vault ~/backups/notes.bak init
```

```console
$ sefy init
New master password:
Repeat it:
created /home/you/backups/notes.bak
```

The password is asked for twice, because a typo here would lock you out of an
empty vault forever — there is no recovery path, no hint and no second key.

`init` refuses to touch a path that already exists, rather than offering to
overwrite it. Overwriting a vault destroys every secret in it and cannot be
undone, so that is not a confirmation prompt worth having.

## Choosing a name

The name is yours. `notes.bak`, `archive-2019.dat`, anything at all — there is
no extension sefy expects and none it writes. The file itself carries no header
and no magic bytes, so nothing but the name suggests what it is.

## Related

- [`change-password`](/sefy/reference/change-password/) — replace the password afterwards
- [How the vault works](/sefy/concepts/vault-format/) — what `init` actually writes
