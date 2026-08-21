//! Running git, and saying something useful when it is not there.

use std::path::Path;
use std::process::Command;

/// What went wrong while running git.
#[derive(Debug)]
pub enum Error {
    /// git is not installed, or not on PATH.
    NotInstalled,
    /// git ran and refused.
    Failed {
        /// What was being attempted, for the message.
        what: String,
        /// git's own stderr, trimmed.
        reason: String,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => formatter.write_str(
                "git is not installed, or not on PATH\n\
                 this transport carries the vault with git, so it needs one",
            ),
            Self::Failed { what, reason } => {
                write!(formatter, "{what}: {reason}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Runs git in `directory` and returns its stdout.
///
/// The command's stderr is only quoted back on failure, and only what git
/// itself printed: a remote URL can carry a token, so nothing is echoed that
/// this program constructed.
pub fn run(directory: Option<&Path>, arguments: &[&str], what: &str) -> Result<String, Error> {
    let mut command = Command::new("git");
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    command.args(arguments);

    // Otherwise git may open a credential prompt, and a plugin runs without a
    // terminal to answer it on — the call would hang rather than fail.
    command.env("GIT_TERMINAL_PROMPT", "0");

    let output = match command.output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NotInstalled);
        }
        Err(error) => {
            return Err(Error::Failed {
                what: what.to_owned(),
                reason: error.to_string(),
            });
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(Error::Failed {
            what: what.to_owned(),
            reason: if stderr.is_empty() {
                format!("git exited with {}", output.status)
            } else {
                stderr
            },
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failing_git_command_carries_what_git_said() {
        let directory = tempfile::tempdir().unwrap();

        // Not a repository, so git refuses with something of its own.
        let error = run(Some(directory.path()), &["log"], "reading the history").unwrap_err();

        let message = error.to_string();
        assert!(message.contains("reading the history"), "got: {message}");
        assert!(
            message.to_lowercase().contains("repository"),
            "git's own reason must survive: {message}"
        );
    }

    #[test]
    fn a_successful_command_gives_back_its_output() {
        let directory = tempfile::tempdir().unwrap();
        run(Some(directory.path()), &["init"], "creating").unwrap();

        // `rev-parse HEAD` would fail here: a fresh repository has no commit
        // for HEAD to point at, and this test is about a command succeeding.
        let top = run(
            Some(directory.path()),
            &["rev-parse", "--show-toplevel"],
            "reading the working tree",
        )
        .unwrap();

        assert!(!top.is_empty());
    }
}
