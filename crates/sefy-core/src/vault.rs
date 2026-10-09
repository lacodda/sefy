//! The vault: an encrypted file, its in-memory database, and the operations
//! that move data between them.

use crate::copies;
use crate::db::{self, Stamp};
use crate::error::{Error, Result};
use crate::format;
use crate::history::Version;
use crate::model::{Field, Item, ItemSummary, NewItem, Payload, Query};
use rusqlite::Connection;
use std::cell::Cell;
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
    device: Option<String>,
    /// Whether this vault has a file of its own to be written to.
    ///
    /// A remote copy read from a transport's bytes, or the scratch copy a
    /// preview folds into, has none: it lives in memory, is looked at and is
    /// dropped. Neither ever leaves this crate in that state.
    backing: Backing,
    /// How many times this value has written its file.
    writes: Cell<u64>,
}

/// Where a vault's contents go when it is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backing {
    /// To the file at the vault's path.
    File,
    /// Nowhere: a copy that exists only to be read.
    Memory,
}

/// What a password change did beyond the vault itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PasswordChange {
    /// The [copies](crate::copies) beside the vault, now sealed under the new
    /// password as well.
    pub resealed: usize,
    /// Copies that did not open under the old password and were left as they
    /// were: whatever they are, they are not this vault's to re-seal.
    pub left: Vec<PathBuf>,
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
            device: None,
            backing: Backing::File,
            writes: Cell::new(0),
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
            device: None,
            backing: Backing::File,
            writes: Cell::new(0),
        })
    }

    /// Opens a vault from the sealed bytes of its file, without a file of its
    /// own.
    ///
    /// For the copy a transport fetched: it is read, folded from and dropped,
    /// and never written anywhere. The bytes are the same ciphertext the
    /// remote holds.
    pub(crate) fn from_sealed(sealed: &[u8], password: &[u8]) -> Result<Self> {
        let database = format::decode(password, sealed)?;
        Ok(Self {
            path: PathBuf::new(),
            password: Zeroizing::new(Vec::new()),
            connection: db::load(&database)?,
            device: None,
            backing: Backing::Memory,
            writes: Cell::new(0),
        })
    }

    /// A copy of this vault's database to change and throw away.
    ///
    /// What a preview folds into: the merge runs for real, on contents nobody
    /// will write. The copy keeps the machine's name, so the versions a
    /// preview makes are stamped as the real merge's would be.
    pub(crate) fn scratch_copy(&self) -> Result<Self> {
        let database = Zeroizing::new(db::dump(&self.connection)?);
        Ok(Self {
            path: self.path.clone(),
            password: Zeroizing::new(Vec::new()),
            connection: db::load(&database)?,
            device: self.device.clone(),
            backing: Backing::Memory,
            writes: Cell::new(0),
        })
    }

    /// Names the machine this vault is being changed on.
    ///
    /// Every version written from here on carries the name, so a history can
    /// say which machine a change came from — the question a merge conflict
    /// raises first. The library does not guess it: the program around it
    /// knows what the machine is called, and a test knows it is none.
    pub fn set_device(&mut self, device: Option<String>) {
        self.device = device.filter(|name| !name.trim().is_empty());
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
        // Only copies made inside this crate are memory-backed, and none of
        // them is ever saved; reaching this is a bug here, not a condition a
        // caller can meet.
        assert_eq!(
            self.backing,
            Backing::File,
            "a vault with no file of its own was asked to save"
        );
        let database = Zeroizing::new(db::dump(&self.connection)?);
        let sealed = format::encode(&self.password, &database)?;
        write_atomically(&self.path, &sealed)?;
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }

    /// How many times this value has written its file since it was opened.
    ///
    /// The question a program asks after a command: did that change the vault
    /// on disk? Counted at the write itself, so a command that changed nothing
    /// and saved nothing reads as exactly that.
    pub fn writes(&self) -> u64 {
        self.writes.get()
    }

    /// Adds an item and returns its identifier.
    pub fn add(&mut self, item: NewItem) -> Result<i64> {
        db::insert_item(&mut self.connection, item, now(), self.device.as_deref())
    }

    /// Adds an item that already has an identity elsewhere.
    ///
    /// For contents arriving from an export, where re-generating the identity
    /// would make the same item look like a new one. The export carries no
    /// history, so the contents start a line of their own, written at
    /// `updated_at`.
    pub fn add_existing(
        &mut self,
        item: NewItem,
        uuid: &str,
        created_at: i64,
        updated_at: i64,
    ) -> Result<i64> {
        let stamp = Stamp::new(1, updated_at, self.device.as_deref())?;
        db::insert_item_with_uuid(
            &mut self.connection,
            item,
            uuid,
            created_at,
            updated_at,
            &stamp,
        )
    }

    /// Adds an item from another vault with the version its contents are and
    /// every earlier one.
    pub(crate) fn add_travelled(
        &mut self,
        item: NewItem,
        uuid: &str,
        (created_at, updated_at): (i64, i64),
        stamp: &Stamp,
        past: &[Version],
    ) -> Result<(i64, usize)> {
        let id = db::insert_item_with_uuid(
            &mut self.connection,
            item,
            uuid,
            created_at,
            updated_at,
            stamp,
        )?;
        let mut kept = 0;
        for version in past {
            if db::keep_version(&self.connection, id, version)? {
                kept += 1;
            }
        }
        Ok((id, kept))
    }

    /// The stamp of the version an item's contents are.
    pub(crate) fn stamp(&self, id: i64) -> Result<Stamp> {
        db::stamp_of(&self.connection, id)
    }

    /// An item's earlier versions, oldest first, without the current one.
    pub(crate) fn past_versions(&self, id: i64) -> Result<Vec<Version>> {
        db::past_versions(&self.connection, id)
    }

    /// Keeps a version that came from elsewhere, unless it is already kept.
    pub(crate) fn keep_version(&mut self, id: i64, version: &Version) -> Result<bool> {
        db::keep_version(&self.connection, id, version)
    }

    /// Puts contents from elsewhere in place under the version they already
    /// are, keeping what they replace.
    pub(crate) fn take_contents(
        &mut self,
        id: i64,
        payload: &Payload,
        stamp: &Stamp,
        replaced_lost_a_conflict: bool,
    ) -> Result<()> {
        let transaction = self.connection.transaction()?;
        db::replace_contents(&transaction, id, payload, stamp, replaced_lost_a_conflict)?;
        transaction.commit()?;
        Ok(())
    }

    /// Replaces an item's title and tags, leaving its contents alone.
    pub(crate) fn set_labels(
        &mut self,
        id: i64,
        title: &str,
        tags: &[String],
        updated_at: i64,
    ) -> Result<()> {
        db::set_labels(&mut self.connection, id, title, tags, updated_at)
    }

    /// Sets when an item last changed.
    pub(crate) fn set_updated_at(&mut self, id: i64, updated_at: i64) -> Result<()> {
        db::set_updated_at(&self.connection, id, updated_at)
    }

    /// Finds the item carrying this identity, if there is one.
    pub fn find_by_uuid(&self, uuid: &str) -> Result<Option<i64>> {
        db::find_by_uuid(&self.connection, uuid)
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
    /// rejected: an item's kind is fixed for its lifetime. New contents become
    /// a new version and the ones they replace stay in [`Vault::history`].
    pub fn update(
        &mut self,
        id: i64,
        title: Option<String>,
        payload: Option<Payload>,
        tags: Option<Vec<String>>,
    ) -> Result<()> {
        self.update_at(id, title, payload, tags, now())
    }

    /// [`Vault::update`] at a moment given rather than read off the clock.
    ///
    /// For tests of what a merge makes of two edits in the same second: whole
    /// seconds are what timestamps hold, and a test cannot wait its way into a
    /// tie.
    pub(crate) fn update_at(
        &mut self,
        id: i64,
        title: Option<String>,
        payload: Option<Payload>,
        tags: Option<Vec<String>>,
        at: i64,
    ) -> Result<()> {
        db::update_item(
            &mut self.connection,
            id,
            title,
            payload,
            tags,
            at,
            self.device.as_deref(),
        )
    }

    /// The database underneath, for a test that has to write what an older
    /// build would.
    #[cfg(test)]
    pub(crate) fn connection_for_tests(&self) -> &Connection {
        &self.connection
    }

    /// Every version of an item's contents, oldest first, the current one last.
    ///
    /// A title or a tag is not part of a version: renaming an item makes none,
    /// and restoring one does not bring an old name back.
    pub fn history(&self, id: i64) -> Result<Vec<Version>> {
        db::history(&self.connection, id)
    }

    /// How many earlier versions of an item's contents are kept.
    pub fn earlier_versions(&self, id: i64) -> Result<usize> {
        db::count_past_versions(&self.connection, id)
    }

    /// Brings back the contents of an earlier version, or one field of them.
    ///
    /// Restoring is an edit like any other: the contents it replaces become a
    /// version of their own, so a restore can itself be undone. With `field`,
    /// only that field comes back — in place if the record still has it, at
    /// the end if not — and every other field keeps its current value.
    ///
    /// Returns whether anything changed; contents that already match the
    /// version are left alone rather than recorded again.
    pub fn restore(&mut self, id: i64, version: &str, field: Option<&str>) -> Result<bool> {
        let versions = self.history(id)?;
        let wanted = versions
            .iter()
            .find(|candidate| candidate.uuid == version)
            .ok_or_else(|| Error::VersionNotFound {
                id,
                version: version.to_owned(),
            })?;
        let current = &versions
            .last()
            .expect("history ends with the current version")
            .payload;

        let restored = match field {
            None => wanted.payload.clone(),
            Some(name) => with_field_from(current, &wanted.payload, name)?,
        };
        if &restored == current {
            return Ok(false);
        }

        self.update(id, None, Some(restored), None)?;
        Ok(true)
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

    /// Turns what a user typed into exactly one item.
    ///
    /// A reference is either an id or text. Text prefers an exact,
    /// case-insensitive title match, and falls back to a substring search
    /// across titles and item contents. Anything that resolves to more than one
    /// item comes back as [`Error::Ambiguous`] carrying the candidates, so the
    /// caller can show them rather than guess.
    pub fn resolve(&self, reference: &str) -> Result<ItemSummary> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(Error::NotFound(String::new()));
        }

        // A bare number is an id. Titles that look like numbers stay reachable
        // through the text path below when no such id exists.
        if let Ok(id) = reference.parse::<i64>() {
            match self.summary(id) {
                Ok(summary) => return Ok(summary),
                Err(Error::ItemNotFound(_)) => {}
                Err(other) => return Err(other),
            }
        }

        let matches = self.search(&Query::all().text(reference))?;
        let exact: Vec<ItemSummary> = matches
            .iter()
            .filter(|summary| summary.title.eq_ignore_ascii_case(reference))
            .cloned()
            .collect();
        let candidates = if exact.is_empty() { matches } else { exact };

        match candidates.len() {
            0 => Err(Error::NotFound(reference.to_owned())),
            1 => Ok(candidates.into_iter().next().expect("length checked")),
            _ => Err(Error::Ambiguous {
                reference: reference.to_owned(),
                candidates,
            }),
        }
    }

    /// Replaces the master password and rewrites the file under it.
    ///
    /// The salt and nonce are fresh, so the new file shares nothing with the
    /// old one beyond its contents.
    ///
    /// The [copies](crate::copies) kept beside the vault are re-sealed under
    /// the new password too. Left as they were, they would be three more files
    /// the retired password still opens — and retiring it is usually the
    /// whole point of changing it. The vault goes first: it is the one that
    /// matters if anything stops halfway.
    pub fn change_password(&mut self, password: &[u8]) -> Result<PasswordChange> {
        let old = std::mem::replace(&mut self.password, Zeroizing::new(password.to_vec()));
        self.save()?;
        copies::reseal(&self.path, &old, password)
    }

    /// What this vault holds, without revealing any of it.
    ///
    /// Every number here is about shape rather than contents: how many items,
    /// of which kinds, how many tags. It is what `sefy status` reports, and it
    /// is deliberately the whole of it — a status that printed a title would
    /// put a secret's name on a screen that was asked only whether the vault is
    /// there.
    pub fn stats(&self) -> Result<Stats> {
        Ok(Stats {
            items: db::count_items(&self.connection)?,
            by_kind: db::count_by_kind(&self.connection)?,
            tags: self.tags()?.len(),
            schema: db::schema_version(&self.connection)?,
            last_sync: self.last_sync()?,
        })
    }

    /// When this vault last reached a remote, and through which transport.
    ///
    /// `None` means it never has — or that it last did so under a build that
    /// did not record it, which reads the same way and is the honest answer.
    pub fn last_sync(&self) -> Result<Option<SyncStamp>> {
        let Some(raw) = db::meta_get(&self.connection, LAST_SYNC)? else {
            return Ok(None);
        };
        Ok(SyncStamp::parse(&raw))
    }

    /// Records that this vault has just reached a remote.
    ///
    /// The caller saves: a stamp is worth exactly as much as the file it was
    /// written into, and a push that stamped a vault it then failed to write
    /// would claim a sync that left no trace.
    pub fn record_sync(&mut self, transport: &str, operation: &str) -> Result<()> {
        let stamp = SyncStamp {
            at: now(),
            transport: transport.to_owned(),
            operation: operation.to_owned(),
        };
        db::meta_set(&self.connection, LAST_SYNC, &stamp.encode())
    }
}

/// The current contents of a record with one field taken from an earlier
/// version.
fn with_field_from(current: &Payload, earlier: &Payload, name: &str) -> Result<Payload> {
    let (Payload::Fields { kind, fields }, Payload::Fields { fields: then, .. }) =
        (current, earlier)
    else {
        // A note or a file is one value, not a set of fields: its version is
        // restored whole or not at all.
        return Err(Error::FieldNotInVersion {
            name: name.to_owned(),
            available: Vec::new(),
        });
    };

    let taken: &Field = then
        .iter()
        .find(|field| field.name == name)
        .ok_or_else(|| Error::FieldNotInVersion {
            name: name.to_owned(),
            available: then.iter().map(|field| field.name.clone()).collect(),
        })?;

    let mut fields = fields.clone();
    match fields.iter_mut().find(|field| field.name == name) {
        Some(field) => *field = taken.clone(),
        None => fields.push(taken.clone()),
    }
    Ok(Payload::Fields {
        kind: kind.clone(),
        fields,
    })
}

/// `meta` key under which the last sync is recorded.
const LAST_SYNC: &str = "last_sync";

/// What a vault holds, counted rather than read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stats {
    /// How many items in total.
    pub items: usize,
    /// How many of each kind, in the order the kinds are declared, skipping
    /// kinds this vault has none of.
    pub by_kind: Vec<(String, usize)>,
    /// How many distinct tags are in use.
    pub tags: usize,
    /// Schema version of the database inside the blob.
    pub schema: i64,
    /// When this vault last reached a remote, if it ever has.
    pub last_sync: Option<SyncStamp>,
}

/// A note that this vault reached a remote, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStamp {
    /// Unix seconds.
    pub at: i64,
    /// Short name of the transport it went through.
    pub transport: String,
    /// Which operation wrote it: `push`, `pull` or `sync`.
    pub operation: String,
}

impl SyncStamp {
    /// Renders the stamp for storage: seconds, transport, operation.
    ///
    /// A flat string rather than JSON, because it is three fields that will
    /// stay three fields, and a parser that cannot fail is worth more here
    /// than one that can express more.
    fn encode(&self) -> String {
        format!("{} {} {}", self.at, self.transport, self.operation)
    }

    /// Reads a stamp back, or nothing if the value is not one.
    ///
    /// Unreadable is treated as absent rather than as an error: this is a
    /// convenience recorded by some build, possibly a future one, and a vault
    /// that will not open because it carries a note about itself would be a
    /// poor trade.
    fn parse(raw: &str) -> Option<Self> {
        let mut parts = raw.split(' ');
        let at = parts.next()?.parse().ok()?;
        let transport = parts.next()?.to_owned();
        let operation = parts.next().unwrap_or("sync").to_owned();
        Some(Self {
            at,
            transport,
            operation,
        })
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
pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Writes `bytes` to `path` so that the file is either fully replaced or
/// untouched.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
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
