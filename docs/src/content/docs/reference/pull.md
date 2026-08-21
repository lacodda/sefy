---
title: "pull"
description: Fetch the remote copy and fold it into this vault.
---

Asks a transport for the copy at the remote, opens it here, and merges it into
this vault.

## Usage

```sh
sefy pull [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `-p, --transport <NAME>` | Which transport to use; omit when only one is installed. |
| `--name <NAME>` | What the remote copy is called. Default `vault`. |
| `--remote-password-env <VAR>` | Read the remote copy's password from this variable. |
| `--ask-remote-password` | Ask for the remote copy's password instead of reusing this vault's. |

```console
$ sefy pull
Master password:
pulled "vault" through github
downloaded 12.4 KiB
merged: 2 added, 1 updated, 14 unchanged
```

## It merges, it does not replace

A pull is not a download over the top of your vault. What comes back is folded
in item by item, exactly as [`merge`](/sefy/reference/merge/) does it: items
missing here are copied across, newer contents replace older ones, an item
changed on both sides is **kept twice**, and nothing local is ever deleted.

That is the whole reason a transport carries a sealed blob it cannot read. Since
it cannot tell what changed, it does not try — it fetches the other copy, and
sefy decides, with both sides open and both passwords in hand.

When both sides changed the same item, the output is the same loud report
`merge` gives, and the same cleanup applies:

```console
1 item changed on both sides and could not be resolved here.
This vault's version was kept; the incoming one is beside it:
  "mail" → also kept as "mail (conflicted copy)"
Compare them, keep the right one, and remove the other.
```

## The remote copy's password

A pull brings back a copy of *this* vault, so the same master password is the
ordinary case and the default — unlike `merge`, which folds in a file from
anywhere and always asks separately.

When the two genuinely differ:

```sh
sefy pull --ask-remote-password              # asks on the terminal
sefy pull --remote-password-env REMOTE_PW    # for scripts
```

If the remote copy is under another password and you have not said so, sefy
fails with `wrong password, or … is not a vault` — an encrypted file cannot tell
those two apart.

## What touches the disk

The transport has to write the fetched copy somewhere, so sefy gives it a
scratch path and removes it as soon as the merge is done — on the failure path
as well as the successful one. What sits there in between is the **sealed** blob,
the same thing anyone would find at the remote; the decrypted database still
never leaves memory.

A transport that reports success without writing anything is called out for it,
rather than surfacing as a password error:

```console
error: plugin github failed: it reported success but wrote no file
```

## Related

- [`push`](/sefy/reference/push/) — send this vault the other way
- [`sync`](/sefy/reference/sync/) — pull, then push
- [`merge`](/sefy/reference/merge/) — the same fold, from a local file
- [Moving a vault between machines](/sefy/guides/moving-a-vault/)
