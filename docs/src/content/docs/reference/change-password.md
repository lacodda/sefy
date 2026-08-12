---
title: "change-password"
description: Replace the master password.
---

Replaces the master password and rewrites the vault under it.

## Usage

```console
$ sefy change-password
Master password:
New master password:
Repeat it:
password changed
```

| Option | Meaning |
| --- | --- |
| `--new-password-env <VAR>` | Read the **new** password from this variable. |

```sh
sefy --password-env OLD change-password --new-password-env NEW
```

The global `--password-env` supplies the current password, `--new-password-env`
the replacement — two variables, because one would make "old" and "new"
indistinguishable.

## What actually changes

The file is rewritten with a **fresh salt and nonce**, so the new vault shares
nothing with the old one: not a key, not a prefix, not a comparable byte. An
observer holding both copies cannot tell they contain the same items.

What this does **not** do is reach into copies you already made. Backups,
synced copies and anything a cloud service kept still open with the **old**
password. Changing the password limits what a future copy is worth; it does not
retract the ones already out there.

That is the reason to change it when a machine is handed on: delete the vault
there *and* change the password on the copy you keep, so the two are no longer
opened by the same secret.

## Related

- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — backups and old copies
- [How the vault works](/sefy/concepts/vault-format/) — salt, nonce and key derivation
