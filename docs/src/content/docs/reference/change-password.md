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

The [copies sefy keeps beside the vault](/sefy/guides/syncing/#a-copy-before-every-merge)
before a merge are re-sealed under the new password too, each keeping the time
it was taken:

```console
password changed
2 copies beside the vault sealed under it too
```

A file in a copy's place that the old password does not open is named and left
exactly as it was — it is not known to be this vault's.

What this does **not** do is reach into copies you made yourself. Backups,
synced copies and anything a cloud service kept still open with the **old**
password. Changing the password limits what a future copy is worth; it does not
retract the ones already out there.

That is the reason to change it when a machine is handed on: delete the vault
there *and* change the password on the copy you keep, so the two are no longer
opened by the same secret.

## With syncing after every change

With [`SEFY_AUTO_SYNC=on`](/sefy/guides/syncing/#syncing-after-every-change),
the new password goes to the remote straight away: the remote copy is opened
with the old password, folded in, and replaced by one under the new. The other
machines then need the new password to pull.

## Related

- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — backups and old copies
- [How the vault works](/sefy/concepts/vault-format/) — salt, nonce and key derivation
