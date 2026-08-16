---
title: "Commands"
description: Every sefy command, and the conventions they all share.
---

Every command that touches items works on one vault file, asks for one master
password, and does one thing to the items inside it.
[`plugin`](/sefy/reference/plugin/) and
[`completions`](/sefy/reference/completions/) are the exceptions: they report on
the installation itself and need neither a vault nor a password.

## The commands

### Making and reading a vault

- [`init`](/sefy/reference/init/) — create a new vault
- [`ls`](/sefy/reference/ls/) — list items
- [`find`](/sefy/reference/find/) — search items by text, kind and tags
- [`show`](/sefy/reference/show/) — show an item without its secret fields

### Items

- [`add`](/sefy/reference/add/) — add a note, a credential or a file
- [`get`](/sefy/reference/get/) — copy a secret to the clipboard
- [`edit`](/sefy/reference/edit/) — change a title, contents or tags
- [`rm`](/sefy/reference/rm/) — remove an item
- [`extract`](/sefy/reference/extract/) — write a stored file back to disk
- [`tags`](/sefy/reference/tags/) — list the tags in use

### Moving data in and out

- [`export`](/sefy/reference/export/) — write the contents out as plain JSON
- [`import`](/sefy/reference/import/) — add the contents of an export
- [`merge`](/sefy/reference/merge/) — fold another vault file into this one

### The vault itself

- [`change-password`](/sefy/reference/change-password/) — replace the master password
- [`plugin`](/sefy/reference/plugin/) — inspect the installed transports
- [`completions`](/sefy/reference/completions/) — print a shell completion script

## Choosing the vault

sefy has **no default location**: pass `--vault <FILE>` or set `SEFY_VAULT`. A
vault at a predictable path like `~/.sefy/vault` would undo the point of a file
that looks like nothing.

```sh
export SEFY_VAULT=~/backups/notes.bak
```

| Variable | Meaning |
| --- | --- |
| `SEFY_VAULT` | Path of the vault to work on, when `--vault` is not given. |

## The master password

The password is asked for on the terminal, without echo. For scripts,
`--password-env <VAR>` reads it from an environment variable instead.

A password cannot be passed as an argument: it would land in the shell history
and in every process listing. Password variables are never fixed names either —
you name them yourself and point sefy at them with `--password-env`,
`--item-password-env` or `--new-password-env`.

Without a terminal, sefy refuses to prompt rather than hanging.

## References

Wherever a command takes a `<REFERENCE>`, it accepts:

1. an **id** — `sefy get 7`;
2. an **exact title**, case-insensitive — `sefy get bank`;
3. **text to search for**, matched against titles, note bodies and credential
   fields — `sefy get grocer`.

An exact title always beats a substring. If more than one item still matches,
sefy lists the candidates rather than guessing:

```console
$ sefy get mail
error: 2 items match "mail":
     3  mail — personal                 credential
     7  mail — work                     credential
narrow the text, or use an id
```

## Exit status

`0` on success, `1` on any error. Errors go to stderr; a wrong password and a
file that is not a vault produce the same message, because an authenticated
blob genuinely cannot tell the two apart.
