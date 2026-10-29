# Contributing to sefy

## Building

```
cargo build --release   # workspace: sefy-core (library) + sefy (CLI)
cargo test              # unit, integration and doc tests
```

Use a release build for daily work: Argon2id is deliberately expensive, and an
unoptimized build makes it several times slower still.

The tests that put something on the real clipboard are ignored by default -
run on your machine, they would throw away whatever you had copied. CI runs
them on Windows; to run them yourself:

```
cargo test -p sefy-core --features clipboard --test clipboard -- --ignored --test-threads=1
cargo test -p sefy-gui --test clip -- --ignored --test-threads=1
```

The library is published separately as [`sefy-core`](https://crates.io/crates/sefy-core)
if you want vaults from your own code.

On Linux the workspace includes the desktop app, which builds against the
system's webview: install WebKitGTK first (`libwebkit2gtk-4.1-dev` and its
companions, as `.github/workflows/ci.yml` lists them).

## The desktop app

The window is a Tauri app over the same core, in `gui/` - its page in `gui/`,
its Rust in `gui/src-tauri`. Releases carry it as a per-user installer on
Windows, an `.app` on macOS and an AppImage on Linux, which the installers in
`tools/` fetch. To run it from source (Node 22 and pnpm):

```
cd gui
pnpm install
pnpm tauri dev          # the window, rebuilt on every change
pnpm lint               # types, the design system's lint rules, tests
pnpm tauri build --bundles nsis   # the Windows installer, as a release makes it
```

The page's components come from the line's design system,
[dowel](https://lacodda.github.io/dowel/), as copies under
`gui/src/components/ui/`. The types that cross between the page and Rust are
generated; after changing one in `gui/src-tauri/src/wire.rs`:

```
SEFY_BLESS=1 cargo test -p sefy-gui --test bindings
```

Why the window keeps no list of vaults, and how its file dialog is told not to
remember one either: [ADR-0010](docs/adr/0010-the-desktop-app.md). How it
shows an item without its secrets, copies without the page, and what quick
search is allowed to be: [ADR-0011](docs/adr/0011-browsing-copying-and-quick-search.md).

## Repository layout

- `crates/sefy-core` - the vault library: format, cryptography, schema and merge.
- `crates/sefy-cli` - the `sefy` command.
- `crates/sefy-plugin-github`, `crates/sefy-plugin-sftp` - the two transports
  that ship alongside the CLI.
- `gui/` - the desktop app: the page, and `gui/src-tauri` - the `sefy-gui` crate.
- `docs/` - the documentation site (Astro Starlight); architecture decision
  records live in `docs/adr/`.

## Commits

English, [Conventional Commits](https://www.conventionalcommits.org/), no
trailers.

## License

By contributing, you agree that your contributions will be licensed under the
project's [MIT license](LICENSE).
