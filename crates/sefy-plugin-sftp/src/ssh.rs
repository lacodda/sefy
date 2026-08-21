//! Running OpenSSH, and saying something useful when it is not there.

use std::process::Command;

/// What went wrong while running ssh or scp.
#[derive(Debug)]
pub enum Error {
    /// The program is not installed, or not on PATH.
    NotInstalled(&'static str),
    /// It ran and refused.
    Failed {
        /// What was being attempted, for the message.
        what: String,
        /// The program's own stderr, trimmed.
        reason: String,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled(program) => write!(
                formatter,
                "{program} is not installed, or not on PATH\n\
                 this transport carries the vault with OpenSSH, so it needs one.\n\
                 Windows 10 and 11 ship it; on Linux and macOS it is the openssh \
                 client package"
            ),
            Self::Failed { what, reason } => write!(formatter, "{what}: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

/// Options every call carries.
///
/// `BatchMode` is the important one: a plugin runs with no terminal, so a
/// password or passphrase prompt would hang the call rather than fail it. This
/// transport authenticates the way the machine already does — a key, an agent,
/// whatever `ssh` is configured with — and says so plainly when that is not set
/// up, instead of waiting for an answer nobody can give.
fn batch_options() -> Vec<&'static str> {
    vec!["-o", "BatchMode=yes"]
}

/// Runs `ssh <destination> <command>` and returns its stdout.
///
/// Only what ssh itself printed is quoted back on failure: a destination can
/// carry a username, and nothing this program constructed is echoed.
pub fn ssh(destination: &str, remote_command: &str, what: &str) -> Result<String, Error> {
    let mut command = Command::new("ssh");
    command.args(batch_options());
    command.arg(destination);
    command.arg(remote_command);
    output(command, "ssh", what)
}

/// Copies a file with `scp`, in whichever direction the arguments say.
///
/// Paths are passed as separate arguments, never assembled into a shell line:
/// a vault is named like anything else, and a name with a space in it must not
/// turn into two arguments on the far side.
pub fn scp(from: &str, to: &str, what: &str) -> Result<(), Error> {
    let mut command = Command::new("scp");
    command.args(batch_options());
    // Preserving nothing on purpose: -p would carry the local mtime to the
    // remote, and the timestamps of a file that gives nothing away are still
    // something it need not give away.
    command.arg("--").arg(from).arg(to);
    output(command, "scp", what).map(|_| ())
}

/// Runs a prepared command, turning both kinds of failure into [`Error`].
fn output(mut command: Command, program: &'static str, what: &str) -> Result<String, Error> {
    // Otherwise a console window flashes on every call on Windows.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = match command.output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NotInstalled(program));
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
                format!("{program} exited with {}", output.status)
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
    fn a_failure_carries_what_ssh_said() {
        // A destination that cannot resolve, so ssh fails on its own terms
        // rather than on anything this test arranged.
        let error = ssh(
            "sefy-test-nonexistent.invalid",
            "true",
            "reaching the server",
        )
        .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("reaching the server"), "got: {message}");
        assert!(
            !message.is_empty(),
            "ssh's own reason must survive: {message}"
        );
    }

    #[test]
    fn a_missing_program_is_named_in_the_message() {
        let error = Error::NotInstalled("scp");

        let message = error.to_string();
        assert!(message.contains("scp"), "got: {message}");
        assert!(message.contains("OpenSSH"), "got: {message}");
    }

    #[test]
    fn every_call_refuses_to_prompt() {
        // A plugin runs without a terminal; a password prompt would hang the
        // call instead of failing it, and a hung sync is worse than a failed
        // one because nothing says what it is waiting for.
        assert!(batch_options().contains(&"BatchMode=yes"));
    }
}
