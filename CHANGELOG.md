# Changelog

All notable changes to this project are documented in this file.

Vault files stay readable across releases. The **file format** is frozen at
version 1; the **database schema** inside the ciphertext is versioned separately
and does move — 0.2.0 added an identity to items, which is migrated in on load.
A vault written by 0.1.x opens in 0.2.0 and remains readable by 0.1.x
afterwards. Any change that would break an existing file gets its own
"Breaking Changes" section here, with the migration path or a plain statement
that there is none. This paragraph survives regenerating the file.

## [0.4.0] - 2026-08-21

The vault file format is untouched (still version 1), and so is the plugin
protocol (still version 1). One behaviour did change: when two copies of an item
carry the *same* timestamp and differ, the merge now keeps both instead of
letting the incoming one win. Timestamps are whole seconds, so a tie is ordinary
rather than exotic once a sync runs shortly after edits on two machines — and
the previous behaviour discarded one of them silently.

### Bug Fixes
- Keep both versions when two edits share a timestamp

### Features
- Move a vault through a transport without opening it to one
- Push, pull and sync a vault through a transport
- A transport that keeps the vault in a git repository

### Refactoring
- One rule for what name addresses a plugin

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

