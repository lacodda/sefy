# Contributing to sefy

## Building

```
cargo build --release   # workspace: sefy-core (library) + sefy (CLI)
cargo test              # unit, integration and doc tests
```

Use a release build for daily work: Argon2id is deliberately expensive, and an
unoptimized build makes it several times slower still.

The library is published separately as [`sefy-core`](https://crates.io/crates/sefy-core)
if you want vaults from your own code.

## Repository layout

- `crates/sefy-core` - the vault library: format, cryptography, schema and merge.
- `crates/sefy-cli` - the `sefy` command.
- `crates/sefy-plugin-github`, `crates/sefy-plugin-sftp` - the two transports
  that ship alongside the CLI.
- `docs/` - the documentation site (Astro Starlight); architecture decision
  records live in `docs/adr/`.

## Commits

English, [Conventional Commits](https://www.conventionalcommits.org/), no
trailers.

## License

By contributing, you agree that your contributions will be licensed under the
project's [MIT license](LICENSE).
