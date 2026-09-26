//! Starting another program in sefy's place, for `sefy run`.
//!
//! The command is meant to behave as if it had been typed on its own: the same
//! input and output, the same Ctrl+C, and its exit status as sefy's. On Unix
//! sefy *becomes* it — the process image is replaced, so signals reach the
//! command directly and the decrypted vault leaves memory with sefy's image.
//! Windows has no such call, so sefy starts the command, stands aside while it
//! runs, and exits with its status.

use std::ffi::{OsStr, OsString};
use std::process::Command;

/// The command could not be started at all, as opposed to having run and
/// failed.
///
/// Told apart by exit status the way `env`, `nohup` and `timeout` tell it:
/// 127 when there is no such program, 126 when there is one and it would not
/// start. A script can then tell "the tool is missing" from "the tool said
/// no" from "sefy could not get the secrets" (125).
#[derive(Debug)]
pub struct NotStarted {
    program: OsString,
    error: std::io::Error,
}

impl NotStarted {
    /// The exit status sefy ends with.
    pub fn status(&self) -> u8 {
        if self.error.kind() == std::io::ErrorKind::NotFound {
            127
        } else {
            126
        }
    }
}

impl std::fmt::Display for NotStarted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let program = self.program.to_string_lossy();
        if self.error.kind() == std::io::ErrorKind::NotFound {
            write!(f, "cannot run {program}: no such program on PATH")
        } else {
            write!(f, "cannot run {program}: {}", self.error)
        }
    }
}

impl std::error::Error for NotStarted {}

/// Runs `program` with `arguments` in sefy's place and never comes back.
///
/// The environment is sefy's own, less `unset`, plus `set`. Returns only when
/// the command could not be started.
pub fn replace_with(
    program: &OsStr,
    arguments: &[OsString],
    unset: Option<&str>,
    set: Vec<(String, String)>,
) -> Result<std::convert::Infallible, NotStarted> {
    let mut command = Command::new(resolve(program));
    command.args(arguments);
    if let Some(variable) = unset {
        command.env_remove(variable);
    }
    command.envs(set);
    hand_over(command, program)
}

#[cfg(unix)]
fn hand_over(
    mut command: Command,
    program: &OsStr,
) -> Result<std::convert::Infallible, NotStarted> {
    use std::os::unix::process::CommandExt;
    // `exec` returns only on failure; on success this process is the command.
    Err(NotStarted {
        program: program.to_owned(),
        error: command.exec(),
    })
}

#[cfg(windows)]
fn hand_over(
    mut command: Command,
    program: &OsStr,
) -> Result<std::convert::Infallible, NotStarted> {
    leave_interrupts_to_the_command();
    let mut child = command.spawn().map_err(|error| NotStarted {
        program: program.to_owned(),
        error,
    })?;
    // The values are in the child's environment now; sefy's copies of them
    // have no further use.
    drop(command);
    let status = child.wait().map_err(|error| NotStarted {
        program: program.to_owned(),
        error,
    })?;
    // `process::exit` rather than an `ExitCode`: a Windows status is 32 bits
    // (0xC000013A for a console closed on it), and `ExitCode` holds eight.
    std::process::exit(status.code().unwrap_or(1));
}

/// Keeps Ctrl+C and Ctrl+Break from ending sefy while the command runs.
///
/// Every process on a console receives them. Left to the default, sefy would
/// exit at once and hand the prompt back while the command is still deciding
/// what to do with the same keystroke — a shell and a live program drawing on
/// one screen. With this, the command decides alone and sefy follows it out.
/// A handler routine, unlike the "ignore" flag, is not inherited, so the
/// command gets the ordinary behaviour.
#[cfg(windows)]
fn leave_interrupts_to_the_command() {
    type Handler = unsafe extern "system" fn(u32) -> i32;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<Handler>, add: i32) -> i32;
    }

    const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;

    unsafe extern "system" fn leave_it(event: u32) -> i32 {
        // Handled for the two keystrokes; a closing console or a logoff goes
        // on to the default handler, which ends sefy as it should.
        i32::from(event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT)
    }

    // Failure means there is no console, and then there are no console
    // keystrokes to leave to anyone.
    // SAFETY: registers a handler that only reads its argument; it stays
    // valid for the life of the process.
    unsafe {
        SetConsoleCtrlHandler(Some(leave_it), 1);
    }
}

/// Finds the program a shell would run for the name typed.
///
/// On Unix the standard search is already the shell's. On Windows it is not:
/// it tries a bare name with `.exe` alone, so `npm`, `pnpm` and every other
/// tool installed as a `.cmd` script would be "not found" by sefy while the
/// shell the same line was typed into finds them through PATHEXT. This walks
/// PATH the way that shell does — directory by directory, extension by
/// extension — keeping to the extensions a program can be started from
/// directly: a `.js` file is opened through its file association, which
/// starting a process does not do.
#[cfg(windows)]
fn resolve(program: &OsStr) -> std::path::PathBuf {
    use std::path::{Component, Path};

    const STARTABLE: [&str; 4] = [".COM", ".EXE", ".BAT", ".CMD"];

    let path = Path::new(program);
    let bare = path.extension().is_none()
        && matches!(
            path.components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        );
    if !bare {
        return path.to_path_buf();
    }

    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| STARTABLE.join(";"));
    let extensions: Vec<&str> = pathext
        .split(';')
        .filter(|extension| {
            STARTABLE
                .iter()
                .any(|startable| startable.eq_ignore_ascii_case(extension))
        })
        .collect();
    let directories = std::env::var_os("PATH").unwrap_or_default();
    for directory in std::env::split_paths(&directories) {
        for extension in &extensions {
            let mut name = program.to_os_string();
            name.push(extension);
            let candidate = directory.join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    // Nothing on PATH: the standard search still looks in a few places of
    // its own, and its "not found" is the honest answer if those fail too.
    path.to_path_buf()
}

#[cfg(not(windows))]
fn resolve(program: &OsStr) -> &OsStr {
    program
}
