//! The command line and a window holding the vault, without the window.
//!
//! The desktop app answers in a seat of its own, and what it answers with is a
//! [`Keeper`]; this test sits in that seat with one, in its own process, since
//! where an agent listens comes from an environment variable. What it holds
//! down is the part a window cannot show in a test: `sefy lock` reaches a window
//! as well as an agent, and neither hides the other. The window's own side is
//! in `gui/src-tauri`.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use sefy_core::agent::{self, IDLE, Keeper, LockState, SOCKET_ENV, Seat};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

const MASTER: &str = "master password";

/// Somewhere of this test's own, never where a real agent would be.
fn socket_base(directory: &Path) -> String {
    if cfg!(windows) {
        format!("sefy-test-window-{}", std::process::id())
    } else {
        directory.join("socket").display().to_string()
    }
}

fn wait_for(what: &str, happened: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !happened() {
        assert!(Instant::now() < until, "{what} did not happen");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_window_answers_the_command_line_and_is_locked_by_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.bak");
    let socket = socket_base(directory.path());
    // SAFETY: the only test in this binary, and nothing else runs yet.
    unsafe { std::env::set_var(SOCKET_ENV, &socket) };

    let sefy = || {
        let mut command = Command::cargo_bin("sefy").unwrap();
        command
            .env("SEFY_VAULT", &path)
            .env("SEFY_TEST_PASSWORD", MASTER)
            .env_remove("SEFY_AUTO_SYNC")
            .timeout(Duration::from_secs(60));
        command
    };
    sefy()
        .args(["--password-env", "SEFY_TEST_PASSWORD", "init"])
        .assert()
        .success();
    sefy()
        .args(["--password-env", "SEFY_TEST_PASSWORD"])
        .args(["add", "note", "hello", "--text", "world"])
        .assert()
        .success();

    // A window that has started and has nothing unlocked: it listens, holding
    // nothing, and the command line goes on asking.
    let keeper = Keeper::new();
    let told = Arc::new(AtomicUsize::new(0));
    let counter = told.clone();
    keeper.when_locked(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    agent::serve(Seat::Window, keeper.clone()).unwrap();
    sefy()
        .args(["get", "hello", "--stdout"])
        .assert()
        .failure()
        .stderr(contains("cannot ask for a password"));
    sefy()
        .arg("status")
        .assert()
        .success()
        .stdout(contains("lock     locked\n").or(contains("lock     locked\r\n")));

    // Unlocked in the window, the command line does not ask.
    keeper.give(Zeroizing::new(MASTER.as_bytes().to_vec()), IDLE);
    sefy()
        .args(["get", "hello", "--stdout"])
        .assert()
        .success()
        .stdout(contains("world"));
    sefy()
        .arg("status")
        .assert()
        .success()
        .stdout(contains("unlocked by the window, locks in 14m"));
    sefy()
        .arg("agent")
        .assert()
        .success()
        .stdout(contains("already unlocked by the window"));

    // A lock reaches it, and whoever runs it is told.
    sefy()
        .arg("lock")
        .assert()
        .success()
        .stdout(contains("locked the window"));
    assert_eq!(keeper.state(), LockState::Locked);
    wait_for("telling the window", || told.load(Ordering::SeqCst) >= 1);
    sefy()
        .args(["get", "hello", "--stdout"])
        .assert()
        .failure()
        .stderr(contains("cannot ask for a password"));

    // With an agent beside it, a lock reaches both: an unlocked window must not
    // be left behind a lock that says it worked.
    keeper.give(Zeroizing::new(MASTER.as_bytes().to_vec()), IDLE);
    let other = directory.path().join("other.bak");
    sefy()
        .env("SEFY_VAULT", &other)
        .env("SEFY_OTHER_PASSWORD", "another password")
        .args(["--password-env", "SEFY_OTHER_PASSWORD", "init"])
        .assert()
        .success();
    sefy()
        .env("SEFY_VAULT", &other)
        .env("SEFY_OTHER_PASSWORD", "another password")
        .args(["--password-env", "SEFY_OTHER_PASSWORD", "agent"])
        .assert()
        .success();
    // Each vault finds the one that opens it, whichever seat that is.
    sefy()
        .args(["get", "hello", "--stdout"])
        .assert()
        .success()
        .stdout(contains("world"));
    sefy()
        .env("SEFY_VAULT", &other)
        .arg("status")
        .assert()
        .success()
        .stdout(contains("unlocked by the agent"));

    sefy()
        .arg("lock")
        .assert()
        .success()
        .stdout(contains("locked the agent and the window"));
    assert_eq!(keeper.state(), LockState::Locked);
}
