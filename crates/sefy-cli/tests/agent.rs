//! `sefy agent` and `sefy lock`, with real processes.
//!
//! Every test has a socket of its own (`SEFY_AGENT_SOCKET`), so none of them can
//! reach an agent holding someone's real password, or each other's. Every
//! secret here is synthetic.
//!
//! The commands that are meant to find the vault unlocked are run with no
//! terminal and no `--password-env`: where one could not get a key, it fails
//! rather than asks, and that failure is what most of these look for.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

const MASTER: &str = "master password";
const OTHER: &str = "another password";

/// How long a command may take. A command that outlives this is holding on to
/// something it should not: above all, an agent carrying this test's pipes.
const TIME_LIMIT: Duration = Duration::from_secs(60);

struct Fixture {
    directory: tempfile::TempDir,
    path: PathBuf,
    socket: String,
}

/// A socket name no other test in this process has.
fn socket_in(directory: &Path) -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let number = NEXT.fetch_add(1, Ordering::SeqCst);
    if cfg!(windows) {
        format!("sefy-test-cli-{}-{number}", std::process::id())
    } else {
        // A level down, for the agent to make. A temporary directory is one
        // others may enter, and an agent refuses to listen in those.
        directory
            .join("sockets")
            .join(format!("s{number}"))
            .display()
            .to_string()
    }
}

impl Fixture {
    /// A directory holding a vault with one note in it.
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.bak");
        let socket = socket_in(directory.path());
        let fixture = Self {
            directory,
            path,
            socket,
        };
        fixture.with_password().arg("init").assert().success();
        fixture
            .with_password()
            .args(["add", "note", "hello", "--text", "world"])
            .assert()
            .success();
        fixture
    }

    /// `sefy` on this vault and this socket, with no password to hand: what a
    /// command does then is up to the agent.
    fn sefy(&self) -> Command {
        let mut command = Command::cargo_bin("sefy").unwrap();
        command
            .env("SEFY_VAULT", &self.path)
            .env("SEFY_AGENT_SOCKET", &self.socket)
            .env_remove("SEFY_AUTO_SYNC")
            .timeout(TIME_LIMIT);
        command
    }

    /// The same, with the password in the environment.
    fn with_password(&self) -> Command {
        let mut command = self.sefy();
        command
            .env("SEFY_TEST_PASSWORD", MASTER)
            .args(["--password-env", "SEFY_TEST_PASSWORD"]);
        command
    }

    /// Starts an agent, and returns its process id.
    fn agent(&self, ttl: &str) -> u32 {
        let output = self
            .with_password()
            .args(["agent", "--ttl", ttl])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let output = String::from_utf8(output).unwrap();
        let (_, rest) = output.split_once("agent pid ").expect(&output);
        rest.split(')').next().unwrap().parse().unwrap()
    }

    fn salt(&self) -> Vec<u8> {
        std::fs::read(&self.path).unwrap()[..16].to_vec()
    }
}

impl Drop for Fixture {
    /// An agent a failed test left behind would hold its pipe, and its working
    /// memory, for as long as its time ran.
    fn drop(&mut self) {
        let _ = self.sefy().arg("lock").output();
    }
}

/// Whether a process is running, asked of the system and not of the process:
/// these tests are about what it does with nobody asking it anything.
fn is_running(pid: u32) -> bool {
    if cfg!(windows) {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
    } else {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }
}

fn wait_until_gone(pid: u32) {
    let until = Instant::now() + Duration::from_secs(30);
    while is_running(pid) {
        assert!(Instant::now() < until, "process {pid} is still there");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn nothing_running_means_every_command_needs_the_password() {
    let fixture = Fixture::new();
    fixture
        .sefy()
        .args(["get", "hello", "--stdout"])
        .assert()
        .failure()
        .stderr(contains("cannot ask for a password").and(contains("sefy agent")));
}

#[test]
fn an_agent_keeps_the_vault_unlocked_until_it_is_locked() {
    let fixture = Fixture::new();
    let pid = fixture.agent("10m");

    // Reading.
    fixture
        .sefy()
        .args(["get", "hello", "--stdout"])
        .assert()
        .success()
        .stdout(contains("world"));

    // Writing, each time under a salt of its own (ADR-0001), to a file the
    // password alone opens.
    let before = fixture.salt();
    fixture
        .sefy()
        .args(["add", "note", "second", "--text", "two"])
        .assert()
        .success();
    let after = fixture.salt();
    assert_ne!(before, after, "a write kept the salt");
    fixture
        .sefy()
        .args(["add", "note", "third", "--text", "three"])
        .assert()
        .success();
    assert_ne!(after, fixture.salt(), "a write kept the salt");
    fixture
        .with_password()
        .args(["get", "third", "--stdout"])
        .assert()
        .success()
        .stdout(contains("three"));

    // Locking ends the process, and the next command needs the password again.
    fixture
        .sefy()
        .arg("lock")
        .assert()
        .success()
        .stdout(contains("locked the agent"));
    wait_until_gone(pid);
    fixture
        .sefy()
        .args(["get", "hello", "--stdout"])
        .assert()
        .failure()
        .stderr(contains("cannot ask for a password"));

    // Nothing left to lock, which is not an error.
    fixture
        .sefy()
        .arg("lock")
        .assert()
        .success()
        .stdout(contains("nothing was unlocked"));
}

#[test]
fn the_process_goes_when_its_time_is_up_with_nobody_asking_it_anything() {
    // Only the system is asked here. An agent that wiped the password when
    // the next request came, and only then, would still be running.
    let fixture = Fixture::new();
    let pid = fixture.agent("2s");
    assert!(is_running(pid));
    wait_until_gone(pid);

    fixture
        .sefy()
        .args(["get", "hello", "--stdout"])
        .assert()
        .failure()
        .stderr(contains("cannot ask for a password"));
}

#[test]
fn using_the_vault_keeps_it_unlocked_past_the_idle_time() {
    let fixture = Fixture::new();
    let pid = fixture.agent("5s");
    for _ in 0..4 {
        std::thread::sleep(Duration::from_secs(2));
        fixture
            .sefy()
            .args(["get", "hello", "--stdout"])
            .assert()
            .success();
    }
    assert!(is_running(pid));
    wait_until_gone(pid);
}

#[test]
fn a_wrong_password_starts_nothing() {
    let fixture = Fixture::new();
    fixture
        .sefy()
        .env("SEFY_TEST_PASSWORD", "not it")
        .args(["--password-env", "SEFY_TEST_PASSWORD", "agent"])
        .assert()
        .failure()
        .stderr(contains("wrong password"));
    fixture
        .sefy()
        .arg("lock")
        .assert()
        .success()
        .stdout(contains("nothing was unlocked"));
}

#[test]
fn a_time_is_a_time_and_has_a_limit() {
    let fixture = Fixture::new();
    for wrong in ["0", "0m", "soon", "5d", "13h", "99999999999999h"] {
        fixture
            .with_password()
            .args(["agent", "--ttl", wrong])
            .assert()
            .failure();
    }
    fixture
        .sefy()
        .arg("lock")
        .assert()
        .stdout(contains("nothing was unlocked"));
}

#[test]
fn running_it_again_asks_for_nothing_and_starts_nothing() {
    // It goes in a shell's profile, which every shell runs.
    let fixture = Fixture::new();
    let pid = fixture.agent("10m");
    fixture
        .sefy()
        .arg("agent")
        .assert()
        .success()
        .stdout(contains("already unlocked by the agent"));
    assert!(is_running(pid));
}

#[test]
fn an_agent_holds_one_password_and_another_vault_goes_on_asking() {
    let fixture = Fixture::new();
    fixture.agent("10m");

    let other = fixture.directory.path().join("other.bak");
    let on_other = || {
        let mut command = fixture.sefy();
        command.env("SEFY_VAULT", &other);
        command
    };
    on_other()
        .env("SEFY_OTHER_PASSWORD", OTHER)
        .args(["--password-env", "SEFY_OTHER_PASSWORD", "init"])
        .assert()
        .success();

    // The key does not fit, and that is not a wrong password: it asks.
    on_other()
        .args(["ls"])
        .assert()
        .failure()
        .stderr(contains("cannot ask for a password"));
    // Its status says what is going on rather than just "locked".
    on_other()
        .arg("status")
        .assert()
        .success()
        .stdout(contains("locked (what is running holds another vault"));
    // A second agent is refused, not started: it could not listen.
    on_other()
        .env("SEFY_OTHER_PASSWORD", OTHER)
        .args(["--password-env", "SEFY_OTHER_PASSWORD", "agent"])
        .assert()
        .failure()
        .stderr(contains("another vault's password").and(contains("sefy lock")));
}

#[test]
fn a_password_given_is_the_password_used() {
    // A script that passes one should behave the same with an agent running
    // as without. A wrong one fails, however unlocked the vault is.
    let fixture = Fixture::new();
    fixture.agent("10m");
    fixture
        .sefy()
        .env("SEFY_TEST_PASSWORD", "not it")
        .args(["--password-env", "SEFY_TEST_PASSWORD", "ls"])
        .assert()
        .failure()
        .stderr(contains("wrong password"));
}

#[test]
fn status_never_asks_and_says_what_it_can() {
    let fixture = Fixture::new();

    // Locked, with no password and no terminal: an answer, not an error.
    fixture.sefy().arg("status").assert().success().stdout(
        contains("lock     locked")
            .and(contains("vault "))
            .and(contains("size "))
            .and(contains("sefy agent"))
            .and(contains("items").not()),
    );

    // With the password: the whole of it, and still locked.
    fixture
        .with_password()
        .arg("status")
        .assert()
        .success()
        .stdout(contains("lock     locked").and(contains("items    1 item")));

    // Unlocked: the whole of it, how, and for how much longer. Asked with the
    // password as well: an agent is running either way.
    fixture.agent("10m");
    fixture.sefy().arg("status").assert().success().stdout(
        contains("lock     unlocked by the agent, locks in 9m").and(contains("items    1 item")),
    );
    fixture
        .with_password()
        .arg("status")
        .assert()
        .success()
        .stdout(contains("lock     unlocked by the agent, locks in 9m"));
}

#[test]
fn looking_at_the_status_does_not_keep_it_unlocked() {
    let fixture = Fixture::new();
    let pid = fixture.agent("3s");
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        fixture.sefy().arg("status").assert().success();
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(!is_running(pid), "still running after its idle time");
}

#[test]
fn status_of_a_file_that_is_not_there_is_an_error() {
    let fixture = Fixture::new();
    fixture
        .sefy()
        .env("SEFY_VAULT", fixture.directory.path().join("gone.bak"))
        .arg("status")
        .assert()
        .failure()
        .stderr(contains("cannot read"));
}

#[test]
fn changing_the_password_takes_it_typed_and_retires_the_agent() {
    let fixture = Fixture::new();
    let pid = fixture.agent("10m");

    // Unlocked is not enough.
    fixture
        .sefy()
        .env("SEFY_NEW_PASSWORD", "new one")
        .args(["change-password", "--new-password-env", "SEFY_NEW_PASSWORD"])
        .assert()
        .failure()
        .stderr(contains("cannot ask for a password"));
    fixture.with_password().arg("ls").assert().success();
    assert!(is_running(pid));

    // With it, and the agent holding the old one goes.
    fixture
        .with_password()
        .env("SEFY_NEW_PASSWORD", "new one")
        .args(["change-password", "--new-password-env", "SEFY_NEW_PASSWORD"])
        .assert()
        .success()
        .stdout(contains("locked the agent"));
    wait_until_gone(pid);

    fixture
        .sefy()
        .env("SEFY_NEW_PASSWORD", "new one")
        .args(["--password-env", "SEFY_NEW_PASSWORD", "ls"])
        .assert()
        .success()
        .stdout(contains("hello"));
}

#[test]
fn the_doctor_says_who_holds_the_password() {
    let fixture = Fixture::new();
    fixture
        .with_password()
        .arg("doctor")
        .assert()
        .stdout(contains("nothing is holding the password"));

    fixture.agent("10m");
    fixture.sefy().arg("doctor").assert().stdout(
        contains("the agent is holding the password")
            .and(contains("notes.bak opens"))
            .and(contains("cannot ask").not()),
    );
}

/// `add note` with its text still to come on standard input, so the vault is
/// open and the command waiting while the test changes things around it.
fn add_waiting_on_input(fixture: &Fixture) -> std::process::Child {
    let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin("sefy"));
    command
        .env("SEFY_VAULT", &fixture.path)
        .env("SEFY_AGENT_SOCKET", &fixture.socket)
        .env_remove("SEFY_AUTO_SYNC")
        .args(["add", "note", "written late"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command.spawn().unwrap();
    // Time to open the vault through the agent and reach the read.
    std::thread::sleep(Duration::from_secs(3));
    child
}

fn finish(mut child: std::process::Child) -> std::process::Output {
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"the note\n")
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn a_vault_that_locked_while_a_command_ran_is_left_as_it_was() {
    // With nobody at a terminal there is no one to ask, and nothing is written.
    let fixture = Fixture::new();
    fixture.agent("10m");
    let waiting = add_waiting_on_input(&fixture);
    let before = std::fs::read(&fixture.path).unwrap();

    fixture.sefy().arg("lock").assert().success();

    let output = finish(waiting);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("locked while this was running"), "{stderr}");
    assert_eq!(std::fs::read(&fixture.path).unwrap(), before);
}

#[test]
fn an_agent_on_another_password_does_not_get_to_seal_this_vault() {
    // The vault is opened through one agent, which locks; another comes up
    // with another vault's password; the command that was waiting saves. It
    // must not move this vault to that password.
    let fixture = Fixture::new();
    fixture.agent("10m");
    let waiting = add_waiting_on_input(&fixture);
    let before = std::fs::read(&fixture.path).unwrap();
    fixture.sefy().arg("lock").assert().success();

    let other = fixture.directory.path().join("other.bak");
    fixture
        .sefy()
        .env("SEFY_VAULT", &other)
        .env("SEFY_OTHER_PASSWORD", OTHER)
        .args(["--password-env", "SEFY_OTHER_PASSWORD", "init"])
        .assert()
        .success();
    fixture
        .sefy()
        .env("SEFY_VAULT", &other)
        .env("SEFY_OTHER_PASSWORD", OTHER)
        .args(["--password-env", "SEFY_OTHER_PASSWORD", "agent"])
        .assert()
        .success();

    let output = finish(waiting);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("not the password this vault"), "{stderr}");
    assert_eq!(std::fs::read(&fixture.path).unwrap(), before);
    fixture
        .with_password()
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("written late").not());
}

#[test]
fn what_the_agent_starts_is_not_offered_to_anyone_typing() {
    let fixture = Fixture::new();
    fixture
        .sefy()
        .args(["agent", "--help"])
        .assert()
        .success()
        .stdout(contains("--ttl").and(contains("--serve").not()));
}

#[test]
fn the_process_given_no_password_does_not_listen() {
    let fixture = Fixture::new();
    fixture
        .sefy()
        .args(["agent", "--serve"])
        .write_stdin("")
        .assert()
        .failure()
        .stdout(contains("failed"));
    fixture
        .sefy()
        .arg("lock")
        .assert()
        .stdout(contains("nothing was unlocked"));
}

#[cfg(target_os = "linux")]
#[test]
fn other_programs_of_this_user_cannot_read_the_agent() {
    // /proc/<pid>/environ and /proc/<pid>/mem need the process to be dumpable.
    let fixture = Fixture::new();
    let pid = fixture.agent("10m");
    for file in ["environ", "mem"] {
        let opened = std::fs::File::open(format!("/proc/{pid}/{file}"));
        assert_eq!(
            opened.map(|_| ()).unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied,
            "{file}"
        );
    }
}

#[cfg(unix)]
#[test]
fn an_agent_will_not_listen_in_a_directory_others_can_enter() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let open_to_all = fixture.directory.path().join("shared");
    std::fs::create_dir(&open_to_all).unwrap();
    std::fs::set_permissions(&open_to_all, std::fs::Permissions::from_mode(0o755)).unwrap();

    fixture
        .with_password()
        .env(
            "SEFY_AGENT_SOCKET",
            open_to_all.join("socket").display().to_string(),
        )
        .arg("agent")
        .assert()
        .failure()
        .stderr(contains("other users"));
}

#[cfg(unix)]
#[test]
fn a_socket_a_crash_left_behind_is_not_in_the_way() {
    let fixture = Fixture::new();
    let pid = fixture.agent("10m");
    std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    wait_until_gone(pid);
    assert!(Path::new(&format!("{}-agent", fixture.socket)).exists());

    fixture.agent("10m");
    fixture
        .sefy()
        .args(["get", "hello", "--stdout"])
        .assert()
        .success();
}

#[cfg(unix)]
#[test]
fn locking_leaves_no_socket_on_disk() {
    let fixture = Fixture::new();
    let pid = fixture.agent("10m");
    let socket = format!("{}-agent", fixture.socket);
    assert!(Path::new(&socket).exists());

    fixture.sefy().arg("lock").assert().success();
    wait_until_gone(pid);
    assert!(!Path::new(&socket).exists());
}
