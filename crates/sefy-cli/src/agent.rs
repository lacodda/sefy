//! `sefy agent` and `sefy lock`: a process that holds the password for a while.
//!
//! `sefy agent` asks for the password, checks it against the vault, and leaves
//! behind a process of its own, detached, that holds it until the time is up or
//! `sefy lock` says otherwise; then it wipes the password and exits. The process
//! being gone is the state "locked": nothing stays around waiting to be given a
//! password again, and nothing can be left in its memory.
//!
//! The password reaches that process through an anonymous pipe to its standard
//! input, which nobody else can open: not an argument, which every process
//! listing shows, and not the agent's own socket, which is only ever asked for
//! keys.

use crate::cli::AgentArgs;
use crate::session;
use anyhow::{Context, Result, anyhow, bail};
use sefy_core::agent::{self, Client, Keeper, LockState, Seat, Taken};
use std::io::{BufRead, BufReader, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use zeroize::Zeroizing;

/// More than anyone types, and small enough to read without reallocating: a
/// buffer that grew would leave a copy of the password in the one it outgrew.
const LONGEST_PASSWORD: usize = 64 * 1024;

/// What the agent process says on its standard output once it is listening.
const READY: &str = "ready";

/// What it says if it cannot start.
const FAILED: &str = "failed";

/// `sefy agent`.
pub fn run(args: AgentArgs, vault: Option<PathBuf>, password_env: Option<&str>) -> Result<()> {
    if args.serve {
        return serve(args.ttl);
    }

    let path = session::vault_path(vault)?;

    // Run from a shell's profile, it runs in every shell.
    if let Some((_, master)) = session::looking_through_agent(&path)? {
        println!("already {}", session::lock_line(&path, Some(&master))?);
        return Ok(());
    }
    let running = Client::of(Seat::Agent).and_then(|client| client.state());
    if matches!(running, Some(LockState::Unlocked { .. })) {
        bail!(
            "an agent is running with another vault's password\n\
             `sefy lock` stops it; an agent holds one password at a time"
        );
    }

    let password = Zeroizing::new(session::password(password_env)?);
    // Checked here, where a typo can be said: otherwise it would surface later
    // as every command asking for the password anyway.
    session::open(&path, &password)?;

    let pid = start(&password, args.ttl, password_env)?;
    println!(
        "unlocked for {} of nobody using it, {} at most (agent pid {pid})",
        crate::when::span(args.ttl),
        crate::when::span(agent::CAP)
    );
    println!("`sefy lock` locks it now");
    Ok(())
}

/// `sefy lock`.
pub fn lock() -> Result<()> {
    let locked = session::lock_everywhere();
    if locked.is_empty() {
        println!("nothing was unlocked");
    } else {
        println!("locked the {}", locked.join(" and the "));
    }
    Ok(())
}

/// Starts the agent process, hands it the password and waits until it listens.
fn start(password: &str, idle: Duration, password_env: Option<&str>) -> Result<u32> {
    let program = std::env::current_exe().context("cannot find this program to start it")?;
    let mut command = Command::new(program);
    command
        .args(["agent", "--serve", "--ttl"])
        .arg(format!("{}s", idle.as_secs()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Standing in a directory keeps it from being removed, for hours.
        .current_dir(neutral_directory())
        // It has no use for where the vault is, and a process's environment
        // can be read by anyone who can read its memory.
        .env_remove("SEFY_VAULT");
    if let Some(variable) = password_env {
        command.env_remove(variable);
    }
    detach(&mut command);

    let mut child = command.spawn().context("cannot start the agent")?;
    {
        let mut stdin = child.stdin.take().expect("stdin was piped");
        stdin
            .write_all(password.as_bytes())
            .context("cannot hand the password to the agent")?;
    }

    let stdout = child.stdout.take().expect("stdout was piped");
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .context("the agent did not answer")?;
    let (word, rest) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
    match word {
        READY => rest.parse().context("the agent did not say who it is"),
        FAILED => bail!("{rest}"),
        _ => bail!("the agent stopped before it could listen"),
    }
}

/// The process that stays: reads the password, listens, and waits to be told
/// it is over.
fn serve(idle: Duration) -> Result<()> {
    // Started by hand, this would sit reading a password from a terminal that
    // echoes it.
    if std::io::stdin().is_terminal() {
        bail!("`sefy agent --serve` is what `sefy agent` starts; use that");
    }
    harden();

    let mut password = Zeroizing::new(Vec::with_capacity(LONGEST_PASSWORD));
    std::io::stdin()
        .take(LONGEST_PASSWORD as u64)
        .read_to_end(&mut password)
        .context("cannot read the password")?;
    if password.is_empty() {
        println!("{FAILED} it was given no password");
        bail!("given no password");
    }

    let keeper = Keeper::new();
    keeper.give(password, idle);
    keeper.when_locked(|| {
        agent::leave(Seat::Agent);
        std::process::exit(0);
    });

    if let Err(error) = agent::serve(Seat::Agent, keeper.clone()) {
        println!("{FAILED} {}", describe(&error));
        return Err(anyhow!("{error}"));
    }
    agent::start_ticking(keeper);

    println!("{READY} {}", std::process::id());
    std::io::stdout().flush()?;

    // Everything from here happens on the clock's thread and the socket's.
    loop {
        std::thread::park();
    }
}

fn describe(taken: &Taken) -> String {
    match taken {
        // One line: it is read as one line by the process that started this.
        Taken::Occupied => "an agent is already running; `sefy lock` stops it".to_owned(),
        other => other.to_string(),
    }
}

/// Keeps other programs of this user out of this process's memory, where the
/// system offers a way: on Linux it makes the process undumpable, which is
/// what ssh-agent does, and which also means no core file with a password in
/// it. Windows and macOS have nothing a program can ask for that its owner
/// cannot undo; what the agent protects from there is other users.
fn harden() {
    #[cfg(target_os = "linux")]
    {
        const PR_SET_DUMPABLE: std::ffi::c_int = 4;
        unsafe extern "C" {
            fn prctl(option: std::ffi::c_int, ...) -> std::ffi::c_int;
        }
        // SAFETY: sets a flag on this process and reads no memory. A failure
        // leaves it as any other process, which the threat model states.
        unsafe {
            prctl(PR_SET_DUMPABLE, 0 as std::ffi::c_ulong);
        }
    }
}

/// A directory that will still be there, and that nobody wants to remove.
fn neutral_directory() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/")
    }
}

/// Lets the process outlive this one and its terminal.
#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const STD_HANDLES: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32];
    const HANDLE_FLAG_INHERIT: u32 = 1;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(which: u32) -> *mut std::ffi::c_void;
        fn SetHandleInformation(handle: *mut std::ffi::c_void, mask: u32, flags: u32) -> i32;
    }

    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);

    // Rust starts a child inheriting every inheritable handle of this process,
    // not only the ones named above, and the pipe a shell or a test gave this
    // process as its output is inheritable by construction. An agent carrying
    // it would hold that pipe open for hours, and whoever reads it waits for
    // the end that does not come: `sefy agent | tee` would never return.
    // SAFETY: changes one flag on this process's own standard handles; a
    // handle that is not there fails the call and changes nothing.
    unsafe {
        for which in STD_HANDLES {
            SetHandleInformation(GetStdHandle(which), HANDLE_FLAG_INHERIT, 0);
        }
    }
}

/// Lets the process outlive this one and its terminal: a session of its own
/// has no terminal to be hung up by.
#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    unsafe extern "C" {
        fn setsid() -> std::ffi::c_int;
    }

    // SAFETY: `setsid` is async-signal-safe, and it is all this does between
    // fork and exec. Failure means this already leads a session.
    unsafe {
        command.pre_exec(|| {
            setsid();
            Ok(())
        });
    }
}
