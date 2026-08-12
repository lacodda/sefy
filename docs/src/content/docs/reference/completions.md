---
title: "completions"
description: Print a shell completion script.
---

Prints a completion script for `bash`, `zsh`, `fish`, `powershell` or `elvish`.

## Usage

```sh
sefy completions <SHELL>
```

```sh
sefy completions bash > /etc/bash_completion.d/sefy
sefy completions zsh  > ~/.zfunc/_sefy
```

The script goes to stdout, so where it lands is your shell's business — the
paths above are the usual ones, not something sefy enforces.

## What it completes

Commands, subcommands and flags. **Item titles and tags are not completed**: the
vault would have to be decrypted to know them, which means asking for the master
password every time you press Tab.

## Related

- [Commands](/sefy/reference/commands/) — the full surface these scripts cover
