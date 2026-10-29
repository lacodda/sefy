//! The system clipboard, as the front ends use it.
//!
//! These write to the real clipboard of the machine they run on, which would
//! throw away whatever a person running `cargo test` had copied - so they are
//! ignored by default and CI runs them on its Windows runner:
//!
//!     cargo test -p sefy-core --features clipboard --test clipboard -- --ignored --test-threads=1
//!
//! One at a time: the clipboard is one object for the whole desktop.

#![cfg(all(windows, feature = "clipboard"))]

use sefy_core::clipboard::Clipboard;

const SECRET: &str = "not-a-real-secret: correct horse battery staple";

/// Whether the clipboard currently carries the registered format `name`.
fn carries(name: &str) -> bool {
    let format = clipboard_win::register_format(name).expect("the format registers");
    clipboard_win::raw::is_format_avail(format.get())
}

#[test]
#[ignore = "writes to the system clipboard"]
fn a_secret_on_the_clipboard_is_kept_out_of_its_history_and_the_cloud() {
    let mut clipboard = Clipboard::open().unwrap();
    clipboard.put(SECRET).unwrap();
    assert!(clipboard.holds(SECRET));

    // What Win+V and the cloud clipboard read before they keep a copy, and
    // what clipboard managers read before they watch it. Without them the
    // secret outlives "clipboard cleared" in the history, and leaves the
    // machine with the cloud clipboard.
    for format in [
        "CanIncludeInClipboardHistory",
        "CanUploadToCloudClipboard",
        "ExcludeClipboardContentFromMonitorProcessing",
    ] {
        assert!(carries(format), "a copied secret does not carry {format}");
    }

    assert!(clipboard.take_back(SECRET));
    assert!(!clipboard.holds(SECRET));
}

#[test]
#[ignore = "writes to the system clipboard"]
fn taking_back_leaves_what_the_person_copied_since() {
    let mut clipboard = Clipboard::open().unwrap();
    clipboard.put(SECRET).unwrap();

    // Somebody copies something else before the wait is over.
    clipboard_win::set_clipboard_string("a shopping list").unwrap();

    assert!(!clipboard.take_back(SECRET));
    assert_eq!(
        clipboard_win::get_clipboard_string().unwrap(),
        "a shopping list"
    );
}

#[test]
#[ignore = "writes to the system clipboard"]
fn a_hold_takes_the_secret_back_when_it_ends() {
    let held = Clipboard::open().unwrap().hold(SECRET, 1).unwrap();
    assert!(held.cleared);
    assert!(!Clipboard::open().unwrap().holds(SECRET));
}

#[test]
#[ignore = "writes to the system clipboard"]
fn a_hold_of_zero_seconds_leaves_the_secret_where_it_is() {
    let held = Clipboard::open().unwrap().hold(SECRET, 0).unwrap();
    assert!(!held.cleared);
    let mut clipboard = Clipboard::open().unwrap();
    assert!(clipboard.take_back(SECRET));
}
