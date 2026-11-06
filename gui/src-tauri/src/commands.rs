//! What the page may ask of this process, and nothing more.
//!
//! The page gets no file system, no dialog API, no shell and no clipboard: it
//! asks for a file to be chosen, a vault to be unlocked or locked, what the
//! open vault is and holds, one field to be shown or copied. Each answer is a
//! `wire` value.

use crate::clip::Clip;
use crate::session::{self, Handover, Session};
use crate::wire::{
    Copied, Failure, FailureKind, Filter, ItemCard, ItemRow, Opening, PasswordChanged,
    PasswordStrength, Policy, Revealed, Shortcut, TagCount, Transfer, Transport, Transports,
    VaultStatus, VersionRow, WipeOutcome, WipePart, WipePreview,
};
use sefy_core::agent::{Client, Seat};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use zeroize::Zeroizing;

/// The file the window was started on, if it was started on one.
#[tauri::command]
pub fn opening(opening: State<'_, Opening>) -> Opening {
    opening.inner().clone()
}

/// The file a second launch handed over, if there is one still to take.
#[tauri::command]
pub fn take_handover(handover: State<'_, Handover>) -> Option<String> {
    handover.take()
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
    let password = Zeroizing::new(password.into_bytes());
    let (vault, password) = tauri::async_runtime::spawn_blocking(move || {
        session::open(&PathBuf::from(path), &password).map(|vault| (vault, password))
    })
    .await
    .map_err(|_| Failure {
        kind: FailureKind::Other,
        message: "unlocking stopped before it finished".to_owned(),
    })??;
    let status = app.state::<Session>().hold(vault, password)?;
    crate::quick::make(&app);
    Ok(status)
}

/// Locks the open vault.
///
/// Everything the vault let out goes with it: what this window put on the
/// clipboard is taken back, and the quick search window - which has titles
/// in it - is destroyed rather than hidden.
#[tauri::command]
pub fn lock(app: AppHandle) {
    lock_everything(&app);
}

/// Locks the vault and everything it let out, and tells the page.
///
/// The one way it happens, whoever asks: the page's Lock button, the time
/// running out with nobody using it, `sefy lock`, a launch on another file. The
/// page asking is told as well, and is already there.
pub fn lock_everything(app: &AppHandle) {
    app.state::<Session>().lock();
    app.state::<Clip>().take_back();
    crate::quick::unmake(app);
    let _ = app.emit_to(crate::quick::MAIN, session::LOCKED, ());
}

/// Someone is using the window: the idle time starts over.
///
/// The page calls this when a person does something, not when it asks for
/// anything. What it asks for on its own, a card's one-time code running on,
/// says nothing about anyone being there.
#[tauri::command]
pub fn touch(session: State<'_, Session>) {
    session.touch();
}

// What asks about the vault is async, and so off the window's own thread: the
// vault can be busy with a write, which seals with a stretched password, and the
// window keeps drawing while it is.

/// What and where the open vault is.
#[tauri::command]
pub async fn status(session: State<'_, Session>) -> Result<VaultStatus, Failure> {
    session.status()
}

/// The items `filter` lets through, best match first.
#[tauri::command]
pub async fn items(session: State<'_, Session>, filter: Filter) -> Result<Vec<ItemRow>, Failure> {
    session.items(&filter)
}

/// Every tag in use, with how many items carry it.
#[tauri::command]
pub async fn tags(session: State<'_, Session>) -> Result<Vec<TagCount>, Failure> {
    session.tags()
}

/// The item with `id`, as the card shows it: no secret in it.
#[tauri::command]
pub async fn item(session: State<'_, Session>, id: i64) -> Result<ItemCard, Failure> {
    session.card(id, now() as i64)
}

/// Every version of an item, oldest first, as the card shows them: no secret in
/// them.
#[tauri::command]
pub async fn history(session: State<'_, Session>, id: i64) -> Result<Vec<VersionRow>, Failure> {
    session.history(id, now() as i64)
}

/// Brings back an earlier version of an item, and says whether anything changed.
#[tauri::command]
pub async fn restore(app: AppHandle, id: i64, version: String) -> Result<bool, Failure> {
    worker(app.clone(), move |app| {
        app.state::<Session>().restore(id, &version)
    })
    .await
    .inspect(|_| tell_changed(&app))
}

/// One secret field of an item, because someone asked to see it; of an earlier
/// version, when one is named.
#[tauri::command]
pub async fn reveal(
    session: State<'_, Session>,
    id: i64,
    field: String,
    version: Option<String>,
) -> Result<Revealed, Failure> {
    let (value, _) = session.value_of(id, version.as_deref(), Some(&field), now())?;
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
/// `field` names the field; without one it is what `sefy get` takes. From an
/// earlier version, when one is named.
#[tauri::command]
pub async fn copy(
    app: AppHandle,
    id: i64,
    field: Option<String>,
    version: Option<String>,
) -> Result<Copied, Failure> {
    let (value, what) =
        app.state::<Session>()
            .value_of(id, version.as_deref(), field.as_deref(), now())?;
    let seconds = app.state::<Session>().policy()?.clipboard_seconds;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Clip>()
            .put(value, std::time::Duration::from_secs(seconds))
    })
    .await
    .map_err(|_| Failure {
        kind: FailureKind::Other,
        message: "copying stopped before it finished".to_owned(),
    })??;
    Ok(Copied { what, seconds })
}

/// How long a copy stays on the clipboard, how long the vault stays unlocked
/// with nobody using it, what those can be set to, and the quick search shortcut.
#[tauri::command]
pub async fn policy(
    session: State<'_, Session>,
    shortcut: State<'_, Shortcut>,
) -> Result<Policy, Failure> {
    let policy = session.policy()?;
    Ok(Policy {
        clipboard_seconds: policy.clipboard_seconds,
        lock_after_seconds: policy.lock_after_seconds,
        clipboard_choices: sefy_core::policy::CLIPBOARD_CHOICES.to_vec(),
        lock_after_choices: sefy_core::policy::LOCK_AFTER_CHOICES.to_vec(),
        quick: shortcut.inner().clone(),
    })
}

/// Sets how long a copy stays on the clipboard and how long the vault stays
/// unlocked, in the vault, to values on offer.
#[tauri::command]
pub async fn set_policy(
    app: AppHandle,
    lock_after_seconds: u64,
    clipboard_seconds: u64,
) -> Result<(), Failure> {
    worker(app.clone(), move |app| {
        app.state::<Session>().set_policy(sefy_core::Policy {
            lock_after_seconds,
            clipboard_seconds,
        })
    })
    .await
    .inspect(|()| tell_changed(&app))
}

/// How hard a password would be to guess. Worked out here and offline, for the
/// master password dialog: nothing about it is looked up anywhere.
#[tauri::command]
pub async fn strength(password: String) -> PasswordStrength {
    let password = Zeroizing::new(password);
    let estimate = sefy_core::estimate(&password, &[]);
    PasswordStrength {
        score: estimate.score,
        warning: estimate.warning,
    }
}

/// Changes the master password, which takes knowing the current one. An agent
/// holding the old password has nothing left to answer with, and is locked.
#[tauri::command]
pub async fn change_password(
    app: AppHandle,
    current: String,
    new: String,
) -> Result<PasswordChanged, Failure> {
    let current = Zeroizing::new(current);
    let new = Zeroizing::new(new);
    let change = worker(app.clone(), move |app| {
        app.state::<Session>()
            .change_password(current.as_bytes(), new.as_bytes())
    })
    .await?;
    tell_changed(&app);
    Ok(PasswordChanged {
        resealed: change.resealed,
        left: change
            .left
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        agent_locked: lock_the_agent(),
    })
}

/// Tells a separate `sefy agent`, if one runs, to wipe what it holds. The window
/// itself is the other seat, and is seen to by whoever calls this.
fn lock_the_agent() -> bool {
    Client::of(Seat::Agent).and_then(|client| client.lock()) == Some(true)
}

/// What a wipe of the open vault would destroy.
#[tauri::command]
pub async fn wipe_preview(session: State<'_, Session>) -> Result<WipePreview, Failure> {
    let status = session.status()?;
    let path = PathBuf::from(&status.path);
    Ok(WipePreview {
        name: status.name,
        directory: path
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_default(),
        parts: sefy_core::wipe::parts(&path)
            .iter()
            .map(WipePart::of)
            .collect(),
        remote: status.last_sync.map(|stamp| stamp.transport),
    })
}

/// Destroys the open vault, once its name is typed back and its password typed
/// again.
///
/// There is no way to say yes in advance, here or on the command line. Everything
/// is locked before the first file is touched: a wipe is the wrong moment to
/// leave a password in a process that outlives it.
#[tauri::command]
pub async fn wipe(app: AppHandle, password: String, name: String) -> Result<WipeOutcome, Failure> {
    let password = Zeroizing::new(password);
    worker(app, move |app| {
        let vault = app
            .state::<Session>()
            .open_to_wipe(password.as_bytes(), &name)?;
        lock_everything(&app);
        lock_the_agent();
        let wiped = vault.wipe();
        Ok(WipeOutcome {
            destroyed: wiped.destroyed.len(),
            vanished: wiped.vanished.len(),
            failed: wiped
                .failed
                .iter()
                .map(|(_, error)| error.to_string())
                .collect(),
            remote: wiped.last_sync.map(|stamp| stamp.transport),
        })
    })
    .await
}

/// The transports on this machine. Asks each what it is, which starts a program
/// per transport, so it is not done on the window's thread.
#[tauri::command]
pub async fn transports() -> Result<Transports, Failure> {
    tauri::async_runtime::spawn_blocking(|| Transports {
        found: sefy_core::plugin::discover()
            .iter()
            .map(Transport::of)
            .collect(),
        directory: sefy_core::plugin::plugin_directory().map(|path| path.display().to_string()),
        remote: sefy_core::sync::remote_name(),
    })
    .await
    .map_err(|_| stopped())
}

/// The transport called `name`, or why not.
fn transport_called(name: &str) -> Result<sefy_core::Plugin, Failure> {
    sefy_core::plugin::find(sefy_core::plugin::discover(), name).ok_or_else(|| Failure {
        kind: FailureKind::Other,
        message: format!("no transport called {name:?}"),
    })
}

/// Pull, then push, through the transport called `transport`. With a password,
/// the copy at the remote is opened with that.
#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    transport: String,
    remote_password: Option<String>,
) -> Result<Transfer, Failure> {
    let remote_password = remote_password.map(Zeroizing::new);
    worker(app.clone(), move |app| {
        let plugin = transport_called(&transport)?;
        app.state::<Session>().sync_through(
            &plugin,
            remote_password.as_ref().map(|password| password.as_bytes()),
            now() as i64,
        )
    })
    .await
    .inspect(|_| tell_changed(&app))
}

/// Sends this vault up through the transport called `transport`, replacing what
/// is there.
#[tauri::command]
pub async fn push_now(app: AppHandle, transport: String) -> Result<Transfer, Failure> {
    worker(app.clone(), move |app| {
        let plugin = transport_called(&transport)?;
        app.state::<Session>().push_through(&plugin, now() as i64)
    })
    .await
    .inspect(|_| tell_changed(&app))
}

/// What the last transfer did, since the vault was unlocked.
#[tauri::command]
pub async fn last_transfer(session: State<'_, Session>) -> Result<Option<Transfer>, Failure> {
    Ok(session.last_transfer())
}

/// Runs something that takes its time on a thread that may wait.
async fn worker<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(AppHandle) -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    tauri::async_runtime::spawn_blocking(move || work(app))
        .await
        .map_err(|_| stopped())?
}

fn stopped() -> Failure {
    Failure {
        kind: FailureKind::Other,
        message: "that stopped before it finished".to_owned(),
    }
}

/// What the page shows has moved on: it asks again.
pub fn tell_changed(app: &AppHandle) {
    let _ = app.emit_to(crate::quick::MAIN, session::CHANGED, ());
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
