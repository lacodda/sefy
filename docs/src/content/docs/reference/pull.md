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
| `--dry-run` | Say what the pull would change, item by item, and change nothing. |

```console
$ sefy pull
Master password:
pulled "vault" through github
downloaded 12.4 KiB
merged: 2 added, 1 updated, 14 unchanged
the vault as it was is kept as /home/you/Documents/.notes.db.1
```

## It merges, it does not replace

A pull is not a download over the top of your vault. What comes back is folded
in item by item, exactly as [`merge`](/sefy/reference/merge/) does it: items
missing here are copied across, contents the other side moved on from here are
brought up to date, an item changed on both sides keeps the other side's version
in its [history](/sefy/reference/history/), and nothing local is ever deleted.

That is the whole reason a transport carries a sealed blob it cannot read. Since
it cannot tell what changed, it does not try — it fetches the other copy, and
sefy decides, with both sides open and both passwords in hand.

When both sides changed the same item, the output is the same loud report
`merge` gives, and says where the version that lost went:

```console
1 item changed on both sides.
The copy changed more recently is current; the other is kept in the item's history:
  "mail" (this vault's is current): sefy history 2
Compare with sefy history ID VERSION; bring one back with sefy restore ID VERSION.
```

## Looking first

```console
$ sefy pull --dry-run
here, from "vault" through github:
  add       "wifi"
  add       "github"
  update    "mail" (3)
  14 unchanged
dry run: nothing was written here or sent anywhere
```

The remote copy is fetched for real — there is no other way to know what it
holds — and the merge runs on a copy of this vault held in memory. Nothing is
written, no copy is kept and no transfer is recorded: the vault is exactly as it
was.

## A copy before the merge

When a pull changes anything, the vault as it was is kept beside it first, as
`FILE.1` — the three most recent are kept. A pull that finds nothing new keeps
none. See [A copy before every merge](/sefy/guides/syncing/#a-copy-before-every-merge).

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
scratch path and removes it as soon as it has been read — on the failure path
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
