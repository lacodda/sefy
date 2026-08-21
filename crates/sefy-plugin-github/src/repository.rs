//! Moving the sealed vault in and out of a git repository.
//!
//! The working copy lives in this program's own data directory, cloned on
//! first use and refreshed on every call. The vault is stored under the name
//! sefy chose, with no extension and no directory of its own — a repository
//! full of files named `vault` and `work-laptop` says nothing about what they
//! hold, which matches the file format's own reticence.

use crate::git;
use std::path::{Path, PathBuf};

/// Environment variable naming the repository to use.
pub const REPOSITORY: &str = "SEFY_GITHUB_REPO";

/// What went wrong.
#[derive(Debug)]
pub enum Error {
    /// The request could not be understood.
    BadRequest(String),
    /// `SEFY_GITHUB_REPO` is not set.
    NoRepository,
    /// There is nowhere to keep the working copy.
    NoDataDirectory,
    /// The remote holds no copy under that name.
    NothingThere(String),
    /// A file could not be read or written.
    Io {
        /// What was being attempted.
        what: String,
        /// The underlying failure.
        source: std::io::Error,
    },
    /// git refused.
    Git(git::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(reason) => {
                write!(formatter, "the request could not be read: {reason}")
            }
            Self::NoRepository => write!(
                formatter,
                "{REPOSITORY} is not set\n\
                 point it at the repository to keep the vault in, for example:\n\
                 \x20 {REPOSITORY}=git@github.com:you/vault.git"
            ),
            Self::NoDataDirectory => formatter.write_str(
                "this system offers no per-user data directory to keep the working copy in",
            ),
            Self::NothingThere(name) => write!(
                formatter,
                "the repository holds no copy called {name:?} yet\n\
                 push from a machine that has the vault first"
            ),
            Self::Io { what, source } => write!(formatter, "{what}: {source}"),
            Self::Git(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for Error {}

impl From<git::Error> for Error {
    fn from(error: git::Error) -> Self {
        Self::Git(error)
    }
}

/// What sefy asks for.
#[derive(Debug, serde::Deserialize)]
struct Request {
    operation: String,
    file: PathBuf,
    name: String,
}

/// Carries out one request and returns the line to show the user.
pub fn handle(raw: &str) -> Result<String, Error> {
    let request: Request =
        serde_json::from_str(raw.trim()).map_err(|error| Error::BadRequest(error.to_string()))?;

    let repository = std::env::var(REPOSITORY)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(Error::NoRepository)?;

    let working_copy = working_copy_path()?;
    let checkout = prepare(&working_copy, &repository)?;

    match request.operation.as_str() {
        "push" => push(&checkout, &request),
        "pull" => pull(&checkout, &request),
        other => Err(Error::BadRequest(format!("unknown operation {other:?}"))),
    }
}

/// Clones the repository if needed, and brings it up to date.
fn prepare(working_copy: &Path, repository: &str) -> Result<PathBuf, Error> {
    if working_copy.join(".git").is_dir() {
        // A local commit that never reached the remote would make this fail
        // rather than silently diverge, which is what should happen: this copy
        // is a staging area, not a place to hold work.
        git::run(
            Some(working_copy),
            &["fetch", "--quiet", "origin"],
            "fetching from the remote",
        )?;
        git::run(
            Some(working_copy),
            &["reset", "--quiet", "--hard", "origin/HEAD"],
            "taking the remote's state",
        )?;
        return Ok(working_copy.to_path_buf());
    }

    if let Some(parent) = working_copy.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            what: format!("cannot create {}", parent.display()),
            source,
        })?;
    }

    git::run(
        None,
        &[
            "clone",
            "--quiet",
            repository,
            &working_copy.display().to_string(),
        ],
        "cloning the repository",
    )?;

    Ok(working_copy.to_path_buf())
}

/// Copies the sealed vault in and publishes it.
///
/// Whether anything changed is decided by comparing the bytes, **not** by
/// asking git. A vault re-sealed after an edit is the same length every time —
/// the format is fixed overhead plus the database — and rewriting it within the
/// same second leaves size and mtime identical. git's index caches those, so
/// `git add` can conclude nothing changed and `git status` agrees, and the push
/// silently publishes the previous contents. Reading both files is cheap next
/// to the network call that follows.
fn push(checkout: &Path, request: &Request) -> Result<String, Error> {
    let destination = checkout.join(&request.name);

    let incoming = std::fs::read(&request.file).map_err(|source| Error::Io {
        what: "cannot read the vault".to_owned(),
        source,
    })?;
    let present = std::fs::read(&destination).ok();

    if present.as_deref() == Some(incoming.as_slice()) {
        return Ok(format!(
            "{:?} is already what the repository holds ({})",
            request.name,
            human(incoming.len() as u64)
        ));
    }

    std::fs::write(&destination, &incoming).map_err(|source| Error::Io {
        what: "cannot write the vault into the working copy".to_owned(),
        source,
    })?;

    // Same reason as above: the index may hold a cached stat entry saying this
    // path is unchanged. Refreshing it makes git look at the file again.
    git::run(
        Some(checkout),
        &["update-index", "--really-refresh"],
        "refreshing what git knows about the working copy",
    )
    // Best effort: an older git without this flag, or a repository state it
    // objects to, must not stop a push whose file is already correct.
    .ok();

    git::run(
        Some(checkout),
        &["add", "--", &request.name],
        "staging the vault",
    )?;

    // A fixed message on purpose: a commit subject naming what changed would
    // annotate an otherwise anonymous file in a history anyone with repository
    // access can read.
    git::run(
        Some(checkout),
        &["commit", "--quiet", "-m", "update"],
        "recording the change",
    )?;

    git::run(
        Some(checkout),
        &["push", "--quiet"],
        "pushing to the remote",
    )?;

    Ok(format!(
        "pushed {:?} ({})",
        request.name,
        human(incoming.len() as u64)
    ))
}

/// Writes the repository's copy out to where sefy asked for it.
fn pull(checkout: &Path, request: &Request) -> Result<String, Error> {
    let source = checkout.join(&request.name);
    if !source.is_file() {
        return Err(Error::NothingThere(request.name.clone()));
    }

    std::fs::copy(&source, &request.file).map_err(|error| Error::Io {
        what: "cannot write the fetched copy".to_owned(),
        source: error,
    })?;

    let size = std::fs::metadata(&source).map(|data| data.len()).ok();
    Ok(match size {
        Some(bytes) => format!("fetched {:?} ({})", request.name, human(bytes)),
        None => format!("fetched {:?}", request.name),
    })
}

/// Where the working copy lives.
fn working_copy_path() -> Result<PathBuf, Error> {
    if let Some(override_path) = std::env::var_os("SEFY_GITHUB_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return Ok(override_path);
    }

    data_directory()
        .map(|base| base.join("sefy-plugin-github").join("repository"))
        .ok_or(Error::NoDataDirectory)
}

/// Base directory for per-user application data.
///
/// Resolved from the environment for the same reason sefy itself does it: this
/// is the only path the program needs.
fn data_directory() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(|home| {
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
            })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let from_xdg = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());

        from_xdg.or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local").join("share"))
        })
    }
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
