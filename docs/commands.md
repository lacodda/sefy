# Commands

Every command works on one vault file. sefy has **no default location**: pass
`--vault <FILE>` or set `SEFY_VAULT`. A vault at a predictable path like
`~/.sefy/vault` would undo the point of a file that looks like nothing.

```sh
export SEFY_VAULT=~/backups/notes.bak
```

The master password is asked for on the terminal, without echo. For scripts,
`--password-env <VAR>` reads it from an environment variable instead. A password
cannot be passed as an argument: it would land in the shell history and in every
process listing.

---

## `sefy init`

Creates a new vault. Asks for the password twice, so a typo cannot lock you out
of an empty vault forever.

```sh
sefy --vault ~/backups/notes.bak init
```

Refuses to touch a path that already exists.

---

## `sefy add`

Adds an item. Three kinds, three subcommands.

### `sefy add note <TITLE>`

| Option | Meaning |
| --- | --- |
| `-t, --text <TEXT>` | The note body. Omit to read it from stdin. |
| `-e, --editor` | Write the note in `$EDITOR` instead. |
| `--tag <TAG>` | Tags; repeat the flag or separate with commas. |

```sh
sefy add note "bank" --text "vault code 4815" --tag money,home
pbpaste | sefy add note "meeting notes"
sefy add note "journal" --editor
```

### `sefy add credential <TITLE>`

| Option | Meaning |
| --- | --- |
| `-l, --login <LOGIN>` | Username, email, whatever the service calls it. Required. |
| `-u, --url <URL>` | Where the account lives. |
| `--totp <SECRET>` | Shared secret for one-time passwords. |
| `--notes <TEXT>` | Anything else worth remembering. |
| `--item-password-env <VAR>` | Read the account password from this variable instead of prompting. |
| `--tag <TAG>` | Tags. |

The account password is prompted for separately. Note that
`--item-password-env` is deliberately distinct from the global
`--password-env`: with one variable for both, the master password would end up
stored as the account's password.

```sh
sefy add credential "mail" --login someone --url https://example.com --tag mail
```

### `sefy add file <PATH>`

| Option | Meaning |
| --- | --- |
| `-T, --title <TITLE>` | What to call it; defaults to the file name. |
| `--tag <TAG>` | Tags. |

Contents are stored byte for byte and come back identical.

```sh
sefy add file ~/.ssh/id_ed25519 --tag keys
```

---

## `sefy get <REFERENCE>`

Copies a secret **to the clipboard**, so it does not end up in the terminal
scrollback.

| Option | Meaning |
| --- | --- |
| `--field <FIELD>` | For credentials: `password` (default), `login`, `url`, `totp`. |
| `--stdout` | Print the secret instead of copying it. |
| `--clear-after <SECONDS>` | Clear the clipboard again after this long. Default `45`; `0` leaves it. |

```sh
sefy get bank                       # to the clipboard, cleared after 45s
sefy get mail --field login
sefy get bank --clear-after 0       # leave it there
sefy get bank --stdout | wl-copy    # for pipes and scripts
```

`--stdout` is what scripts want, but the secret then lives in the scrollback and
— if the command is recalled — in the shell history.

### The clipboard timeout

sefy waits for the timeout before exiting, so the command sits there until the
secret is taken back off. It clears the clipboard **only if the secret is still
what is on it** — anything copied in the meantime is left alone.

On Linux this works differently, because X11 and Wayland make the *owning
process* serve the clipboard: sefy keeps serving the value for the timeout and
then lets go, so the secret disappears when sefy exits either way. There
`--clear-after 0` means "hold it for a long while" rather than "leave it
forever", since letting go immediately would make the value unpastable.

For a stored file, `get` points you at `sefy extract` instead.

### References

Wherever a command takes a `<REFERENCE>`, it accepts:

1. an **id** — `sefy get 7`;
2. an **exact title**, case-insensitive — `sefy get bank`;
3. **text to search for**, matched against titles, note bodies and credential
   fields — `sefy get grocer`.

An exact title always beats a substring. If more than one item still matches,
sefy lists the candidates rather than guessing:

```
$ sefy get mail
error: 2 items match "mail":
     3  mail — personal                 credential
     7  mail — work                     credential
narrow the text, or use an id
```

---

## `sefy show <REFERENCE>`

Prints an item's surroundings — title, kind, tags, login, URL, file size —
without revealing its secret fields. `sefy get` remains the only way a secret
leaves the vault.

---

## `sefy ls`

Lists items, newest first.

| Option | Meaning |
| --- | --- |
| `--kind <KIND>` | `note`, `credential` or `file`. |
| `--tag <TAG>` | Keep only items carrying **every** listed tag. |

## `sefy find [TEXT]`

The same listing, narrowed by text. Titles, note bodies and credential fields
are searched; the contents of stored files are not — a match inside a binary
would say nothing useful and would mean scanning every attachment.

```sh
sefy find bank --kind credential
sefy ls --tag money,home
```

---

## `sefy edit <REFERENCE>`

Changes one item. Flags meant for another kind of item are an error rather than
a silent no-op.

| Option | Applies to | Meaning |
| --- | --- | --- |
| `--title <TITLE>` | all | New title. |
| `-t, --text <TEXT>` | notes | New body. |
| `-e, --editor` | notes | Open the current body in `$EDITOR`. |
| `-l, --login <LOGIN>` | credentials | New login. |
| `--password` | credentials | Prompt for a new account password. |
| `--item-password-env <VAR>` | credentials | Take the new account password from this variable. |
| `-u, --url <URL>` | credentials | New URL. |
| `--totp <SECRET>` | credentials | New TOTP secret. |
| `--notes <TEXT>` | credentials | New notes. |
| `--tag <TAG>` | all | Replace the item's tags. |
| `--clear-tags` | all | Remove every tag. |

An item's **kind cannot change**: a note stays a note for its lifetime.

### Editing in `$EDITOR`

`--editor` opens the note in `$VISUAL`, or `$EDITOR` if that is unset; a value
carrying its own arguments (`EDITOR="code --wait"`) works. There is no built-in
default — with none set, sefy says so rather than opening something you did not
ask for.

While the editor is open, the note sits in a temporary file **in the clear**.
sefy overwrites and deletes that file as soon as the editor exits, but an
editor's own swap, undo and backup files are its business and outside sefy's
reach. If that matters for a particular note, use `--text`.

---

## `sefy export`

Writes the whole vault out as plain, **unencrypted** JSON. This exists so a
vault is never a trap: contents can be migrated, kept in another form, or moved
to a different tool.

| Option | Meaning |
| --- | --- |
| `-o, --output <PATH>` | Where to write it; omit to print to stdout. |
| `--i-know-this-writes-plaintext` | Required. |
| `--force` | Overwrite the destination if it exists. |

```sh
sefy export --i-know-this-writes-plaintext -o backup.json
sefy export --i-know-this-writes-plaintext | gpg -c > backup.json.gpg
```

The acknowledgement flag is required rather than a printed warning: a warning
arrives after the file is already on disk, and scripts do not read them at all.
The resulting file is exactly as sensitive as the vault and protects nothing.

## `sefy import [PATH]`

Adds the contents of an export to this vault, reading stdin when no path is
given.

Items are **appended, never merged**: importing into a vault that already holds
them produces duplicates rather than overwriting anything. Merging would need an
identity for items that the format does not carry, and silently replacing
someone's secrets is worse than a visible duplicate.

The whole file is checked before anything is inserted, so a malformed entry
halfway down cannot leave a half-imported vault behind.

### Format

```json
{
  "sefy_export": 1,
  "items": [
    { "title": "bank", "kind": "note", "tags": ["money"], "text": "code 4815" },
    { "title": "mail", "kind": "credential", "login": "someone",
      "password": "…", "url": "…", "totp": "…", "notes": "…" },
    { "title": "key", "kind": "file", "filename": "id_ed25519",
      "bytes_base64": "…" }
  ]
}
```

Notes need `text`; credentials need `login` and `password`; files need
`filename` and `bytes_base64`. Everything else is optional. This is a plain
enough shape to generate from another tool by hand.

---

## `sefy rm <REFERENCE>`

Removes an item, asking for confirmation first. `-y, --yes` skips the question;
without a terminal, sefy refuses rather than assuming yes.

---

## `sefy extract <REFERENCE>`

Writes a stored file back to disk.

| Option | Meaning |
| --- | --- |
| `-o, --output <PATH>` | Where to write it; defaults to the stored file name in the current directory. |
| `--force` | Overwrite the destination if it exists. |

---

## `sefy tags`

Lists the tags in use with the number of items carrying each. Tags with no items
left are dropped automatically.

---

## `sefy change-password`

Replaces the master password and rewrites the file under it, with a fresh salt
and nonce — the new file shares nothing with the old one.

| Option | Meaning |
| --- | --- |
| `--new-password-env <VAR>` | Read the **new** password from this variable. |

```sh
sefy --password-env OLD change-password --new-password-env NEW
```

---

## `sefy completions <SHELL>`

Prints a completion script for `bash`, `zsh`, `fish`, `powershell` or `elvish`.

```sh
sefy completions bash > /etc/bash_completion.d/sefy
sefy completions zsh  > ~/.zfunc/_sefy
```

---

## Environment

| Variable | Meaning |
| --- | --- |
| `SEFY_VAULT` | Path of the vault to work on, when `--vault` is not given. |

Password variables are never fixed names — you name them yourself and point
sefy at them with `--password-env`, `--item-password-env` or
`--new-password-env`.

## Exit status

`0` on success, `1` on any error. Errors go to stderr; a wrong password and a
file that is not a vault produce the same message, because an authenticated
blob genuinely cannot tell the two apart.
