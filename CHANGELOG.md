# Changelog

All notable changes to this project are documented in this file.

Vault files stay readable across releases. The **file format** is frozen at
version 1; the **database schema** inside the ciphertext is versioned separately
and does move — 0.2.0 added an identity to items, which is migrated in on load.
A vault written by 0.1.x opens in 0.2.0 and remains readable by 0.1.x
afterwards. Any change that would break an existing file gets its own
"Breaking Changes" section here, with the migration path or a plain statement
that there is none. This paragraph survives regenerating the file.

## [0.2.0] - 2026-08-12

### Bug Fixes
- Declare the MSRV that actually builds
- Collapse a nested if into a let chain
- Give an identity to rows an older build inserted

### Documentation
- Add the Guides section
- Give every command its own page

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

