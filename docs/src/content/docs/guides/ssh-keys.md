---
title: Keeping ssh keys in a vault
description: Store private keys and their passphrases, put a key back on a new machine, and know where this approach stops.
---

An ssh private key is a file that must not leak and must survive losing a
machine. A vault handles both: the key travels as one opaque blob, and the
passphrase that unlocks it can live beside it.

This guide covers putting keys in, getting them back onto a machine, and the
part worth being clear about — what happens between `sefy extract` and `ssh`.

## Storing a key

Add the private key as a file and the passphrase as a credential, tagged so they
come back together:

```console
$ sefy add file ~/.ssh/id_ed25519 --tag keys
added "id_ed25519" as 3

$ sefy add credential "id_ed25519 passphrase" --login you --tag keys
Password for this item:
added "id_ed25519 passphrase" as 4
```

Contents are stored byte for byte, so a key comes back exactly as it went in —
no re-encoding, no trailing-newline surprises, nothing that would make `ssh`
reject it.

The **public** key is worth storing too. It is not a secret, but regenerating it
on a new machine means having the private key already unlocked, and you may want
to paste it into a service before you have a working key on that machine:

```sh
sefy add file ~/.ssh/id_ed25519.pub --tag keys
```

With several keys, give them titles that say where they belong rather than
leaving the filename to do it:

```console
$ sefy add file ~/.ssh/id_ed25519 -T "work key" --tag keys,work
added "work key" as 5
```

Then the set comes back as a set:

```console
$ sefy ls --tag keys
5  work key               file        [keys, work]
4  id_ed25519 passphrase  credential  [keys]
3  id_ed25519             file        [keys]
```

## Putting a key back on a machine

```console
$ sefy extract id_ed25519 -o ~/.ssh/id_ed25519
wrote /home/you/.ssh/id_ed25519 (464 bytes)
```

An existing file is never overwritten silently:

```console
$ sefy extract id_ed25519 -o ~/.ssh/id_ed25519
error: ./id_ed25519 already exists; pass --force to overwrite
```

Then the two steps sefy does not do for you.

**Fix the permissions.** ssh refuses a private key that is readable by anyone
else, and sefy does not store or restore file modes — it keeps contents, not
metadata:

```sh
chmod 600 ~/.ssh/id_ed25519
```

**Get the passphrase.** It goes to the clipboard, so it does not land in your
scrollback:

```console
$ sefy get "id_ed25519 passphrase"
copied password of "id_ed25519 passphrase" to the clipboard; clearing in 45s
```

Paste it when `ssh-add` asks. If the agent prompt is slower than the timer,
`--clear-after 120` gives you longer.

## The gap this leaves

Between `sefy extract` and `chmod` the key is a plaintext file on disk, and it
stays one afterwards — that is what ssh needs it to be. A vault protects a key
*at rest, elsewhere*; it does not protect the working copy on a machine you are
using.

So the useful framing is: the vault is where the key survives losing the laptop,
not a way to run ssh without a key on disk. If a machine is untrusted, extracting
a key onto it hands the key over.

There is one place this is sharper than it looks. `sefy get` will not print a
stored file at all, not even with `--stdout`:

```console
$ sefy get id_ed25519 --stdout
error: "id_ed25519" is a file; write it to disk with: sefy extract 3
```

That is deliberate — a private key dumped into a terminal is a private key in
the scrollback — but it also means you cannot pipe a key straight into
`ssh-add -` from the vault. The key goes to disk first.

## A short-lived key on a machine you do not keep

If you must use a key somewhere temporary, extract it, load it into the agent,
and remove the file immediately — the agent keeps the decrypted key in memory
and no longer needs the file:

```sh
sefy extract id_ed25519 -o /tmp/k && chmod 600 /tmp/k
ssh-add -t 3600 /tmp/k     # forget it again after an hour
rm /tmp/k
```

`rm` unlinks the file; it does not scrub the blocks it occupied, and on a
copy-on-write or flash-backed filesystem there is no reliable way to. Treat this
as reducing exposure, not eliminating it — and on a machine you truly do not
trust, use a key that machine is allowed to have, not this one.

## Restoring a whole machine

The point of storing keys this way shows up when a machine is gone. With the
vault file and the password:

```sh
export SEFY_VAULT=~/backups/notes.bak
mkdir -p ~/.ssh && chmod 700 ~/.ssh

sefy extract id_ed25519     -o ~/.ssh/id_ed25519
sefy extract id_ed25519.pub -o ~/.ssh/id_ed25519.pub
chmod 600 ~/.ssh/id_ed25519
```

`~/.ssh/config` is not a secret, but it is tedious to rebuild from memory, and
it names the hosts you connect to. Keeping it in the vault is reasonable if you
would rather that list not sit in a public dotfiles repository:

```sh
sefy add file ~/.ssh/config --tag keys
```

## Rotating a key

Storing a key does not extend its life. When you replace one, add the new key
under a new title rather than editing the old item — an item's kind and contents
are replaced wholesale, and you want the old key retrievable until every server
has accepted the new one:

```sh
sefy add file ~/.ssh/id_ed25519_new -T "work key 2026" --tag keys,work
```

Remove the retired one once nothing depends on it:

```console
$ sefy rm "work key"
remove "work key" (5)? [y/N] y
removed 5
```

Remember that older **backups of the vault** still contain the retired key. That
is fine — a rotated key is only dangerous while it is still authorized — but it
is the reason rotation means removing the key from `authorized_keys`, not just
from the vault.
