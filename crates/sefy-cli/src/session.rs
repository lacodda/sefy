//! Getting to an open vault: where the file is, and what the password is.

use anyhow::{Context, Result, anyhow, bail};
use sefy_core::Vault;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// Resolves which vault file to work on.
///
/// The path comes from `--vault` or `$SEFY_VAULT`, and there is no default. A
/// conventional location like `~/.sefy/vault` would undo the point of a file
/// that looks like nothing in particular.
pub fn vault_path(argument: Option<PathBuf>) -> Result<PathBuf> {
    argument.ok_or_else(|| {
        anyhow!(
            "no vault given\n\
             pass --vault <FILE>, or set SEFY_VAULT to the path of your vault.\n\
             sefy has no default location on purpose: a predictable path would\n\
             give the file away."
        )
    })
}

/// Reads the master password of an existing vault.
pub fn password(password_env: Option<&str>) -> Result<String> {
    if let Some(variable) = password_env {
        return from_env(variable);
    }
    prompt("Master password: ")
}

/// Reads a new master password, asking twice so a typo cannot lock the vault.
pub fn new_password(password_env: Option<&str>) -> Result<String> {
    if let Some(variable) = password_env {
        return from_env(variable);
    }

    let first = prompt("New master password: ")?;
    let second = prompt("Repeat it: ")?;
    if first != second {
        bail!("the two passwords do not match");
    }
    Ok(first)
}

/// Reads a secret that belongs *inside* the vault, such as an account password.
pub fn secret(prompt_text: &str, password_env: Option<&str>) -> Result<String> {
    if let Some(variable) = password_env {
        return from_env(variable);
    }
    prompt(prompt_text)
}

fn from_env(variable: &str) -> Result<String> {
    std::env::var(variable).with_context(|| format!("cannot read the password from ${variable}"))
}

/// Asks for a password without echoing it.
///
/// The prompt talks to the terminal itself (`/dev/tty`, `CONIN$`), not to
/// stdin, so a command whose input is piped can still ask: `sefy run` hands
/// its stdin to the command it starts, and `add note` reads the note from it.
/// What it refuses is asking when no stream is a terminal — a script or a
/// service, where nobody is there to answer and a prompt would be a hang.
fn prompt(text: &str) -> Result<String> {
    let someone_is_there = std::io::stdin().is_terminal()
        || std::io::stdout().is_terminal()
        || std::io::stderr().is_terminal();
    if !someone_is_there {
        bail!(
            "cannot ask for a password: sefy is not attached to a terminal\n\
             use --password-env <VAR> to pass it through the environment."
        );
    }
    rpassword::prompt_password(text).context("cannot read the password")
}

/// Opens an existing vault, turning a wrong password into a clear message.
pub fn open(path: &Path, password: &str) -> Result<Vault> {
    match Vault::open(path, password.as_bytes()) {
        Ok(vault) => Ok(vault),
        Err(sefy_core::Error::WrongPasswordOrNotAVault) => bail!(
            "wrong password, or {} is not a vault\n\
             (an encrypted file cannot tell the two apart)",
            path.display()
        ),
        Err(other) => Err(other.into()),
    }
}
