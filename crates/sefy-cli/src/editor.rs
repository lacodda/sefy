//! Editing a note in the user's own editor.
//!
//! This is a deliberate, narrow exception to sefy's "no plaintext on disk"
//! rule. While the editor is open, the note lives in a temporary file in the
//! clear; sefy overwrites and removes it the moment the editor exits, but an
//! editor's swap, undo and backup files are its own business and outside
//! sefy's reach. The alternative — a home-grown text editor inside the CLI —
//! would be worse in every way that matters.

use anyhow::{Context, Result, bail};
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::Command;

/// Opens `initial` in the user's editor and returns what they saved.
pub fn edit(initial: &str) -> Result<String> {
    let editor = editor_command()?;

    // An editor with nowhere to draw either blocks forever or opens a window
    // nobody asked for; both are worse than saying so.
    if !std::io::stdin().is_terminal() {
        bail!(
            "cannot open an editor: input is not a terminal\n\
             pass the text with --text instead"
        );
    }

    let directory = tempfile::Builder::new()
        .prefix("sefy-")
        .tempdir()
        .context("cannot create a temporary directory for the editor")?;
    let path = directory.path().join("note.txt");

    std::fs::write(&path, initial).with_context(|| format!("cannot write {}", path.display()))?;

    let status = spawn(&editor, &path)?;
    if !status.success() {
        // Leaving the draft behind would strand plaintext on disk, so it is
        // scrubbed on this path too, by the shred below.
        shred(&path);
        bail!("{editor} exited with {status}; nothing was changed");
    }

    let edited = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {} back", path.display()))?;
    shred(&path);

    Ok(edited.trim_end_matches(['\n', '\r']).to_owned())
}

/// Which editor to run: `$VISUAL`, then `$EDITOR`.
///
/// There is no platform default on purpose. Falling back to `notepad` on
/// Windows meant that a script with no editor configured silently opened a
/// window and waited forever for a human — which is exactly what happened the
/// first time this ran unattended.
fn editor_command() -> Result<String> {
    for variable in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(variable) {
            let value = value.trim();
            if !value.is_empty() {
                return Ok(value.to_owned());
            }
        }
    }

    bail!(
        "no editor configured\n\
         set $EDITOR (or $VISUAL), or pass the text with --text"
    )
}

/// Runs the editor, honouring an $EDITOR that carries its own arguments.
fn spawn(editor: &str, path: &Path) -> Result<std::process::ExitStatus> {
    // `EDITOR="code --wait"` is common enough that treating the variable as a
    // bare program name would break real setups.
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or(editor);

    Command::new(program)
        .args(parts)
        .arg(path)
        .status()
        .with_context(|| format!("cannot run {program}"))
}

/// Overwrites a draft before deleting it.
///
/// On a journalling or copy-on-write filesystem this does not guarantee the
/// old bytes are gone, and it is not meant to: it is a cheap way to avoid
/// leaving an obvious plaintext file behind, not a secure-erase claim.
fn shred(path: &Path) {
    if let Ok(metadata) = std::fs::metadata(path) {
        if let Ok(mut file) = std::fs::File::create(path) {
            let _ = file.write_all(&vec![0u8; metadata.len() as usize]);
            let _ = file.sync_all();
        }
    }
    let _ = std::fs::remove_file(path);
}
