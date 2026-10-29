# ADR-0010: The desktop app

- **Status:** accepted
- **Date:** 2026-10-22

## Context

By 0.16 sefy was a command line that did everything the GUI mockup of
2026-08-12 draws. The next stage starts the window: Tauri v2 with the line's
design system, dowel (decided with the owner on 2026-09-11, without another
round of framework research — kilna, scheda and furca already run on it).

A window over a secret store has more ways to leak than a command line has,
and most of them are defaults: a webview remembers what is typed into forms,
a file dialog remembers what it opened, a list of recent vaults remembers
where they are. Each default is reasonable for a document editor and wrong for
a file whose one property is that nothing points at it. So the first version
of the window decides where its boundaries are before it has anything to show
inside them.

## Decision

### The window is a thin layer over the same core

The crate is `sefy-gui` (`gui/src-tauri`), a workspace member beside the CLI,
with its page in `gui/`. It links `sefy-core` and holds the open vault in its
own process. The page is never handed the database: it asks for a file to be
chosen, a vault to be unlocked or locked, and what the open vault is, and every
answer is a type in `wire.rs` built from what the core already says.

Where the page needed something the CLI had been assembling itself, the core
grew it instead of the window repeating it:

- `Vault::status()` → `Status`: the file, its size, the counts, the copies
  beside it, how it is sealed. `sefy status` prints the same value.
- `format::SEALING`: the key derivation and its cost, the cipher, the layout,
  read from the constants the derivation runs on. The window's passport of the
  file and `sefy status`'s `sealed` line cannot describe one thing while the
  code does another.
- `clipboard::HOLD_SECONDS`: one clipboard lifetime for every front end.

The TypeScript side of the wire is generated from the Rust types with ts-rs
(`gui/src/ipc/generated/`), and a test fails when the committed copy and the
types disagree, as kilna does.

### No list of vaults, anywhere

The mockup's Unlock screen had a *Recent* list kept in the application's data
directory. It is not built. A list of the files sefy has opened, in a
predictable place under the user's profile, names exactly the files the format
spends its effort making unremarkable — to anyone who looks there, and to a
backup or a scanner that knows where sefy keeps its things. The plugins
directory is in the same place and says only that sefy is installed; a recent
list would say which file is the vault.

The window offers a file three ways, none of which writes anything down: the
file it was started on (the first argument, as "Open with" passes it), else
`SEFY_VAULT` — the path the person has already chosen to keep in their
environment for the CLI — else the last file opened while the window stays
open. A file can also be dropped on the window.

### The file dialog is told not to remember

The system's dialog keeps its own history. On Windows the common item dialog
records the file it opened in `ComDlg32\OpenSavePidlMRU` (per extension and in
`*`), which the next open dialog of any program offers back, and it records
under the program's name that the program used it, with the dialog's size
(`CIDSizeMRU`). Measured on Windows 11 by driving the dialog with window
messages and reading the registry and the Recent folder before and after:

| What the dialog is given | What the registry holds afterwards |
| --- | --- |
| `FOS_DONTADDTORECENT`, client id, `ClearClientData` | nothing |
| the same without `FOS_DONTADDTORECENT` | the file in `OpenSavePidlMRU\*` and `\dat` |
| the same without the client id | `sefy-gui.exe` in `CIDSizeMRU` |

No shortcut appeared in the shell's Recent folder in any of the three runs;
`FOS_DONTADDTORECENT` is documented as keeping the file out of it as well.

So on Windows the window calls the common item dialog itself (`pick.rs`, the
one module in the crate that allows `unsafe`, for COM) with all three. Tauri's
dialog plugin offers none of these switches; it remains the dialog on macOS and
Linux, where the system's file chooser may keep its own recent list — not
measured there, and not claimed.

There is no filter and no extension in the dialog: a vault is any file.

### The webview remembers nothing typed into it

`generalAutofillEnabled: false` on the window: WebView2's form autofill keeps
what it saw typed in a database under the user's profile. Password autosave is
off in WebView2 by default and Tauri leaves it so; the password field also
says `autocomplete="off"`. The master password does cross the page — it is
typed there — and lives in the webview's memory until the input is cleared,
which is the moment the request leaves; on the Rust side it is held in
`Zeroizing` and dropped with the attempt. The IPC message that carried it is
the webview's, and sefy cannot wipe it; that is said here rather than
promised away.

### The page can reach nothing but the window

The capability grants the page the window's own controls and nothing else: no
file system, no dialog, no shell, no opener, no network. The CSP loads nothing
from anywhere but the bundle and the IPC bridge. Reload and print are taken
from the keyboard: a reload would start the page over with a vault still open
behind it, so a page that starts locks whatever is open; a print would put the
screen in a spooler's files.

### The splash and the frame are dowel's

The splash (static in `index.html`, then dowel's `Splash`) and the frameless
title bar (dowel's `TitleBar`, `ResizeEdges`) come from the registry, as in
every desktop product of the line. The splash says the product's name, promise
and version and never which vault. It is taken down before the Unlock screen —
typing a password is not loading — and comes back while a password is being
stretched, but only once that has taken 250 ms: on a fast machine unsealing
takes a tenth of a second, and a splash that blinked for that long reads as a
fault.

### Not released yet

The window ships when it can show what is in a vault (0.18). Until then it is
built and tested in CI on all three systems and run from source; it is not in
the release archives, not on crates.io (`publish = false`, held by a gate), and
the MSRV promise stays the CLI's 1.88 — the window declares Tauri's 1.90.

*Kept as decided:* the window shipped in 0.18 as an application bundle - how
is [ADR-0011](0011-browsing-copying-and-quick-search.md#the-window-ships).

## Consequences

- Opening a vault in the window costs a Browse or a drop each time the window
  starts, unless it is started on a file or `SEFY_VAULT` is set. That is the
  price of there being no record of where vaults are.
- The Windows dialog is sefy's own code and must be kept working by hand
  across Windows versions; the live check is the registry read above.
- The frame and splash are copies of dowel's registry items; a fix there is a
  re-add here.
