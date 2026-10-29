---
title: The desktop app
description: The same vault in a window - browse and search it, copy a secret without it touching the screen, and find one from anywhere with quick search.
---

The desktop app is the same vault as the command line, in a window: the same
file, the same core, the same rules about what leaves it. It opens a vault,
lists what is in it, shows one item at a time, and copies a field to the
clipboard. Adding and editing items stays on the command line for now.

## Install

The one-line installers from [Getting Started](/sefy/getting-started/#install)
put the desktop app beside the command line:

- **Windows** - the release's own installer, run silently for your account:
  sefy appears in the Start menu, and the program lands in
  `%LOCALAPPDATA%\sefy\sefy-gui.exe`. No administrator is asked for.
- **macOS** - `sefy.app` in `~/Applications`.
- **Linux** - an AppImage in `~/.local/share/sefy/`, a menu entry, and
  `sefy-gui` on your `PATH`. The AppImage needs FUSE 2 (`libfuse2`).

The program is `sefy-gui`, so it never takes the command line's `sefy` on
`PATH`. Run the installer again to update; a window that is open is left
alone, and the installer says to close it first.

## Opening a vault

There is no list of recent vaults, by design: a list of the files sefy opened,
in a predictable place under your profile, would name exactly the files the
[format](/sefy/concepts/vault-format/) works to make unremarkable. The window
takes the file from:

1. the file it was started on - "Open with", or `sefy-gui notes.bak`;
2. otherwise `SEFY_VAULT`, the variable the command line reads;
3. otherwise the file you drop on the window or pick with **Browse…**.

On Windows the file dialog is told not to add the vault to the system's recent
documents either.

## Finding an item

The rail on the left lists every item and each kind with how many there are,
then the tags. The search box (<kbd>Ctrl</kbd> <kbd>K</kbd>) looks where
[`sefy find`](/sefy/reference/find/) looks - titles, notes and the fields that
are not secret, never a password - and puts the items whose **title** matches
ahead of those that match only inside. <kbd>↓</kbd> steps from the box into
the list, <kbd>Enter</kbd> opens the first match, and the arrows walk the list.

**This vault**, at the foot of the rail, is what
[`sefy status`](/sefy/reference/status/) says: how many items of each kind,
where the file is, how it is sealed, when it last travelled. Never a title.

## An item, opened

The card shows what [`sefy show`](/sefy/reference/show/) prints: the public
fields as they are, a note's text, a file's name and size - and every secret
field **named and hidden**. The page in the window does not receive a secret
at all until you press **Show** beside it, and it forgets it when you press
**Hide**, open another item, or lock.

A one-time password key is shown as the code it makes, with a ring for the
seconds the code has left; the key itself never leaves the vault.

Files stay in the vault: take one out with
[`sefy extract`](/sefy/reference/extract/).

## Copying

**Copy** beside a field puts it on the clipboard without it passing through
the window's page. The button counts down in amber while the secret is out,
and after 45 seconds sefy takes it back off - the same hold as
[`sefy get`](/sefy/reference/get/), and the same rule: only if it is still
what is on the clipboard, so whatever you copied since is left alone.

Every copy is marked as one the clipboard history (<kbd>Win</kbd>
<kbd>V</kbd>) and the cloud clipboard must not keep, and that clipboard
managers should pass by - see [the clipboard](/sefy/reference/get/#the-clipboard-timeout).

## Quick search

<kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>Space</kbd> (<kbd>Cmd</kbd>
<kbd>Shift</kbd> <kbd>Space</kbd> on macOS) brings up a search line over
whatever is on the screen. Type a few letters, and <kbd>Enter</kbd> copies the
item's secret - what `sefy get` would take: a login's password, a card's
number, a note's text - and the line goes away. <kbd>Esc</kbd> or a click
elsewhere puts it away without copying.

- It shows nothing until you type, so bringing it up does not put a list of
  your items on the screen.
- With the vault locked, the shortcut brings the main window forward to unlock
  it instead: the master password is typed in one place only.
- It works while the window is running, minimized or not. Closing the window
  closes sefy, and the shortcut with it.
- If another program already holds the keys, the status line at the foot of
  the window says so, and the window works without quick search.

## Locking

**Lock** closes the vault in the window's process, takes back whatever sefy
put on the clipboard, and closes quick search - which has titles in it, so it
is destroyed rather than hidden. Closing the window does the same.

## What the window does not protect against

The [threat model](/sefy/concepts/threat-model/) is the command line's, with
two additions: an open window shows titles to whoever is looking at the
screen, and a secret you **Show** is a string in the window's page from then
until the page is gone - something no program can wipe from a webview's
memory. That is why a secret is shown only when you ask, and copied without
being shown at all.

## Related

- [`get`](/sefy/reference/get/) - the command line's copy, and the clipboard rules
- [`status`](/sefy/reference/status/) - what **This vault** shows
- [Threat model](/sefy/concepts/threat-model/)
