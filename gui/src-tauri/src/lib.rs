//! The desktop app of [sefy](https://github.com/lacodda/sefy): the same vault
//! as the command line, in a window.
//!
//! A thin layer over `sefy-core`. The vault is opened, held and locked in this
//! process; the page in the window draws what it is told and asks for what it
//! needs through [`commands`], whose answers are the [`wire`] types. Nothing
//! the page could do on its own touches the disk.

#![deny(unsafe_code)]

pub mod commands;
pub mod pick;
pub mod session;
pub mod wire;

/// Builds and runs the window.
pub fn run() {
    let opening = session::opening(std::env::args_os(), std::env::var_os("SEFY_VAULT"));

    let builder = tauri::Builder::default();
    // Windows has a dialog of its own (`pick`); elsewhere it is the plugin's.
    #[cfg(not(windows))]
    let builder = builder.plugin(tauri_plugin_dialog::init());

    builder
        .manage(session::Session::default())
        .manage(opening)
        .invoke_handler(tauri::generate_handler![
            commands::opening,
            commands::choose_file,
            commands::unlock,
            commands::lock,
            commands::status,
        ])
        .run(tauri::generate_context!())
        .expect("the window could not be started");
}
