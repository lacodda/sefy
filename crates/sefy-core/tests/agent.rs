//! The agent over a real socket, in one process.
//!
//! One test: where an agent listens comes from an environment variable, which
//! is the whole process's, and one socket at a time is what can be stood up
//! here. The end-to-end behaviour, with real processes, is in the command
//! line's tests.

#![cfg(feature = "agent")]

use interprocess::local_socket::ListenerOptions;
use interprocess::local_socket::prelude::*;
use sefy_core::agent::{self, Client, Keeper, LockState, SOCKET_ENV, Seat, Taken};
use sefy_core::crypto::{DerivedKey, SALT_LEN};
use sefy_core::{Error, KeySource, NewItem, Payload, Vault};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

const PASSWORD: &[u8] = b"correct horse battery staple";

fn password() -> Zeroizing<Vec<u8>> {
    Zeroizing::new(PASSWORD.to_vec())
}

/// Somewhere of this test's own to listen, never where a real agent would be.
#[cfg(windows)]
fn socket_base(_: &std::path::Path) -> String {
    format!("sefy-test-core-{}", std::process::id())
}

#[cfg(unix)]
fn socket_base(directory: &std::path::Path) -> String {
    directory.join("socket").display().to_string()
}

#[test]
fn an_agent_answers_over_the_socket_until_it_locks() {
    let directory = tempfile::tempdir().unwrap();
    // SAFETY: the only test in this binary, and nothing else runs yet.
    unsafe { std::env::set_var(SOCKET_ENV, socket_base(directory.path())) };

    let keeper = Keeper::new();
    let told = Arc::new(AtomicUsize::new(0));
    let counter = told.clone();
    keeper.when_locked(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    agent::serve(Seat::Agent, keeper.clone()).unwrap();
    let client = Client::of(Seat::Agent).unwrap();

    // Nobody sits in the other seat, and that is an answer rather than a wait.
    assert_eq!(Client::of(Seat::Window).unwrap().state(), None);
    assert_eq!(Client::of(Seat::Window).unwrap().lock(), None);
    assert!(matches!(
        Client::of(Seat::Window).unwrap().key_for(&[1; SALT_LEN]),
        Err(Error::KeysGone)
    ));

    // Listening with nothing held: everyone is told there is nothing.
    assert_eq!(client.state(), Some(LockState::Locked));
    assert!(matches!(
        client.key_for(&[1; SALT_LEN]),
        Err(Error::KeysGone)
    ));
    assert!(matches!(client.fresh(&[1; SALT_LEN]), Err(Error::KeysGone)));

    // A seat holds one.
    assert!(matches!(
        agent::serve(Seat::Agent, Keeper::new()),
        Err(Taken::Occupied)
    ));

    keeper.give(password(), Duration::from_secs(60));
    let Some(LockState::Unlocked { left, idle }) = client.state() else {
        panic!("it was given the password");
    };
    assert_eq!(idle, Duration::from_secs(60));
    assert!(left <= idle && left > Duration::from_secs(50), "{left:?}");

    // The keys are the password's.
    let salt = [7; SALT_LEN];
    let expected = DerivedKey::derive(PASSWORD, &salt).unwrap();
    assert!(client.key_for(&salt).unwrap().same_as(&expected));
    let fresh = client.fresh(&salt).unwrap();
    assert!(fresh.proof.same_as(&expected));
    assert!(
        fresh
            .key
            .same_as(&DerivedKey::derive(PASSWORD, &fresh.salt).unwrap())
    );

    // A whole vault through it, and the password opens what it wrote.
    let path = directory.path().join("notes.bak");
    Vault::create(&path, PASSWORD).unwrap();
    let mut vault = Vault::open_through(&path, Arc::new(client.clone())).unwrap();
    vault
        .add(NewItem::new(
            "through the socket",
            Payload::Note {
                text: "synthetic".into(),
            },
        ))
        .unwrap();
    vault.save().unwrap();
    assert_eq!(
        Vault::open(&path, PASSWORD).unwrap().list().unwrap().len(),
        1
    );

    // Many at once: a pipe has instances, and they have to keep up.
    let askers: Vec<_> = (0..16)
        .map(|_| {
            let client = client.clone();
            std::thread::spawn(move || client.key_for(&salt).is_ok())
        })
        .collect();
    for asker in askers {
        assert!(asker.join().unwrap(), "a client went unanswered");
    }

    #[cfg(windows)]
    only_the_owner_may_open_the_pipe();

    // Locking: the answer comes, the password is gone, whoever asked is told.
    assert_eq!(client.lock(), Some(true));
    assert_eq!(keeper.state(), LockState::Locked);
    assert_eq!(client.state(), Some(LockState::Locked));
    assert!(matches!(client.key_for(&salt), Err(Error::KeysGone)));
    wait_until_told(&told, 1);

    // The vault open through it survives that for reading, and cannot save.
    assert!(matches!(vault.save(), Err(Error::KeysGone)));

    // Twice is fine, and says there was nothing the second time.
    assert_eq!(client.lock(), Some(false));
    wait_until_told(&told, 2);

    an_agent_that_stopped_answering_costs_a_moment();
}

/// Something listens in the other seat and never answers. Named pipes have no
/// read timeout, so without one of its own a command would wait there forever
/// instead of asking for the password.
fn an_agent_that_stopped_answering_costs_a_moment() {
    let base = std::env::var(SOCKET_ENV).unwrap();
    let address = format!("{base}-window");
    #[cfg(windows)]
    let name = address
        .as_str()
        .to_ns_name::<interprocess::local_socket::GenericNamespaced>()
        .unwrap();
    #[cfg(unix)]
    let name = address
        .as_str()
        .to_fs_name::<interprocess::local_socket::GenericFilePath>()
        .unwrap();
    let silent = ListenerOptions::new().name(name).create_sync().unwrap();

    let impatient = Client::of(Seat::Window)
        .unwrap()
        .with_patience(Duration::from_millis(300));
    let started = Instant::now();
    assert_eq!(impatient.state(), None);
    assert_eq!(impatient.lock(), None);
    assert!(matches!(
        impatient.key_for(&[1; SALT_LEN]),
        Err(Error::KeysGone)
    ));
    assert!(matches!(
        impatient.fresh(&[1; SALT_LEN]),
        Err(Error::KeysGone)
    ));
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "it waited {:?}",
        started.elapsed()
    );
    drop(silent);
}

/// Being told comes just after the answer, on the agent's own thread.
fn wait_until_told(told: &AtomicUsize, times: usize) {
    let until = Instant::now() + Duration::from_secs(10);
    while told.load(Ordering::SeqCst) < times {
        assert!(Instant::now() < until, "nobody was told");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A pipe left with the system's default lets anyone on the machine open it
/// for reading. Read back through the system, as another program would see it.
#[cfg(windows)]
fn only_the_owner_may_open_the_pipe() {
    let pipe = format!(r"\\.\pipe\{}-agent", std::env::var(SOCKET_ENV).unwrap());
    let script = format!(
        "[System.IO.Directory]::GetAccessControl('{pipe}').GetSecurityDescriptorSddlForm('Access')"
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .unwrap();
    let sddl = String::from_utf8_lossy(&output.stdout);
    assert_eq!(sddl.trim(), "D:P(A;;FA;;;OW)", "{output:?}");
}
