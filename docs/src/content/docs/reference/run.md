---
title: "run"
description: Run a command with secrets from the vault in its environment.
---

A script or a tool that reads a token from a variable gets it from the vault
for as long as it runs: no `.env` file on disk, no `export` in the shell
history, no secret on a command line where every process listing can see it.

## Usage

```sh
sefy run --env <VAR=REFERENCE[#FIELD]>... -- <COMMAND> [ARGS]...
```

| Option | Meaning |
| --- | --- |
| `-e`, `--env <VAR=REFERENCE[#FIELD]>` | Set `VAR` from an item. Repeat for several; at least one is required. |

```sh
sefy run -e GITHUB_TOKEN=github -- gh repo list
sefy run -e NPM_TOKEN=npm -- npm publish
sefy run -e DB_USER=db#login -e DB_PASSWORD=db -- ./migrate.sh
```

The command comes after `--`, always: everything past it belongs to the
command, so its own flags are never taken for sefy's. It gets the terminal,
the input and the output exactly as if it had been typed on its own, and sefy
prints nothing of its own unless something goes wrong.

## Which value a variable gets

`VAR=REFERENCE` takes the value [`get`](/sefy/reference/get/) would take: the
item's own secret — a login's password, an API token's token, a note's text.
`#FIELD` names another field of a record:

| Mapping | Value |
| --- | --- |
| `TOKEN=ci` | the `token` of the API token `ci` |
| `DB_USER=db#login` | the `login` of the login `db` |
| `DB_PASSWORD=db` | its `password` |
| `API_URL=ci#url` | the `url` of `ci` — public fields are fine too |

The reference is the same as everywhere else — an id, an exact title, or text
to search for — and the split is at the **last** `#`. A title with a `#` in it
is taken by its id. In a script, prefer ids or exact titles: text that finds one
item today can find two tomorrow, and sefy then stops rather than guessing.

Every variable is settled **before** the command starts. A reference that
matches nothing or several items, a field the record does not have, or a
stored file (which belongs to [`extract`](/sefy/reference/extract/)) stops the
run, and the command never starts with half of what it needs:

```console
$ sefy run -e TOKEN=ci -e DB_PASSWORD=dbx -- ./deploy.sh
error: cannot set DB_PASSWORD: nothing matches "dbx"
```

Naming one variable twice is refused, and so are two spellings of one name
like `TOKEN` and `token` — Windows holds those as one variable, and a script
should not lose a value when it moves there.

## What else the command sees

The rest of the environment is passed on as it is, with one exception: the
variable named by `--password-env` is **left out**. The command was given a
token, not the key to every other secret in the vault. A variable already set
in the environment is replaced by the mapping of the same name.

The vault is closed before the command starts. On Linux and macOS sefy then
*becomes* the command — the process is replaced, so Ctrl+C, signals and job
control reach the command directly, and nothing of sefy stays in memory. On
Windows sefy starts the command, leaves Ctrl+C to it, and exits when it does.

A bare command name is found the way your shell finds it. On Windows that
includes `.cmd` and `.bat` scripts through `PATHEXT`, so `npm`, `pnpm` and
the like work as typed.

## Input from a pipe

The command's input is sefy's input, so data can be piped straight through:

```sh
cat payload.json | sefy run -e TOKEN=ci -- ./upload.sh
```

The master password is asked for on the terminal itself, not on stdin, so the
pipe does not get in its way. Without any terminal at all — in CI, from a
service — pass it with `--password-env`.

## Exit status

sefy ends with the command's own status. When the command never ran, the
status says why, the same way `env`, `nohup` and `timeout` do:

| Status | Meaning |
| --- | --- |
| `125` | sefy could not set the variables: no vault, a wrong password, a reference that does not resolve. |
| `126` | The program was found but could not be started. |
| `127` | No such program. |
| `2` | The command line itself is wrong — no `--`, no `--env`. |

## What the environment does not protect

A variable is out of sight, not out of reach. Any process running as the same
user can read another's environment, and the command passes it on to every
process it starts in turn. `run` keeps secrets out of files, history and
process listings; it does not make them invisible to the machine they are
used on. The [threat model](/sefy/concepts/threat-model/) has the whole
picture.

## Related

- [`get`](/sefy/reference/get/) — one secret to the clipboard or to stdout
- [`add`](/sefy/reference/add/) — `add api-token` for the tokens `run` hands out
- [Commands](/sefy/reference/commands/) — references, the vault and the master password
