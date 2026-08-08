//! The vault: an encrypted file, its in-memory database, and the operations
//! that move data between them.

use crate::db;
use crate::error::{Error, Result};
use crate::format;
use crate::model::{Item, ItemSummary, NewItem, Payload, Query};
use rusqlite::Connection;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

/// An open vault.
///
/// The database lives in memory for as long as this value does; [`Vault::save`]
/// is what puts anything on disk, and it only ever writes ciphertext.
pub struct Vault {
    path: PathBuf,
    password: Zeroizing<Vec<u8>>,
    connection: Connection,
}

impl Vault {
    /// Creates a new vault file, failing if the path is already taken.
    pub fn create(path: impl AsRef<Path>, password: &[u8]) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            return Err(Error::AlreadyExists(path));
        }

        let vault = Self {
            path,
            password: Zeroizing::new(password.to_vec()),
            connection: db::create()?,
        };
        vault.save()?;
        Ok(vault)
    }

    /// Opens an existing vault file.
    pub fn open(path: impl AsRef<Path>, password: &[u8]) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = fs::read(&path)
            .map_err(|source| Error::io(format!("cannot read {}", path.display()), source))?;
        let database = format::decode(password, &file)?;

        Ok(Self {
            path,
            password: Zeroizing::new(password.to_vec()),
            connection: db::load(&database)?,
        })
    }

    /// Path of the file backing this vault.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Encrypts the current state and replaces the vault file atomically.
    ///
    /// The ciphertext goes to a temporary file in the same directory, is
    /// flushed and synced, and only then renamed over the target. A crash
    /// leaves either the old vault or the new one — never a half-written file,
    /// and never plaintext.
    pub fn save(&self) -> Result<()> {
        let database = Zeroizing::new(db::dump(&self.connection)?);
        let sealed = format::encode(&self.password, &database)?;
        write_atomically(&self.path, &sealed)
    }

    /// Adds an item and returns its identifier.
    pub fn add(&mut self, item: NewItem) -> Result<i64> {
        db::insert_item(&mut self.connection, item, now())
    }

    /// Reads an item with its payload.
    pub fn get(&self, id: i64) -> Result<Item> {
        db::get_item(&self.connection, id)
    }

    /// Reads an item without its payload.
    pub fn summary(&self, id: i64) -> Result<ItemSummary> {
        db::get_summary(&self.connection, id)
    }

    /// Changes an item's title, payload or tags; `None` leaves a field alone.
    ///
    /// A payload of a different kind than the item was created with is
    /// rejected: an item's kind is fixed for its lifetime.
    pub fn update(
        &mut self,
        id: i64,
        title: Option<String>,
        payload: Option<Payload>,
        tags: Option<Vec<String>>,
    ) -> Result<()> {
        db::update_item(&mut self.connection, id, title, payload, tags, now())
    }

    /// Removes an item.
    pub fn remove(&mut self, id: i64) -> Result<()> {
        db::delete_item(&self.connection, id)
    }

    /// Finds items matching a query, newest first.
    pub fn search(&self, query: &Query) -> Result<Vec<ItemSummary>> {
        db::search(&self.connection, query)
    }

    /// Lists every item, newest first.
    pub fn list(&self) -> Result<Vec<ItemSummary>> {
        db::search(&self.connection, &Query::all())
    }

    /// Lists every tag with the number of items carrying it.
    pub fn tags(&self) -> Result<Vec<(String, i64)>> {
        db::list_tags(&self.connection)
    }

    /// Replaces the master password and rewrites the file under it.
    ///
    /// The salt and nonce are fresh, so the new file shares nothing with the
    /// old one beyond its contents.
    pub fn change_password(&mut self, password: &[u8]) -> Result<()> {
        self.password = Zeroizing::new(password.to_vec());
        self.save()
    }
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .field("password", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// Seconds since the Unix epoch, or zero on a clock set before it.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Writes `bytes` to `path` so that the file is either fully replaced or
/// untouched.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(directory) = directory {
        fs::create_dir_all(directory).map_err(|source| {
            Error::io(format!("cannot create {}", directory.display()), source)
        })?;
    }

    let temporary = temporary_path(path);

    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()
    };
    if let Err(source) = write() {
        let _ = fs::remove_file(&temporary);
        return Err(Error::io(
            format!("cannot write {}", temporary.display()),
            source,
        ));
    }

    // `fs::rename` replaces an existing destination on both Unix and Windows,
    // so the swap is a single step: readers see either vault, never a partial
    // one.
    if let Err(source) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(Error::io(
            format!("cannot replace {}", path.display()),
            source,
        ));
    }
    Ok(())
}

/// Sibling path holding the ciphertext until the rename makes it the vault.
///
/// The name is derived from the target rather than randomized: a leftover
/// temporary from a crashed write is then reused instead of accumulating, and
/// anything already sitting at that path — including a directory — surfaces as
/// a write error rather than being quietly worked around.
fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("vault"))
        .to_os_string();
    name.push(".sefy-tmp");
    path.with_file_name(name)
}
