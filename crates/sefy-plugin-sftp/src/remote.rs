//! Moving the sealed vault to a server over SSH and back.
//!
//! The remote side is a plain directory holding one file per name — no history,
//! no working copy, nothing beside the blob. That is the whole difference from
//! the git transport: there is no earlier version to fall back on, so a push
//! replaces what was there and the previous contents are gone from the server.

use crate::ssh;
use std::path::{Path, PathBuf};

/// Environment variable naming where the vault is kept.
///
/// `[user@]host:/path/to/directory` — the same shape scp takes, because that is
/// what people already have written down somewhere.
pub const DESTINATION: &str = "SEFY_SFTP_DESTINATION";

/// What went wrong.
#[derive(Debug)]
pub enum Error {
    /// The request could not be understood.
    BadRequest(String),
    /// `SEFY_SFTP_DESTINATION` is not set, or is not the shape scp takes.
    BadDestination(String),
    /// The name sefy chose cannot be used as a file name on the far side.
    UnusableName(String),
    /// The server holds no copy under that name.
    NothingThere(String),
    /// A local file could not be read or written.
    Io {
        /// What was being attempted.
        what: String,
        /// The underlying failure.
        source: std::io::Error,
    },
    /// ssh or scp refused.
    Ssh(ssh::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(reason) => {
                write!(formatter, "the request could not be read: {reason}")
            }
            Self::BadDestination(reason) => write!(
                formatter,
                "{DESTINATION} {reason}\n\
                 set it to where the vault should live, for example:\n\
                 \x20 {DESTINATION}=you@server.example:/home/you/backups"
            ),
            Self::UnusableName(name) => write!(
                formatter,
                "{name:?} cannot be used as a file name on the server\n\
                 pick a remote name without a slash or a leading dot: sefy push --name <NAME>"
            ),
            Self::NothingThere(name) => write!(
                formatter,
                "the server holds no copy called {name:?} yet\n\
                 push from a machine that has the vault first"
            ),
            Self::Io { what, source } => write!(formatter, "{what}: {source}"),
            Self::Ssh(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for Error {}

impl From<ssh::Error> for Error {
    fn from(error: ssh::Error) -> Self {
        Self::Ssh(error)
    }
}

/// What sefy asks for.
#[derive(Debug, serde::Deserialize)]
struct Request {
    operation: String,
    file: PathBuf,
    name: String,
}

/// Where the vault lives on the far side.
#[derive(Debug, PartialEq, Eq)]
pub struct Destination {
    /// What ssh and scp are given as the host part: `you@server` or `server`.
    pub host: String,
    /// Directory on the server, without a trailing slash.
    pub directory: String,
}

impl Destination {
    /// Reads `[user@]host:/path` the way scp does.
    ///
    /// Split on the *first* colon after the host part, which is also how scp
    /// reads it. A Windows path like `C:\vaults` is therefore not a
    /// destination — and saying so beats treating `C` as a hostname and
    /// failing minutes later with a name resolution error.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(Error::BadDestination("is not set".into()));
        }

        let Some((host, directory)) = raw.split_once(':') else {
            return Err(Error::BadDestination(format!(
                "is {raw:?}, which carries no directory; it needs the form host:/path"
            )));
        };

        if host.is_empty() {
            return Err(Error::BadDestination(format!(
                "is {raw:?}, which names no server"
            )));
        }
        if host.len() == 1 && host.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(Error::BadDestination(format!(
                "is {raw:?}, which looks like a Windows path rather than a server; \
                 it needs the form host:/path"
            )));
        }

        let directory = directory.trim_end_matches('/');
        if directory.is_empty() {
            return Err(Error::BadDestination(format!(
                "is {raw:?}, which carries no directory; it needs the form host:/path"
            )));
        }

        Ok(Self {
            host: host.to_owned(),
            directory: directory.to_owned(),
        })
    }

    /// The scp argument for one file in this directory.
    fn path_of(&self, name: &str) -> String {
        format!("{}:{}/{}", self.host, self.directory, name)
    }
}

/// Carries out one request and returns the line to show the user.
pub fn handle(raw: &str) -> Result<String, Error> {
    let request: Request =
        serde_json::from_str(raw.trim()).map_err(|error| Error::BadRequest(error.to_string()))?;

    let destination = Destination::parse(
        &std::env::var(DESTINATION).map_err(|_| Error::BadDestination("is not set".into()))?,
    )?;

    check_name(&request.name)?;

    match request.operation.as_str() {
        "push" => push(&destination, &request),
        "pull" => pull(&destination, &request),
        other => Err(Error::BadRequest(format!("unknown operation {other:?}"))),
    }
}

/// Whether the name sefy chose is safe to use as a file name over there.
///
/// A name is checked rather than escaped. It comes from `sefy push --name`, so
/// the person choosing it can pick another one — and a name that has to be
/// quoted to be safe is a name that will be mishandled by the next tool that
/// touches the directory.
fn check_name(name: &str) -> Result<(), Error> {
    let unusable = name.is_empty()
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "'\"`$;&|<>*?~()[]{}!#".contains(c));

    if unusable {
        return Err(Error::UnusableName(name.to_owned()));
    }
    Ok(())
}

/// Uploads the sealed vault, replacing what was there.
///
/// Two steps on purpose. scp writes straight into the destination file, so an
/// interrupted upload would leave a truncated blob where the only remote copy
/// used to be — and a truncated vault does not open at all. The bytes land
/// under a temporary name first and are moved into place with `mv`, which is
/// atomic within one filesystem.
fn push(destination: &Destination, request: &Request) -> Result<String, Error> {
    let size = std::fs::metadata(&request.file)
        .map_err(|source| Error::Io {
            what: "cannot read the vault".to_owned(),
            source,
        })?
        .len();

    let staging = format!("{}.incoming", request.name);
    ssh::scp(
        &local_path(&request.file)?,
        &destination.path_of(&staging),
        "uploading the vault",
    )?;

    ssh::ssh(
        &destination.host,
        &format!(
            "mv -- {directory}/{staging} {directory}/{name}",
            directory = destination.directory,
            staging = staging,
            name = request.name
        ),
        "putting the vault in place",
    )
    // A failed move leaves the staging file behind; clearing it keeps the
    // directory from filling up with half-finished uploads. Best effort: the
    // failure worth reporting is the one above.
    .inspect_err(|_| {
        let _ = ssh::ssh(
            &destination.host,
            &format!("rm -f -- {}/{}", destination.directory, staging),
            "clearing the staging file",
        );
    })?;

    Ok(format!("pushed {:?} ({})", request.name, human(size)))
}

/// Downloads the server's copy to where sefy asked for it.
fn pull(destination: &Destination, request: &Request) -> Result<String, Error> {
    // Asked about first, so "nothing has been pushed yet" is answered in those
    // words rather than through whatever scp says about a missing file.
    let listing = ssh::ssh(
        &destination.host,
        &format!(
            "test -f {}/{} && echo present || echo absent",
            destination.directory, request.name
        ),
        "looking for the vault on the server",
    )?;

    if listing.trim() != "present" {
        return Err(Error::NothingThere(request.name.clone()));
    }

    ssh::scp(
        &destination.path_of(&request.name),
        &local_path(&request.file)?,
        "downloading the vault",
    )?;

    let size = std::fs::metadata(&request.file).map(|data| data.len()).ok();
    Ok(match size {
        Some(bytes) => format!("fetched {:?} ({})", request.name, human(bytes)),
        None => format!("fetched {:?}", request.name),
    })
}

/// The local side of an scp argument.
///
/// Passed through as it is: current OpenSSH recognises a Windows drive letter
/// and does not mistake `C:\vaults\notes.bak` for a host called `C` — verified
/// against the system scp rather than assumed. Only a path that is not valid
/// UTF-8 has to be refused, since a command line carries text.
fn local_path(path: &Path) -> Result<String, Error> {
    path.to_str().map(str::to_owned).ok_or_else(|| Error::Io {
        what: format!("cannot use the path {}", path.display()),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the path is not valid UTF-8",
        ),
    })
}

/// A byte count in the units a person reads.
fn human(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let bytes = bytes as f64;

    if bytes < KIB {
        return format!("{bytes:.0} B");
    }
    if bytes < KIB * KIB {
        return format!("{:.1} KiB", bytes / KIB);
    }
    format!("{:.1} MiB", bytes / (KIB * KIB))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(raw: &str) -> Destination {
        Destination::parse(raw).unwrap_or_else(|error| panic!("{raw:?} must parse: {error}"))
    }

    #[test]
    fn a_destination_is_read_the_way_scp_reads_one() {
        let destination = parsed("you@server.example:/home/you/backups");

        assert_eq!(destination.host, "you@server.example");
        assert_eq!(destination.directory, "/home/you/backups");
    }

    #[test]
    fn a_destination_without_a_user_is_fine() {
        // ssh fills the username in from ~/.ssh/config or the local account,
        // which is how most people have it written down.
        let destination = parsed("server:/srv/vaults");

        assert_eq!(destination.host, "server");
        assert_eq!(destination.directory, "/srv/vaults");
    }

    #[test]
    fn a_trailing_slash_does_not_become_a_double_one() {
        // Otherwise every remote path would carry "//" in the middle, which
        // works but shows up in every message the server prints back.
        let destination = parsed("server:/srv/vaults/");

        assert_eq!(destination.path_of("vault"), "server:/srv/vaults/vault");
    }

    #[test]
    fn a_relative_directory_is_accepted_as_scp_accepts_it() {
        // `host:backups` means "relative to the login directory" for scp, and
        // refusing it here would reject a form people already use.
        let destination = parsed("server:backups");

        assert_eq!(destination.directory, "backups");
    }

    #[test]
    fn a_windows_path_is_refused_rather_than_treated_as_a_server() {
        // `C:\vaults` splits into host "C" and directory "\vaults", and the
        // failure would arrive minutes later as a name resolution error with
        // nothing pointing back at the setting that caused it.
        let error = Destination::parse(r"C:\vaults").unwrap_err();

        let message = error.to_string();
        assert!(message.contains("Windows path"), "got: {message}");
    }

    #[test]
    fn a_destination_with_no_directory_says_what_it_needs() {
        for raw in ["server", "you@server", "server:", "server:/"] {
            let error = Destination::parse(raw).unwrap_err().to_string();
            assert!(
                error.contains("host:/path"),
                "{raw:?} should explain the form; got: {error}"
            );
        }
    }

    #[test]
    fn an_empty_destination_says_it_is_not_set() {
        let error = Destination::parse("   ").unwrap_err().to_string();

        assert!(error.contains("is not set"), "got: {error}");
        assert!(error.contains(DESTINATION), "got: {error}");
    }

    #[test]
    fn an_ordinary_name_is_usable() {
        for name in ["vault", "work-laptop", "vault_2", "Vault9"] {
            check_name(name).unwrap_or_else(|error| panic!("{name:?} must be usable: {error}"));
        }
    }

    #[test]
    fn a_name_that_would_have_to_be_quoted_is_refused() {
        // The remote side runs `mv` and `test` through a shell, and a name is
        // checked rather than escaped: it comes from `sefy push --name`, so the
        // person choosing it can pick another one — and a name needing quotes
        // is one the next tool to touch that directory will mishandle anyway.
        for name in [
            "vault; rm -rf /",
            "vault name",
            "va'ult",
            "va\"ult",
            "$(whoami)",
            "`id`",
            "vault|tee",
            "vault*",
            "",
        ] {
            assert!(
                check_name(name).is_err(),
                "{name:?} must be refused as a remote name"
            );
        }
    }

    #[test]
    fn a_name_that_would_escape_the_directory_is_refused() {
        for name in ["../vault", "sub/vault", "sub\\vault", ".hidden"] {
            assert!(
                check_name(name).is_err(),
                "{name:?} must not be usable as a remote name"
            );
        }
    }

    #[test]
    fn the_refusal_names_the_flag_that_chooses_another_one() {
        let error = check_name("bad name").unwrap_err().to_string();

        assert!(error.contains("--name"), "got: {error}");
    }

    #[test]
    fn sizes_read_the_way_a_person_reads_them() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(53_305), "52.1 KiB");
        assert_eq!(human(5_242_880), "5.0 MiB");
    }
}
