//! Asking the system for a file, without the system remembering which.
//!
//! Every file dialog keeps a memory. On Windows the common dialog writes the
//! file it opened into its own most-recent list in the registry, which the
//! next open dialog of any program offers back, and notes under the program's
//! name that the program used it — a record, kept under the user's profile,
//! of exactly the files the vault format makes unremarkable. The dialog is told
//! not to (`FOS_DONTADDTORECENT`, which also keeps the file out of the shell's
//! recent documents), and the state it keeps between uses is filed under
//! sefy's own client id rather than its name, and cleared as it closes. Each
//! of the three was measured by its absence: take one away and the registry
//! holds the trace (ADR 0010).
//!
//! Elsewhere the dialog is Tauri's, which offers none of these switches.

use std::path::{Path, PathBuf};

/// Shows the dialog over `owner`, opening in `folder`, and gives back the
/// chosen file, or nothing when the person cancels.
#[cfg(windows)]
pub fn file(owner: Option<isize>, folder: Option<&Path>) -> Option<PathBuf> {
    win32::file(owner, folder)
}

/// The one place this crate calls the operating system directly: the common
/// item dialog is COM, and COM is `unsafe` to call from Rust.
#[cfg(windows)]
#[allow(unsafe_code)]
mod win32 {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_DONTADDTORECENT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR,
        FOS_PATHMUSTEXIST, FileOpenDialog, IFileOpenDialog, IShellItem,
        SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{GUID, PCWSTR};

    /// The id the dialog files what it keeps between uses under, instead of
    /// under the name of the program — so that it can be cleared, which
    /// state filed under a program's name cannot.
    const CLIENT: GUID = GUID::from_u128(0x6f0c_2d41_9b5e_4a83_8d2c_51e7_a0b3_c94f);

    pub fn file(owner: Option<isize>, folder: Option<&Path>) -> Option<PathBuf> {
        // SAFETY: COM is initialised on this thread for the length of the
        // call and uninitialised on the way out, once every interface taken
        // from it has been dropped inside `show`.
        let initialised =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        let chosen = if initialised.is_ok() {
            show(owner, folder)
        } else {
            None
        };
        if initialised.is_ok() {
            unsafe { CoUninitialize() };
        }
        chosen
    }

    fn show(owner: Option<isize>, folder: Option<&Path>) -> Option<PathBuf> {
        // SAFETY: each call is on an interface created here, on the thread
        // that initialised COM; the strings passed in outlive the calls that
        // read them, and the one string handed back is freed after copying.
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            dialog.SetClientGuid(&CLIENT).ok()?;
            let options = dialog.GetOptions().ok()?;
            dialog
                .SetOptions(
                    options
                        | FOS_DONTADDTORECENT
                        | FOS_FORCEFILESYSTEM
                        | FOS_FILEMUSTEXIST
                        | FOS_PATHMUSTEXIST
                        | FOS_NOCHANGEDIR,
                )
                .ok()?;
            if let Some(folder) = folder {
                let wide: Vec<u16> = folder.as_os_str().encode_wide().chain([0]).collect();
                if let Ok(item) =
                    SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(wide.as_ptr()), None)
                {
                    let _ = dialog.SetFolder(&item);
                }
            }

            let shown = dialog.Show(owner.map(|handle| HWND(handle as *mut _)));
            let chosen = shown
                .ok()
                .and_then(|()| dialog.GetResult().ok())
                .and_then(|item| item.GetDisplayName(SIGDN_FILESYSPATH).ok())
                .map(|name| {
                    let path = PathBuf::from(String::from_utf16_lossy(name.as_wide()));
                    CoTaskMemFree(Some(name.0 as *const _));
                    path
                });
            // Whatever the dialog filed under the client id while it was open
            // goes with it.
            let _ = dialog.ClearClientData();
            chosen
        }
    }
}

/// Elsewhere: the system's dialog, through Tauri.
#[cfg(not(windows))]
pub fn file_through<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    folder: Option<&Path>,
) -> Option<PathBuf> {
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;

    let mut dialog = app.dialog().file();
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    if let Some(folder) = folder {
        dialog = dialog.set_directory(folder);
    }
    dialog
        .blocking_pick_file()
        .and_then(|picked| picked.into_path().ok())
}
