//! Guards the facts that must agree before a version is published.
//!
//! sefy ships to three places — GitHub, crates.io and npm — and each renders
//! its own copy of the metadata. They drift silently: nothing fails when
//! `npm/package.json` still says 0.1.0 after the crate moved on, or when the
//! npm page describes the product differently from the crate. The drift only
//! becomes visible after publishing, when it cannot be taken back.
//!
//! These checks run in CI, so a mismatch fails the build instead of shipping.

use std::fs;
use std::path::{Path, PathBuf};

/// Root of the repository, two levels above this crate.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives at crates/sefy-cli")
        .to_path_buf()
}

fn read(path: impl AsRef<Path>) -> String {
    let path = repo_root().join(path);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Extracts a top-level `key = "value"` from a manifest's `[section]`.
///
/// Deliberately naive: it reads one block, which is all these checks need, and
/// avoids adding a TOML parser as a dev-dependency.
fn manifest_field(file: &str, section: &str, key: &str) -> String {
    let manifest = read(file);
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // Stop at the next section: `version` also appears under [lib],
            // [[bin]] and in every dependency.
            if inside {
                break;
            }
            inside = line == section;
            continue;
        }
        if !inside {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        // Exact match, so `rust-version` cannot answer a lookup for `version`.
        if name.trim() != key {
            continue;
        }
        return value
            .trim()
            .trim_end_matches(',')
            .trim()
            .trim_matches('"')
            .to_string();
    }
    panic!("`{key}` not found in the {section} block of {file}");
}

/// The version every published artifact must carry.
fn workspace_version() -> String {
    manifest_field("Cargo.toml", "[workspace.package]", "version")
}

/// Extracts a `"key": "value"` from a JSON file, without a JSON dependency.
fn json_field(file: &str, key: &str) -> String {
    let text = read(file);
    let needle = format!("\"{key}\"");
    let start = text
        .find(&needle)
        .unwrap_or_else(|| panic!("`{key}` not found in {file}"));
    let after = &text[start + needle.len()..];
    let after = after.trim_start().trim_start_matches(':').trim_start();
    let after = after
        .strip_prefix('"')
        .unwrap_or_else(|| panic!("`{key}` in {file} is not a string"));
    after[..after.find('"').expect("unterminated string")].to_string()
}

#[test]
fn npm_package_version_matches_the_crate() {
    let crate_version = workspace_version();
    let npm_version = json_field("npm/package.json", "version");

    assert_eq!(
        npm_version, crate_version,
        "npm/package.json version ({npm_version}) differs from Cargo.toml ({crate_version}); \
         the npm page would advertise a version that was never released"
    );
}

#[test]
fn npm_wrapper_downloads_the_matching_binary() {
    let crate_version = workspace_version();
    let binary_tag = json_field("npm/package.json", "binary");

    assert_eq!(
        binary_tag,
        format!("v{crate_version}"),
        "the npm wrapper points at release {binary_tag} while this is {crate_version}; \
         installing from npm would fetch the wrong binary"
    );
}

#[test]
fn the_product_is_described_the_same_way_everywhere() {
    let crate_description =
        manifest_field("crates/sefy-cli/Cargo.toml", "[package]", "description");
    let npm_description = json_field("npm/package.json", "description");

    assert_eq!(
        npm_description, crate_description,
        "crates.io and npm describe the product differently; \
         the CLI crate's `description` is the single source"
    );
}

#[test]
fn readme_is_shared_rather_than_duplicated() {
    // A second copy under npm/ is what lets the two pages drift apart. The
    // publish workflow copies the root README into npm/ at publish time.
    let duplicate = repo_root().join("npm/README.md");
    assert!(
        !duplicate.exists(),
        "npm/README.md exists again; it will drift from the root README. \
         The publish workflow copies the root one into npm/ instead."
    );
}

#[test]
fn readme_links_resolve_off_github() {
    // The same file is rendered on crates.io and npm, where a relative path has
    // no repository to resolve against: the banner turns into a broken image
    // and the links 404.
    let readme = read("README.md");

    for (line_no, line) in readme.lines().enumerate() {
        for (marker, kind) in [("src=\"", "image"), ("](", "link")] {
            let mut rest = line;
            while let Some(at) = rest.find(marker) {
                let target = &rest[at + marker.len()..];
                let end = if marker == "](" { ')' } else { '"' };
                let target = &target[..target.find(end).unwrap_or(target.len())];

                let relative =
                    !target.starts_with("http") && !target.starts_with('#') && !target.is_empty();
                assert!(
                    !relative,
                    "README line {}: relative {kind} `{target}` breaks on crates.io and npm; \
                     use an absolute URL",
                    line_no + 1
                );

                rest = &rest[at + marker.len()..];
            }
        }
    }
}

#[test]
fn the_unix_installer_redirects_windows_shells() {
    // Field report from kasl, 19.08: run in Git Bash on Windows, the script
    // matched no case arm and answered "No prebuilt binary for MINGW64_NT-…",
    // which reads as "unsupported platform" although a Windows release exists
    // - it is just installed by the other script.
    let installer = read("tools/install.sh");

    for shell in ["MINGW*", "MSYS*", "CYGWIN*"] {
        assert!(
            installer.contains(shell),
            "install.sh does not recognise {shell}; Windows shells fall through to the              generic 'no prebuilt binary' message"
        );
    }
    assert!(
        installer.contains("install.ps1"),
        "install.sh does not name the PowerShell installer, leaving Windows users at a dead end"
    );
}

#[test]
fn installers_name_the_crate_that_actually_exists() {
    // A `cargo install <name>` fallback that does not resolve is worse than
    // none: in kasl the suggestion named `kasl` while the crate is published
    // as `kasl-cli`, so the advice in the error message failed. Here the CLI
    // crate is the published one, not the workspace directory it lives in.
    let crate_name = manifest_field("crates/sefy-cli/Cargo.toml", "[package]", "name");

    for file in ["tools/install.sh", "tools/install.ps1"] {
        let text = read(file);
        for (line_no, line) in text.lines().enumerate() {
            let Some(at) = line.find("cargo install ") else {
                continue;
            };
            // Trim shell quoting around the suggestion, e.g. `... sefy" >&2`.
            let named = line[at + "cargo install ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
            assert_eq!(
                named,
                crate_name,
                "{file} line {} suggests `cargo install {named}`, but the crate is `{crate_name}`",
                line_no + 1
            );
        }
    }
}

#[test]
fn readme_only_shows_commands_that_exist() {
    // Documentation for a command that no longer exists is worse than none: it
    // sends people to an error. Every `sefy <word>` in a console block must
    // name a real subcommand.
    let readme = read("README.md");
    let help = String::from_utf8(
        std::process::Command::new(env!("CARGO_BIN_EXE_sefy"))
            .arg("--help")
            .output()
            .expect("cannot run sefy --help")
            .stdout,
    )
    .expect("help output is not utf-8");

    // Subcommand names are the indented first words in the Commands block.
    let known: Vec<String> = help
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();

    assert!(
        !known.is_empty(),
        "could not parse subcommands out of --help"
    );

    // Only lines typed at a prompt inside a fenced block count. Matching bare
    // "sefy " anywhere would read prose like "sefy is a secret store" as a
    // command and force the text to be written around the test.
    let mut in_block = false;
    for line in readme.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_block = !in_block;
            continue;
        }
        if !in_block {
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("$ sefy ") else {
            continue;
        };
        let Some(word) = rest.split_whitespace().next() else {
            continue;
        };
        if word.starts_with('-') {
            continue; // a flag on the bare binary, e.g. `sefy --version`
        }
        assert!(
            known.contains(&word.to_string()),
            "README shows `sefy {word}`, which is not a command; known: {known:?}"
        );
    }
}

#[test]
fn the_documented_vault_format_version_matches_the_code() {
    // The README promises a stable on-disk format. If the code ever writes a
    // different version, that promise silently becomes false.
    assert_eq!(
        sefy_core::FORMAT_VERSION,
        1,
        "the vault format version changed; the README promises v1 files stay \
         readable, so this needs a migration path and a Breaking Changes entry"
    );
}
