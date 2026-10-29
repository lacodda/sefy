//! The window's clipboard thread, on the real clipboard.
//!
//! Like the core's clipboard tests these write to the clipboard of the
//! machine they run on, so they are ignored by default and CI runs them on
//! its Windows runner:
//!
//!     cargo test -p sefy-gui --test clip -- --ignored --test-threads=1

#![cfg(windows)]

use sefy_core::clipboard::Clipboard;
use sefy_gui::clip::Clip;
use std::time::Duration;
use zeroize::Zeroizing;

const SECRET: &str = "not-a-real-secret: tr0ub4dor&3";

fn holds(value: &str) -> bool {
    Clipboard::open().unwrap().holds(value)
}

#[test]
#[ignore = "writes to the system clipboard"]
fn a_copy_is_taken_back_when_the_hold_is_over() {
    let clip = Clip::start(Duration::from_millis(400));
    clip.put(Zeroizing::new(SECRET.to_owned())).unwrap();
    assert!(holds(SECRET));

    std::thread::sleep(Duration::from_millis(1200));
    assert!(
        !holds(SECRET),
        "the hold ended and the secret is still there"
    );
}

#[test]
#[ignore = "writes to the system clipboard"]
fn a_lock_takes_the_copy_back_at_once() {
    let clip = Clip::start(Duration::from_secs(60));
    clip.put(Zeroizing::new(SECRET.to_owned())).unwrap();

    clip.take_back();
    assert!(!holds(SECRET));
}

#[test]
#[ignore = "writes to the system clipboard"]
fn a_second_copy_replaces_the_first_and_is_the_one_taken_back() {
    let clip = Clip::start(Duration::from_secs(60));
    clip.put(Zeroizing::new("first-not-real".to_owned()))
        .unwrap();
    clip.put(Zeroizing::new(SECRET.to_owned())).unwrap();
    assert!(holds(SECRET));

    clip.take_back();
    assert!(!holds(SECRET));
}

#[test]
#[ignore = "writes to the system clipboard"]
fn what_the_person_copied_since_is_left_alone() {
    let clip = Clip::start(Duration::from_secs(60));
    clip.put(Zeroizing::new(SECRET.to_owned())).unwrap();
    clipboard_win::set_clipboard_string("a shopping list").unwrap();

    clip.take_back();
    assert_eq!(
        clipboard_win::get_clipboard_string().unwrap(),
        "a shopping list"
    );
}

#[test]
#[ignore = "writes to the system clipboard"]
fn closing_the_window_takes_the_copy_back() {
    // Dropping the last handle is how the thread learns the window is gone.
    let clip = Clip::start(Duration::from_secs(60));
    clip.put(Zeroizing::new(SECRET.to_owned())).unwrap();
    drop(clip);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!holds(SECRET));
}
