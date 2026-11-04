---
title: "lock"
description: Lock the vault now - wipe what the agent and the desktop app hold.
---

Tells whatever holds the password on this machine to wipe it, at once. The
[agent](/sefy/reference/agent/) exits; the [desktop app](/sefy/guides/desktop-app/#locking)
goes back to its Unlock screen, takes back what it put on the clipboard and
closes quick search.

## Usage

```console
$ sefy lock
locked the agent and the window
```

It takes no password and no vault: you may well be walking away. With nothing
holding a password it says so, and that is not an error, so it can go in a
script that runs at the end of the day.

```console
$ sefy lock
nothing was unlocked
```

## Related

- [`agent`](/sefy/reference/agent/) - keep the vault unlocked for a while
- [`status`](/sefy/reference/status/) - whether it is locked
