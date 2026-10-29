//! Quick search: a line over whatever is on the screen, one shortcut away.
//!
//! The shortcut belongs to the whole desktop for as long as the window runs.
//! Pressed with a vault open, it brings up a small window with a search line;
//! typing finds, Enter puts the item's secret on the clipboard and the window
//! goes away again. Pressed with no vault open, it brings the main window
//! forward to unlock one - quick search never asks for the master password
//! itself, so there is one place the password is typed, not two.
//!
//! The small window exists only while a vault is open: it is made at unlock
//! and destroyed at lock, so no title found in it outlives the lock in a
//! hidden page.

use crate::session::Session;
use crate::wire::Shortcut;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// The quick search window's label, in `tauri.conf.json` and the
/// capabilities.
pub const LABEL: &str = "quick";

/// The main window's label.
pub const MAIN: &str = "main";

/// The event the quick window is sent each time it is brought up, so it
/// starts from an empty line rather than from the last search.
pub const SHOWN: &str = "quick-shown";

/// The keys, as the system is asked for them and as a person reads them.
///
/// Ctrl+Alt+Space on Windows and Linux: the system itself leaves it alone,
/// where Alt+Space is the window menu and Ctrl+Shift+Space the parameter hints
/// of the common editors. On macOS Control+Option+Space selects the next
/// input source, so it is Command+Shift+Space there.
#[cfg(not(target_os = "macos"))]
pub const KEYS: (&str, &str) = ("Control+Alt+Space", "Ctrl+Alt+Space");
#[cfg(target_os = "macos")]
pub const KEYS: (&str, &str) = ("Super+Shift+Space", "Cmd+Shift+Space");

/// Asks the system for the shortcut, and says whether it was given.
///
/// Another program may hold the same keys; then the window works as before,
/// without quick search, and says so rather than failing to start.
pub fn register(app: &AppHandle) -> Shortcut {
    let active = app
        .global_shortcut()
        .on_shortcut(KEYS.0, |app, _, event| {
            if event.state == ShortcutState::Pressed {
                summon(app);
            }
        })
        .is_ok();
    Shortcut {
        keys: KEYS.1.to_owned(),
        active,
    }
}

/// What the shortcut does: quick search over an open vault, the main window
/// over a locked one.
pub fn summon(app: &AppHandle) {
    if !app.state::<Session>().is_open() {
        bring_forward(app);
        return;
    }
    let Some(quick) = app.get_webview_window(LABEL) else {
        bring_forward(app);
        return;
    };
    // Pressed again while it is up, the shortcut puts it away: the same keys
    // that called it can dismiss it.
    if quick.is_visible().unwrap_or(false) {
        let _ = quick.hide();
        return;
    }
    let _ = quick.center();
    let _ = quick.show();
    let _ = quick.set_focus();
    let _ = app.emit_to(LABEL, SHOWN, ());
}

/// Brings the main window to the front, restored if it was minimized.
pub fn bring_forward(app: &AppHandle) {
    if let Some(main) = app.get_webview_window(MAIN) {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}

/// Makes the quick window, hidden, from its configuration; once a vault is
/// open. Does nothing if it is already there.
pub fn make(app: &AppHandle) {
    if app.get_webview_window(LABEL).is_some() {
        return;
    }
    let Some(config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == LABEL)
        .cloned()
    else {
        return;
    };
    let Ok(builder) = WebviewWindowBuilder::from_config(app, &config) else {
        return;
    };
    if let Ok(quick) = builder.build() {
        put_away_on_blur(&quick);
    }
}

/// Destroys the quick window, if there is one; at lock.
pub fn unmake(app: &AppHandle) {
    if let Some(quick) = app.get_webview_window(LABEL) {
        let _ = quick.destroy();
    }
}

/// Hides the quick window; once something is copied, or on Escape.
pub fn hide(app: &AppHandle) {
    if let Some(quick) = app.get_webview_window(LABEL) {
        let _ = quick.hide();
    }
}

/// A line over the screen that stayed after the person clicked elsewhere
/// would be a window to close by hand. It goes the moment it loses focus.
fn put_away_on_blur(quick: &WebviewWindow) {
    let window = quick.clone();
    quick.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            let _ = window.hide();
        }
    });
}
