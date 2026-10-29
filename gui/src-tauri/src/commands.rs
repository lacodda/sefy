//! What the page may ask of this process, and nothing more.
//!
//! The page gets no file system, no dialog API, no shell and no clipboard: it
//! asks for a file to be chosen, a vault to be unlocked or locked, what the
//! open vault is and holds, one field to be shown or copied. Each answer is a
//! `wire` value.

use crate::clip::Clip;
use crate::session::{self, Session};
use crate::wire::{
    Copied, Failure, FailureKind, Filter, ItemCard, ItemRow, Opening, Policy, Revealed, Shortcut,
    TagCount, VaultStatus,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
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
    let status = app.state::<Session>().hold(vault)?;
    crate::quick::make(&app);
    Ok(status)
}

/// Locks the open vault.
///
/// Everything the vault let out goes with it: what this window put on the
/// clipboard is taken back, and the quick search window - which has titles
/// in it - is destroyed rather than hidden.
#[tauri::command]
pub fn lock(app: AppHandle, session: State<'_, Session>, clip: State<'_, Clip>) {
    session.lock();
    clip.take_back();
    crate::quick::unmake(&app);
}

/// What and where the open vault is.
#[tauri::command]
pub fn status(session: State<'_, Session>) -> Result<VaultStatus, Failure> {
    session.status()
}

/// The items `filter` lets through, best match first.
#[tauri::command]
pub fn items(session: State<'_, Session>, filter: Filter) -> Result<Vec<ItemRow>, Failure> {
    session.items(&filter)
}

/// Every tag in use, with how many items carry it.
#[tauri::command]
pub fn tags(session: State<'_, Session>) -> Result<Vec<TagCount>, Failure> {
    session.tags()
}

/// The item with `id`, as the card shows it: no secret in it.
#[tauri::command]
pub fn item(session: State<'_, Session>, id: i64) -> Result<ItemCard, Failure> {
    session.card(id, now() as i64)
}

/// One secret field of an item, because someone asked to see it.
#[tauri::command]
pub fn reveal(session: State<'_, Session>, id: i64, field: String) -> Result<Revealed, Failure> {
    let (value, _) = session.value(id, Some(&field), now())?;
    // Into the page it goes: from here on it is a string in a webview, which
    // no one can wipe. That is the cost of showing a secret, and why one is
    // shown only on request.
    Ok(Revealed {
        value: value.as_str().to_owned(),
    })
}

/// Puts one field of an item on the clipboard, never through the page, and
/// takes it back after the hold every front end uses.
///
/// `field` names the field; without one it is what `sefy get` takes.
#[tauri::command]
pub async fn copy(app: AppHandle, id: i64, field: Option<String>) -> Result<Copied, Failure> {
    let (value, what) = app.state::<Session>().value(id, field.as_deref(), now())?;
    tauri::async_runtime::spawn_blocking(move || app.state::<Clip>().put(value))
        .await
        .map_err(|_| Failure {
            kind: FailureKind::Other,
            message: "copying stopped before it finished".to_owned(),
        })??;
    Ok(Copied {
        what,
        seconds: sefy_core::clipboard::HOLD_SECONDS,
    })
}

/// How long a copy stays on the clipboard, and the quick search shortcut.
#[tauri::command]
pub fn policy(shortcut: State<'_, Shortcut>) -> Policy {
    Policy {
        clipboard_seconds: sefy_core::clipboard::HOLD_SECONDS,
        quick: shortcut.inner().clone(),
    }
}

/// Puts the quick search window away.
#[tauri::command]
pub fn dismiss(app: AppHandle) {
    crate::quick::hide(&app);
}

/// Unix seconds now. A clock before 1970 is a clock that cannot make a
/// one-time code either, and reads as 0.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}
