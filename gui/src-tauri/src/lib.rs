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

use sefy_core::agent::{self, Seat};
use std::path::Path;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

/// Builds and runs the window.
pub fn run() {
    let opening = session::opening(std::env::args_os(), std::env::var_os("SEFY_VAULT"));

    let builder = tauri::Builder::default()
        // First, before anything else registers: a second launch hands its
        // file to this window and exits inside this plugin, without building a
        // window of its own.
        .plugin(tauri_plugin_single_instance::init(
            |app, args, launched_in| {
                if let Some(file) = session::handed_over(&args, Path::new(&launched_in)) {
                    // Another file is someone asking for another vault: the one
                    // open here does not stay unlocked behind it. The same file is
                    // only someone looking for the window.
                    if app
                        .state::<session::Session>()
                        .holds_other_than(Path::new(&file))
                    {
                        commands::lock_everything(app);
                    }
                    app.state::<session::Handover>().offer(file);
                    let _ = app.emit_to(quick::MAIN, session::HANDED_OVER, ());
                }
                quick::bring_forward(app);
            },
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());
    // Windows has a dialog of its own (`pick`); elsewhere it is the plugin's.
    #[cfg(not(windows))]
    let builder = builder.plugin(tauri_plugin_dialog::init());

    let app = builder
        .manage(session::Session::default())
        .manage(session::Handover::default())
        .manage(clip::Clip::standard())
        .manage(opening)
        .setup(|app| {
            let shortcut = quick::register(app.handle());
            app.manage(shortcut);

            // The clock runs whatever happens below: a window that cannot
            // listen for the command line still locks when nobody is using it.
            let keeper = app.state::<session::Session>().keeper();
            let handle = app.handle().clone();
            keeper.when_locked(move || commands::lock_everything(&handle));
            agent::start_ticking(keeper.clone());
            // A window that cannot listen works as before: the command line
            // goes on asking for the password, which is all it loses.
            let _ = agent::serve(Seat::Window, keeper);
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
            commands::take_handover,
            commands::touch,
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
            agent::leave(Seat::Window);
        }
    });
}
