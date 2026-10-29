//! The desktop app of [sefy](https://github.com/lacodda/sefy): the same vault
//! as the command line, in a window.
//!
//! A thin layer over `sefy-core`. The vault is opened, held and locked in this
//! process; the page in the window draws what it is told and asks for what it
//! needs through [`commands`], whose answers are the [`wire`] types. Nothing
//! the page could do on its own touches the disk.

#![deny(unsafe_code)]

pub mod clip;
pub mod commands;
pub mod pick;
pub mod quick;
pub mod session;
pub mod wire;

use tauri::{Manager, RunEvent, WindowEvent};

/// Builds and runs the window.
pub fn run() {
    let opening = session::opening(std::env::args_os(), std::env::var_os("SEFY_VAULT"));

    let builder =
        tauri::Builder::default().plugin(tauri_plugin_global_shortcut::Builder::new().build());
    // Windows has a dialog of its own (`pick`); elsewhere it is the plugin's.
    #[cfg(not(windows))]
    let builder = builder.plugin(tauri_plugin_dialog::init());

    let app = builder
        .manage(session::Session::default())
        .manage(clip::Clip::standard())
        .manage(opening)
        .setup(|app| {
            let shortcut = quick::register(app.handle());
            app.manage(shortcut);
            Ok(())
        })
        .on_window_event(|window, event| {
            // The quick search window is a second window, and an application
            // whose last visible window closed would otherwise keep running
            // behind it. Closing the main window is closing sefy.
            if window.label() == quick::MAIN && matches!(event, WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::opening,
            commands::choose_file,
            commands::unlock,
            commands::lock,
            commands::status,
            commands::items,
            commands::tags,
            commands::item,
            commands::reveal,
            commands::copy,
            commands::policy,
            commands::dismiss,
        ])
        .build(tauri::generate_context!())
        .expect("the window could not be started");

    app.run(|app, event| {
        // A window that closes with a secret on the clipboard has no timer
        // left to take it back: on Windows and macOS it would stay there
        // until something else is copied. It is taken back on the way out.
        if let RunEvent::Exit = event {
            app.state::<clip::Clip>().take_back();
        }
    });
}
