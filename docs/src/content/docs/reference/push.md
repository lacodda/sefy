---
title: "push"
description: Send this vault to the remote, replacing the copy there.
---

Hands the vault file to a transport, which puts it wherever that transport
stores things.

## Usage

```sh
sefy push [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `-p, --transport <NAME>` | Which transport to use; omit when only one is installed. |
| `--name <NAME>` | What the remote copy is called. Default `vault`. |

```console
$ sefy push
Master password:
pushed "vault" through github
uploaded 12.4 KiB
```

The second line comes from the transport itself and says whatever that
transport has to say.

## What travels

The **sealed file** — the same bytes that sit on your disk. A transport is
handed a path and a name, and that is all:

```json
{ "operation": "push", "file": "/tmp/.sefy-a1b2c3/blob", "name": "vault" }
```

No master password, no key, no item. A transport cannot read what it carries,
which is deliberate: handing decrypted items to a third-party binary would put
every secret in the vault into somebody else's process, and that is the one
thing this product exists not to do.

The trade is that a transport cannot merge either. See
[`pull`](/sefy/reference/pull/) for what happens instead.

## Push replaces

The remote copy is overwritten with this one. Anything that was only over there
is gone from the remote — though a transport that keeps versions (a git
repository, for instance) can still have it in its own history.

On a machine that is one of several, use [`sync`](/sefy/reference/sync/)
instead: it pulls first, so what goes up holds both sides.

## Choosing the transport

With one transport installed, sefy uses it. With several, it says so and stops:

```console
$ sefy push
error: several transports are installed: file, github
say which one with --transport <NAME>.
```

Guessing would mean deciding where somebody's vault goes, and a wrong guess
there does not announce itself. `SEFY_TRANSPORT` sets the choice for a shell
session; there is no configuration file, because sefy keeps nothing on disk but
the vault and its plugins.

## The remote name

`--name` is the handle the transport stores the copy under, and it defaults to
`vault`. It exists because the local file name is a poor handle: a vault is
deliberately named like anything else, and on a shared remote two machines'
`notes.bak` would collide.

```sh
sefy push --name work-laptop
```

| Variable | Meaning |
| --- | --- |
| `SEFY_TRANSPORT` | Transport to use, when `--transport` is not given. |
| `SEFY_REMOTE_NAME` | Remote name, when `--name` is not given. |

## Related

- [`pull`](/sefy/reference/pull/) — bring the remote copy back and fold it in
- [`sync`](/sefy/reference/sync/) — pull, then push
- [`plugin`](/sefy/reference/plugin/) — what is installed, and why anything is unusable
- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — doing it by hand
