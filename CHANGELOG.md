# Changelog

All notable changes to this project are documented in this file.

Vault files stay readable across releases. The **file format** is frozen at
version 1; the **database schema** inside the ciphertext is versioned separately
and does move — 0.2.0 added an identity to items, which is migrated in on load.
A vault written by 0.1.x opens in 0.2.0 and remains readable by 0.1.x
afterwards. Any change that would break an existing file gets its own
"Breaking Changes" section here, with the migration path or a plain statement
that there is none. This paragraph survives regenerating the file.

## [0.6.0] - 2026-08-27

Existing vaults are unaffected: the file format is still version 1 and the plugin
protocol is still version 1. What changes is what happens when a vault holds an
item this build does not understand — one written by a newer sefy. Until now a
single such item made `ls`, `find`, `export` and `merge` fail outright, claiming
no item with that id existed while it sat plainly in the file. It is now listed,
searched, retitled, exported and passed over by a merge, and only the operations
that would have to invent its contents are refused.

That matters a release early: it makes adding a kind of item something an older
sefy can survive rather than a breaking change for everyone syncing between
machines.

This release also moves `argon2` from 0.5 to 0.6, its first stable release after
months of candidates. The derived key is unchanged — a vault written by the
published 0.5.0 binary opens under this one and vice versa — and a file from that
binary is now kept in the test suite so any future crypto bump has to prove the
same thing rather than be reasoned about.

### Breaking Changes

Only for code using the `sefy-core` library; the CLI and vault files are
unaffected.

- `ItemKind` gained an `Unknown(String)` variant and is no longer `Copy`; match
  arms must handle it, and `as_str` now takes `&self` and returns `&str`.
- `ItemKind::parse` returns `ItemKind` rather than `Option<ItemKind>`: a name
  this build has not heard of is no longer a failure.
- `Payload` gained an `Unknown { kind }` variant. Writing one into a vault is
  refused with `Error::UnknownItemKind`.
- `Error::ItemKindMismatch` carries `String` fields instead of `&'static str`.
- `ImportReport` and `MergeReport` gained an `unsupported` count, and
  `MergeReport::is_empty` accounts for it.

### Bug Fixes
- An item from a newer sefy no longer breaks the vault it sits in

### Documentation
- Publishing a crate by hand does not connect it to this repository
- Say what holds when two machines run different versions

## [0.5.0] - 2026-08-21

### Documentation
- Note that a new crate's first version goes up by hand

### Features
- A transport that keeps the vault on a server over SSH

## [0.4.0] - 2026-08-21

### Bug Fixes
- Keep both versions when two edits share a timestamp
- Commit as sefy rather than as whoever is at the keyboard

### Documentation
- The landing page lists syncing among what sefy does

### Features
- Move a vault through a transport without opening it to one
- Push, pull and sync a vault through a transport
- A transport that keeps the vault in a git repository

### Refactoring
- One rule for what name addresses a plugin

### Testing
- Find the fixture transport the way a real one is found
- Put the fixture transport where macOS looks for one

## [0.3.1] - 2026-08-19

### Bug Fixes
- Point Windows shells at the PowerShell installer

## [0.3.0] - 2026-08-16

### Documentation
- Changelog and stability note for v0.3.0

### Features
- Let transports carry the vault without seeing it

### Testing
- Make the fixture plugin need nothing on PATH

## [0.2.0] - 2026-08-12

### Bug Fixes
- Declare the MSRV that actually builds
- Collapse a nested if into a let chain
- Give an identity to rows an older build inserted

### Documentation
- Add the Guides section
- Give every command its own page
- Changelog and README for v0.2.0

### Features
- Add one-line installers for Windows and Unix
- Give items an identity and merge two vaults on it

## [0.1.2] - 2026-08-10

### Bug Fixes
- Put the README back in the package

### CI
- Publish on tags again

## [0.1.1] - 2026-08-10

### Bug Fixes
- Publish as sefy-cli, not sefy

## [0.1.0] - 2026-08-09

### Bug Fixes
- Align the labels in sefy show

### Build
- Set up the release pipeline for v0.1.0

### CI
- Run fmt, clippy and tests on Linux, macOS and Windows
- Publish by hand for the first release

### Documentation
- Add MIT license and project readme
- Add lacodda line brand assets and readme banner
- Add the documentation site and brand rasters
- Stop the home page title rendering as "sefy | sefy"

### Features
- Rewrite the vault as a library with modern cryptography
- Resolve items by id, exact title or search text
- Add the sefy command-line tool
- Export and import a vault as plain JSON
- Clear the clipboard after a timeout
- Add export, import and $EDITOR support

### Testing
- Explain why the truncation test uses multi-byte data

