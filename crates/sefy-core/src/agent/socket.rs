//! Where an agent listens, and who can reach it.
//!
//! A Unix domain socket in a directory only its owner can enter, or a named
//! pipe only its owner may open. Either way the system keeps other users out
//! before a byte is read.

use super::Seat;
use interprocess::ConnectWaitMode;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ConnectOptions, Listener, ListenerOptions, Name, Stream};
use std::io;
use std::time::Duration;

/// Names where agents listen, in place of the usual place: on Windows the
/// start of a pipe's name, elsewhere the start of a socket's path.
///
/// For keeping more than one apart. Tests set it, so that they never reach an
/// agent holding someone's real password.
pub const SOCKET_ENV: &str = "SEFY_AGENT_SOCKET";

/// How long a client waits for an agent before asking for the password
/// instead. Long enough for a slow machine to stretch one twice.
pub(super) const PATIENCE: Duration = Duration::from_secs(30);

/// Where one seat listens.
#[derive(Debug, Clone)]
pub(super) struct Address(String);

impl Address {
    pub(super) fn of(seat: Seat) -> Option<Self> {
        let base = std::env::var(SOCKET_ENV)
            .ok()
            .filter(|base| !base.is_empty());
        match base {
            Some(base) => Some(Self(format!("{base}-{}", seat.name()))),
            None => standard(seat).map(Self),
        }
    }
}

/// The pipe's name. Its namespace is the whole machine's, so the user's name
/// keeps two people's agents from each finding the other's name taken.
#[cfg(windows)]
fn standard(seat: Seat) -> Option<String> {
    Some(match std::env::var("USERNAME") {
        Ok(user) if !user.is_empty() => format!("sefy-{}-{user}", seat.name()),
        _ => format!("sefy-{}", seat.name()),
    })
}

/// The socket's path. The system's per-user runtime directory where there is
/// one; otherwise a directory of sefy's own under the user's data directory,
/// as gpg-agent falls back to its home.
#[cfg(unix)]
fn standard(seat: Seat) -> Option<String> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|path| path.join("sefy-agent"));
    let directory = runtime
        .or_else(|| crate::plugin::data_directory().map(|path| path.join("sefy").join("agent")))?;
    directory
        .join(format!("{}.sock", seat.name()))
        .to_str()
        .map(str::to_owned)
}

#[cfg(windows)]
fn name(address: &Address) -> io::Result<Name<'_>> {
    use interprocess::local_socket::GenericNamespaced;
    address.0.as_str().to_ns_name::<GenericNamespaced>()
}

#[cfg(unix)]
fn name(address: &Address) -> io::Result<Name<'_>> {
    use interprocess::local_socket::GenericFilePath;
    address.0.as_str().to_fs_name::<GenericFilePath>()
}

/// Connects to whoever listens there.
pub(super) fn connect(address: &Address) -> io::Result<Stream> {
    ConnectOptions::new()
        .name(name(address)?)
        .wait_mode(ConnectWaitMode::Timeout(PATIENCE))
        .connect_sync()
}

/// Starts listening, for this user alone.
#[cfg(windows)]
pub(super) fn listen(address: &Address) -> io::Result<Listener> {
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;

    // A pipe with no descriptor of its own lets anyone on the machine open it
    // for reading. This one grants all access to its owner and to nobody
    // else, with the rights inherited from nothing.
    let only_the_owner = widestring::U16CString::from_str("D:P(A;;GA;;;OW)")
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    ListenerOptions::new()
        .name(name(address)?)
        .security_descriptor(SecurityDescriptor::deserialize(&only_the_owner)?)
        .create_sync()
}

/// Starts listening, for this user alone: the directory is what keeps others
/// out, and a directory that would not is refused.
#[cfg(unix)]
pub(super) fn listen(address: &Address) -> io::Result<Listener> {
    use std::path::Path;

    let path = Path::new(&address.0);
    if let Some(directory) = path.parent() {
        private_directory(directory)?;
    }
    match ListenerOptions::new().name(name(address)?).create_sync() {
        // A socket file with nobody behind it is what a crash leaves.
        Err(error) if error.kind() == io::ErrorKind::AddrInUse && connect(address).is_err() => {
            std::fs::remove_file(path)?;
            ListenerOptions::new().name(name(address)?).create_sync()
        }
        other => other,
    }
}

/// Makes sure `directory` is one only this user can enter, creating it if it
/// has to be.
#[cfg(unix)]
fn private_directory(directory: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    // Not `metadata`: a link would be judged by what it points at.
    let found = std::fs::symlink_metadata(directory)?;
    if !found.is_dir() || found.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} can be entered by other users; an agent will not listen there",
                directory.display()
            ),
        ));
    }
    Ok(())
}

/// Removes what listening left on disk, which is nothing on Windows: a pipe
/// goes with its process. Everything else about an agent ends with the
/// process, and a socket file in a user's directory would say sefy was there.
pub(super) fn leave(address: &Address) {
    #[cfg(unix)]
    {
        let path = std::path::Path::new(&address.0);
        let _ = std::fs::remove_file(path);
        if let Some(directory) = path.parent() {
            // Only if the other seat has gone too: this fails on a directory
            // with anything in it.
            let _ = std::fs::remove_dir(directory);
        }
    }
    #[cfg(windows)]
    let _ = address;
}
