//! `sefy wipe`: destroying the vault, once its name is typed back.
//!
//! The one command with no way to say yes in advance. Every other confirmation
//! in sefy has a `--yes` for scripts; this one has none, because a wipe in a
//! script is a wipe that runs again the next time the script does, and the
//! line in a shell history is one key away from being repeated.

use crate::commands::{now, size};
use crate::output;
use crate::session;
use crate::when;
use anyhow::{Result, bail};
use sefy_core::wipe::{Part, Role};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

/// What typing at the question came to.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    /// Nothing: the person changed their mind.
    Declined,
    /// The vault's name, exactly.
    Confirmed,
    /// Something else, which is not a yes however close it is.
    Mistyped,
}

/// Reads what was typed against the vault's name.
///
/// Only the name itself goes ahead. Case counts and so does every character:
/// a near miss means the person was not reading what they were destroying.
fn answer(typed: &str, name: &str) -> Answer {
    match typed.trim() {
        "" => Answer::Declined,
        typed if typed == name => Answer::Confirmed,
        _ => Answer::Mistyped,
    }
}

/// Lists everything a wipe would destroy, asks for the vault's name, and
/// destroys it.
pub fn run(path: &Path, password_env: Option<&str>) -> Result<()> {
    // Before the password: a script cannot answer the question, and should
    // learn so without being asked for a password it has no use for.
    if !std::io::stdin().is_terminal() {
        bail!(
            "wipe asks for the vault's name on a terminal, and input is not one\n\
             there is no --yes: what a wipe destroys does not come back"
        );
    }

    let password = session::password(password_env)?;
    let vault = session::open(path, &password)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let parts = sefy_core::wipe::parts(vault.path());
    println!(
        "This overwrites with random bytes and deletes, in {}:",
        directory(path).display()
    );
    print_parts(&parts);
    if let Some(stamp) = vault.last_sync()? {
        println!(
            "Not touched: the remote copy, last reached through {} {}.",
            stamp.transport,
            when::moment(stamp.at, now())
        );
    }
    println!(
        "An SSD, a snapshot, a backup or a synced folder's history can keep\n\
         older blocks of these files out of a wipe's reach: see sefy wipe --help."
    );
    print!("Nothing brings it back. Type the vault's name to wipe it ({name}): ");
    std::io::stdout().flush()?;

    let mut typed = String::new();
    std::io::stdin().read_line(&mut typed)?;
    match answer(&typed, &name) {
        Answer::Declined => {
            println!("nothing was wiped");
            return Ok(());
        }
        Answer::Mistyped => bail!("that is not {name:?}; nothing was wiped"),
        Answer::Confirmed => {}
    }

    let wiped = vault.wipe();
    for part in &wiped.destroyed {
        println!("wiped  {}", file_name(part));
    }
    for part in &wiped.vanished {
        println!(
            "gone   {} (deleted before the wipe reached it)",
            file_name(part)
        );
    }
    if let Some(stamp) = &wiped.last_sync {
        println!(
            "the remote copy is not touched: delete it through {}",
            stamp.transport
        );
    }

    if wiped.complete() {
        return Ok(());
    }
    for (_, error) in &wiped.failed {
        eprintln!("not wiped: {error}");
    }
    bail!(
        "{} still on disk; once a file can be written, wipe it on its own:\n\
         sefy wipe --vault FILE",
        output::count(wiped.failed.len(), "file")
    )
}

/// The directory a vault is in, whole: every file a wipe destroys is there,
/// so it is said once and the files by name.
fn directory(vault: &Path) -> PathBuf {
    let absolute = std::path::absolute(vault).unwrap_or_else(|_| vault.to_path_buf());
    match absolute.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// A part by its name alone, for a listing that has said where.
fn file_name(part: &Part) -> String {
    part.path.file_name().map_or_else(
        || part.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// One line per file: its name, what it is, how large.
fn print_parts(parts: &[Part]) {
    let rows: Vec<(String, String, String)> = parts
        .iter()
        .map(|part| {
            (
                file_name(part),
                describe(part.role),
                part.size.map_or_else(|| "size unknown".to_owned(), size),
            )
        })
        .collect();
    let path_width = rows
        .iter()
        .map(|row| row.0.chars().count())
        .max()
        .unwrap_or(0);
    let role_width = rows
        .iter()
        .map(|row| row.1.chars().count())
        .max()
        .unwrap_or(0);
    for (path, role, size) in rows {
        println!("  {path:path_width$}  {role:role_width$}  {size}");
    }
}

/// What a file is to the vault, as the listing says it.
fn describe(role: Role) -> String {
    match role {
        Role::Vault => "the vault".to_owned(),
        Role::Copy(number) => format!("copy {number}"),
        Role::Unfinished => "an unfinished save".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_name_itself_goes_ahead() {
        assert_eq!(answer("notes.bak\n", "notes.bak"), Answer::Confirmed);
        assert_eq!(answer("  notes.bak \r\n", "notes.bak"), Answer::Confirmed);
    }

    #[test]
    fn a_yes_is_not_the_name() {
        for typed in ["y\n", "yes\n", "Y\n", "wipe\n"] {
            assert_eq!(answer(typed, "notes.bak"), Answer::Mistyped, "{typed:?}");
        }
    }

    #[test]
    fn a_near_miss_is_not_the_name() {
        for typed in ["Notes.bak", "notes.ba", "notes.bak.1", "notes"] {
            assert_eq!(answer(typed, "notes.bak"), Answer::Mistyped, "{typed:?}");
        }
    }

    #[test]
    fn nothing_typed_is_a_no() {
        assert_eq!(answer("\n", "notes.bak"), Answer::Declined);
        assert_eq!(answer("", "notes.bak"), Answer::Declined);
    }

    #[test]
    fn a_vault_with_no_name_cannot_be_confirmed_by_typing_nothing() {
        // `--vault ..` has no file name; an empty name must not turn the
        // empty answer, which means no, into the matching one.
        assert_eq!(answer("\n", ""), Answer::Declined);
    }
}
