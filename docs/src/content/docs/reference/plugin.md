---
title: "plugin"
description: Inspect the transports installed on this machine.
---

A **plugin** is a transport: it moves your vault file between this machine and
somewhere else — a git remote, an FTP server, a cloud drive. sefy itself knows
nothing about any of them.

`sefy plugin` reports on the transports installed here. Moving a vault with one
arrives in a later release; this command is how you check that a plugin is
installed and that sefy is willing to run it.

## Usage

```sh
sefy plugin list [--paths]
```

| Option | Meaning |
| --- | --- |
| `--paths` | Also print the directories sefy searched. |

```console
$ sefy plugin list
broken  ?         unusable: it refused to describe itself
ftp     0.2.1     push
future  9.0.0     unusable: it speaks protocol 99 and this build speaks 1
github  0.1.0     pull, push
```

Three columns: the plugin's name, its version, and either the operations it
offers or why it cannot be used.

Everything found is listed, working or not. A broken plugin that was quietly
omitted would look exactly like one that was never installed — and those two
call for opposite fixes.

## What a plugin never sees

A transport is handed **the path of the sealed vault file** and nothing else.
It never receives your master password, a key derived from it, or the contents
of a single item. What it carries is what anyone would find on your disk: a
blob it cannot read.

That is also why a plugin cannot merge two copies. It fetches the other one to
a file, and sefy folds the two together itself with
[`merge`](/sefy/reference/merge/), where both sides can actually be read.

## Where plugins live

sefy looks in two places, in this order:

| Order | Location |
| --- | --- |
| 1 | `%APPDATA%\sefy\plugins` (Windows) · `~/Library/Application Support/sefy/plugins` (macOS) · `$XDG_DATA_HOME/sefy/plugins`, else `~/.local/share/sefy/plugins` (Linux) |
| 2 | Every directory on `PATH` |

The first copy of a given name wins, as with any other command. `--paths` shows
the list as sefy resolved it:

```console
$ sefy plugin list --paths
looked in:
  C:\Users\you\AppData\Roaming\sefy\plugins
  C:\Users\you\bin
  ...
```

Nothing is ever looked for **beside the vault file**. A `plugins/` directory
sitting next to an otherwise anonymous blob would announce what that blob is,
which is the one thing the file format spends all its effort avoiding.

## Writing a plugin

A plugin is any executable named `sefy-plugin-<name>` — a compiled binary, a
shell script, a Python file. It answers two invocations.

**`--manifest`** prints a JSON description and exits:

```json
{
  "protocol_version": 1,
  "name": "github",
  "version": "0.1.0",
  "description": "Keeps a vault in a private git repository",
  "operations": ["push", "pull"]
}
```

| Field | Meaning |
| --- | --- |
| `protocol_version` | Must be `1`. Anything else is listed but refused. |
| `name` | Short name, shown in `plugin list`. |
| `version` | The plugin's own version, for diagnosis. |
| `description` | Optional single line. |
| `operations` | Some of `push`, `pull`. Declaring none makes it unusable. |

**`run`** reads a request as JSON on stdin and prints a report on stdout:

```console
$ echo '{"operation":"push","file":"/tmp/vault.bin","name":"vault"}' | sefy-plugin-github run
{"message":"pushed to origin"}
```

| Request field | Meaning |
| --- | --- |
| `operation` | `push` — upload the file at `file`. `pull` — download the remote copy *to* `file`. |
| `file` | Local path of the sealed vault. Read it to push; write it to pull. |
| `name` | What the remote copy should be called. |

The report may carry a `message` to show the user, or an `error` to report
failure without a non-zero exit. Printing nothing at all means success with
nothing to say.

```json
{ "message": "pushed 4.2 KiB to origin" }
{ "error": "no credentials for the remote" }
```

A plugin runs for the duration of one call; nothing here starts a service.

### Why the operations are declared

sefy refuses an operation a plugin did not declare, rather than running it and
interpreting whatever comes back. A transport that can only publish says
`["push"]`, and asking it to pull fails with that reason instead of an error
message written for somebody else's tool.

### What sefy will not print back

If a plugin's reply cannot be read, sefy says so without quoting the reply. A
transport may well print a URL with a token in it, and that message can end up
in a log or an issue.

## Related

- [`merge`](/sefy/reference/merge/) — folding a fetched copy into this vault
- [Moving a vault between machines](/sefy/guides/moving-a-vault/) — doing it by hand today
