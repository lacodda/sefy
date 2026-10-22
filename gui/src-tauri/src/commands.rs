//! What the page may ask of this process, and nothing more.
//!
//! The page gets no file system, no dialog API and no shell: it asks for a
//! file to be chosen, a vault to be unlocked or locked, and what the open
//! vault is. Each answer is a `wire` value.

use crate::session::{self, Session};
use crate::wire::{Failure, FailureKind, Opening, VaultStatus};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroizing;

/// The file the window was started on, if it was started on one.
#[tauri::command]
pub fn opening(opening: State<'_, Opening>) -> Opening {
    opening.inner().clone()
}

/// Asks the system for a file, starting in the folder of `near`.
///
/// No filter, no extension, no title naming what the file is for: a vault is
/// any file, and the dialog shows every file as the same kind of thing. Nor
/// does the system keep a note of it afterwards — see [`crate::pick`].
#[tauri::command]
pub async fn choose_file(app: AppHandle, near: Option<String>) -> Option<String> {
    let folder = near
        .map(PathBuf::from)
        .and_then(|path| path.parent().map(PathBuf::from))
        .filter(|folder| folder.is_dir());
    #[cfg(windows)]
    let owner = app
        .get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .map(|handle| handle.0 as isize);

    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let chosen = crate::pick::file(owner, folder.as_deref());
        #[cfg(not(windows))]
        let chosen = crate::pick::file_through(&app, folder.as_deref());
        chosen.map(|path| path.display().to_string())
    })
    .await
    .ok()
    .flatten()
}

/// Unlocks the vault at `path` with `password`, and says what it is.
///
/// The password is stretched on a worker thread — it takes the better part of
/// a second on purpose — so the window keeps drawing while it does.
#[tauri::command]
pub async fn unlock(
    app: AppHandle,
    path: String,
    password: String,
) -> Result<VaultStatus, Failure> {
    let password = Zeroizing::new(password);
    let vault = tauri::async_runtime::spawn_blocking(move || {
        session::open(&PathBuf::from(path), password.as_bytes())
    })
    .await
    .map_err(|_| Failure {
        kind: FailureKind::Other,
        message: "unlocking stopped before it finished".to_owned(),
    })??;
    app.state::<Session>().hold(vault)
}

/// Locks the open vault.
#[tauri::command]
pub fn lock(session: State<'_, Session>) {
    session.lock();
}

/// What and where the open vault is.
#[tauri::command]
pub fn status(session: State<'_, Session>) -> Result<VaultStatus, Failure> {
    session.status()
}
