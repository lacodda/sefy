# ADR-0011: Browsing, copying and quick search in the window

- **Status:** accepted
- **Date:** 2026-10-28

## Context

0.17 opened a vault in a window and described it in shape. 0.18 makes the
window useful: the items listed and searched, one opened, a field copied -
and, from anywhere on the desktop, a search line whose Enter puts a password
on the clipboard. The window also ships, as an application, for the first
time.

Each of these is a place where a secret could reach something that keeps it:
a page that cannot wipe a string, a clipboard history that outlives the
clipboard, a hidden window that still holds what it showed.

## Decision

### A card names every secret and carries none

The page is told about an item what `sefy show` prints: the public fields,
a note's text, a file's name and size, and each secret field by name only
(`wire::CardField::Secret`). A one-time password key is `CardField::Code` - its
period, never the key. A secret value crosses into the page in one wire type,
`Revealed`, and only when the person presses Show beside that field; the page
keeps it in the field's own state, which a second press, another item or a
lock throws away. Once in the page it is a string in a webview, which nothing
can wipe - which is why showing is the one way in, and copying does not use it.

### Copy happens in the process, never through the page

`copy(id, field)` resolves the value in Rust and puts it on the clipboard from
there; the page gets back which field and for how long. With no field it takes
what `sefy get` takes. A one-time password field gives the code it makes now -
what `sefy fill` hands over - and never the key.

### One clipboard, in the core

The clipboard mechanism moved from the CLI to `sefy_core::clipboard`, behind a
`clipboard` feature so a program that only reads vaults links no desktop
clipboard. Every copy, from either front end, is **marked as a secret**: on
Windows with all three of `ExcludeClipboardContentFromMonitorProcessing`,
`CanIncludeInClipboardHistory = 0` and `CanUploadToCloudClipboard = 0`; on
macOS and Linux with the marker their clipboard managers read. Before this a
`sefy get` on Windows went into the Win+V history and the cloud clipboard, and
stayed there after "clipboard cleared". Taking back is only ever of what is
still ours.

The window works the clipboard from one thread of its own (`clip.rs`): on
Windows the clipboard is one object for the desktop that two threads must not
work at once, and on X11 and Wayland whoever put the value there serves it, so
the handle has to outlive the copy. That thread holds at most one value, takes
it back after `HOLD_SECONDS`, at lock, and when the window closes - a closed
window on Windows would otherwise leave its last copy on the clipboard for
good.

The tests that check the marks and the taking back use the real clipboard. They
are `#[ignore]`d - run locally they would throw away what the person had copied
- and CI runs them on its Windows runner.

### Quick search

A global shortcut, `Ctrl+Alt+Space` (`Cmd+Shift+Space` on macOS, where
Control+Option+Space switches input sources), registered from Rust; no
capability lets the page claim keys. Pressed over an open vault it shows a
small window with a search line; over a locked one it brings the main window
forward, so the master password is typed in one place only.

- **The window exists only while a vault is open.** It is configured with
  `create: false`, made at unlock and *destroyed* at lock, so no title found in
  it outlives the lock in a hidden page.
- **It shows nothing until something is typed.** Brought up with someone else
  looking, it should not open on a list of what the vault holds.
- **Enter copies and the window goes; a click elsewhere puts it away.**
- **One fixed chord, no setting.** sefy keeps no configuration file. If another
  program holds the keys, the main window's status line says so and the window
  works without quick search. A way to choose the keys waits for a second
  person to need one.
- **It lives as long as the window.** Closing the main window exits the
  program (the hidden quick window would otherwise keep it running), and the
  shortcut goes with it. A resident mode belongs with the agent of 0.19.

### Search order

The core's search, the same as `sefy find`, lists newest first. With text
typed, the window puts titles that match ahead of matches inside a note or a
field - whole title, start of the title, start of a word, anywhere in the
title, then the rest - so `mail` puts the item called `mailbox` above a note
that mentions mail. This is the window's presentation, not the core's order:
`sefy find` keeps its own.

### The window ships

`release.yml` builds the bundles with `tauri build`: a per-user NSIS installer
on Windows (the `.msi` needs an administrator), `sefy.app` as a tarball on
macOS, an AppImage on Linux. CI builds the same bundles on every push, so a
packaging step breaks before the tag rather than after the crates went out.
The installers in `tools/` run them: silently for the current user on Windows,
checking the installed file's version rather than the exit code; `.app` into
`~/Applications`; the AppImage into sefy's data directory with a menu entry.
A running window is left alone - it holds an unlocked vault.

The executable inside is `sefy-gui` (`mainBinaryName`), not the product name:
on Linux and macOS a bundle named `sefy` would put a second `sefy` on `PATH`
beside the command line.

## Consequences

- A `sefy get` on Windows no longer lands in the clipboard history or the cloud
  clipboard. A clipboard watcher that ignores the marks still sees it for the
  45 seconds, as the threat model says.
- `sefy-core` has a new error variant, `Error::Clipboard`, and an optional
  dependency on arboard.
- The window is read-only: adding and editing items stay on the command line
  until a later version.
- `release_consistency` holds the bundle names, the binary name, CI's bundle
  build and both installers to each other.
