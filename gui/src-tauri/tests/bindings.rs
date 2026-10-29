//! The window's IPC types, generated from the Rust ones that cross it.
//!
//! Every argument and return type of every command is exported with
//! [`ts_rs`], together with everything it depends on, into a temporary
//! directory and compared file by file against the committed
//! `gui/src/ipc/generated/`. A mismatch fails with what changed and this
//! instruction:
//!
//!     SEFY_BLESS=1 cargo test -p sefy-gui --test bindings
//!
//! which rewrites `gui/src/ipc/generated/` to match - deleting it first, so a
//! type that stopped crossing the wire is removed rather than left behind.
//!
//! `#[ts(export)]` is deliberately not used on the types themselves: it would
//! make every `cargo test` write into the tree. This is the one place that
//! exports, and only when asked to.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ts_rs::{Config, TS};

/// Where the frontend keeps its copy.
fn generated_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri always has a parent")
        .join("src/ipc/generated")
}

/// Every type a command takes or answers with; their dependencies follow on
/// their own.
fn export_all(dir: &Path) -> Result<(), ts_rs::ExportError> {
    let config = Config::from_env().with_out_dir(dir);
    sefy_gui::wire::Opening::export_all(&config)?;
    sefy_gui::wire::VaultStatus::export_all(&config)?;
    sefy_gui::wire::Failure::export_all(&config)?;
    sefy_gui::wire::Filter::export_all(&config)?;
    sefy_gui::wire::ItemRow::export_all(&config)?;
    sefy_gui::wire::TagCount::export_all(&config)?;
    sefy_gui::wire::ItemCard::export_all(&config)?;
    sefy_gui::wire::Revealed::export_all(&config)?;
    sefy_gui::wire::Copied::export_all(&config)?;
    sefy_gui::wire::Policy::export_all(&config)?;
    Ok(())
}

/// Every file under `dir`, relative to it, with `\r\n` folded to `\n` so a
/// Windows checkout compares equal to what this process just wrote.
fn read_tree(dir: &Path) -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries {
        let path = entry.expect("a directory entry reads").path();
        if path.is_file() {
            let text = std::fs::read_to_string(&path)
                .expect("a generated file reads")
                .replace("\r\n", "\n");
            out.insert(path.strip_prefix(dir).unwrap().to_path_buf(), text);
        }
    }
    out
}

#[test]
fn the_committed_bindings_are_the_ones_the_types_make() {
    let fresh = tempfile::tempdir().unwrap();
    export_all(fresh.path()).expect("the types export");
    let made = read_tree(fresh.path());
    assert!(
        made.len() >= 3,
        "the export wrote {} files; it is not reaching the types",
        made.len()
    );

    let committed_dir = generated_dir();
    if std::env::var_os("SEFY_BLESS").is_some() {
        let _ = std::fs::remove_dir_all(&committed_dir);
        export_all(&committed_dir).expect("the types export into the tree");
        return;
    }

    let committed = read_tree(&committed_dir);
    let mut differences = Vec::new();
    for (path, text) in &made {
        match committed.get(path) {
            None => differences.push(format!("missing: {}", path.display())),
            Some(theirs) if theirs != text => {
                differences.push(format!("changed: {}", path.display()))
            }
            Some(_) => {}
        }
    }
    for path in committed.keys().filter(|path| !made.contains_key(*path)) {
        differences.push(format!("stale: {}", path.display()));
    }

    assert!(
        differences.is_empty(),
        "gui/src/ipc/generated/ is out of step with the Rust types:\n  {}\n\
         regenerate with: SEFY_BLESS=1 cargo test -p sefy-gui --test bindings",
        differences.join("\n  ")
    );
}
