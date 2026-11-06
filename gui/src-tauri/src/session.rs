//! The one vault the window has open, and how it gets there.
//!
//! The vault lives here, in this process, for as long as it is unlocked; the
//! page is told what it is (`VaultStatus`), never handed the database. Locking
//! drops it, which wipes the password the core holds and lets the in-memory
//! database go.
//!
//! While it is unlocked the window is also an agent: the [`Keeper`] here holds
//! the password for the command line to ask keys of, and runs the clock that
//! locks the vault when nobody has used it for a while.

use crate::wire::{
    CardBody, Failure, FailureKind, Filter, ItemCard, ItemRow, Opening, TagCount, Transfer,
    VaultStatus, VersionRow,
};
use sefy_core::agent::Keeper;
use sefy_core::crypto::{DerivedKey, SALT_LEN};
use sefy_core::vault::version_on_disk;
use sefy_core::{
    Error, FreshKey, ItemKind, KeySource, Payload, Plugin, Policy, Query, Totp, Vault, sync,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use zeroize::Zeroizing;

/// The event the page is sent when the vault locked: by itself, or because
/// something outside the window said so.
pub const LOCKED: &str = "vault-locked";

/// The event the page is sent when a second launch handed over a file.
pub const HANDED_OVER: &str = "file-handed-over";

/// The event the page is sent when what it shows may have moved on: something
/// else wrote the file and the window read it again, or the window wrote it.
pub const CHANGED: &str = "vault-changed";

/// The event the page is sent when the file changed and the window cannot read
/// the new version.
pub const UNREADABLE: &str = "vault-unreadable";

/// How often the window looks at the file. It reads no more than the salt at the
/// front, so it can afford to, and a person switching back from a terminal should
/// not find the window behind.
pub const LOOK_EVERY: Duration = Duration::from_secs(2);

/// What looking at the file came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    /// Nothing the page has to hear about.
    Nothing,
    /// The window now holds a newer version of the file, or can read it again.
    Moved,
    /// The file changed and does not open: what is on screen is the last version
    /// read, and writing is refused until the file can be read.
    Unreadable,
}

/// Asks the keeper for keys as someone looking rather than someone using the
/// vault: a file that changes in an empty room must not keep the vault unlocked.
/// A write is someone using it, and counts.
struct Looking(Arc<Keeper>);

impl KeySource for Looking {
    fn key_for(&self, salt: &[u8; SALT_LEN]) -> sefy_core::Result<DerivedKey> {
        self.0.peek(salt)
    }

    fn fresh(&self, opened_with: &[u8; SALT_LEN]) -> sefy_core::Result<FreshKey> {
        self.0.fresh(opened_with)
    }
}

/// The vault the window has open, if any.
pub struct Session {
    vault: Mutex<Option<Vault>>,
    keeper: Arc<Keeper>,
    /// Goes up whenever what is open changes hands, so that something that took
    /// its time - a transfer, a second look at the file - can tell it is still
    /// about the vault it started on.
    opened: AtomicU64,
    /// The version of the file that would not open, if one would not: it is not
    /// tried again, at the cost of a stretched password every few seconds, until
    /// the file is something else.
    unreadable: Mutex<Option<Option<[u8; SALT_LEN]>>>,
    /// What the last transfer did, until the vault locks.
    last_transfer: Mutex<Option<Transfer>>,
    /// Whether a transfer is under way: there is one remote and one file.
    transferring: AtomicBool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            vault: Mutex::new(None),
            keeper: Keeper::new(),
            opened: AtomicU64::new(0),
            unreadable: Mutex::new(None),
            last_transfer: Mutex::new(None),
            transferring: AtomicBool::new(false),
        }
    }
}

/// Holds [`Session::transferring`] for as long as a transfer lasts.
struct OneTransfer<'a>(&'a AtomicBool);

impl Drop for OneTransfer<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Session {
    /// Puts an opened vault in the window, in place of whatever was open.
    ///
    /// The keeper gets its own copy of the password. The vault keeps one to
    /// seal with, and neither outlasts [`Session::lock`].
    pub fn hold(
        &self,
        mut vault: Vault,
        password: Zeroizing<Vec<u8>>,
    ) -> Result<VaultStatus, Failure> {
        let status = VaultStatus::of(&vault.status()?);
        let idle = Duration::from_secs(vault.policy()?.lock_after_seconds);
        // Versions written from here say where, as the command line's do.
        vault.set_device(sefy_core::device::name());
        *self.slot() = Some(vault);
        self.changed_hands();
        self.keeper.give(password, idle);
        Ok(status)
    }

    /// What is open is no longer what it was.
    fn changed_hands(&self) {
        self.opened.fetch_add(1, Ordering::SeqCst);
        *self
            .unreadable
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
        *self
            .last_transfer
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Closes the open vault, if there is one.
    pub fn lock(&self) {
        // Taken out and dropped here rather than left to the next unlock, so
        // the password and the database are gone the moment the window says
        // the vault is locked.
        drop(self.slot().take());
        self.changed_hands();
        self.keeper.wipe();
    }

    /// What holds the password for the command line and runs the clock.
    pub fn keeper(&self) -> Arc<Keeper> {
        self.keeper.clone()
    }

    /// Someone is using the vault here: the idle time starts over.
    pub fn touch(&self) {
        self.keeper.touch();
    }

    /// Whether a vault other than the one at `path` is what is open.
    pub fn holds_other_than(&self, path: &Path) -> bool {
        let slot = self.slot();
        let Some(vault) = slot.as_ref() else {
            return false;
        };
        // Compared as the system sees them: a launch passes the path as it was
        // typed, and the case or the separators may differ from what opened
        // this. A file that cannot be resolved is not this one.
        match (vault.path().canonicalize(), path.canonicalize()) {
            (Ok(open), Ok(other)) => open != other,
            _ => true,
        }
    }

    /// What and where the open vault is.
    pub fn status(&self) -> Result<VaultStatus, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        Ok(VaultStatus::of(&vault.status()?))
    }

    /// Whether a vault is open.
    pub fn is_open(&self) -> bool {
        self.slot().is_some()
    }

    /// The items `filter` lets through, best match first.
    ///
    /// The core finds them (`Vault::search`, the same search as `sefy find`)
    /// and lists them newest first. With text typed, the window then puts the
    /// items whose *title* matches ahead of those that match only inside -
    /// see [`rank`] - keeping newest first within each.
    pub fn items(&self, filter: &Filter) -> Result<Vec<ItemRow>, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        let mut query = Query::all();
        let text = filter
            .text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        if let Some(text) = text {
            query = query.text(text);
        }
        if let Some(kind) = &filter.kind {
            query = query.kind(ItemKind::parse(kind));
        }
        if let Some(tag) = &filter.tag {
            query = query.tags([tag.as_str()]);
        }
        let mut found = vault.search(&query)?;
        if let Some(text) = text {
            found.sort_by_key(|item| rank(&item.title, text));
        }
        Ok(found.iter().map(ItemRow::of).collect())
    }

    /// Every tag in use, with how many items carry it.
    pub fn tags(&self) -> Result<Vec<TagCount>, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        Ok(vault
            .tags()?
            .into_iter()
            .map(|(tag, count)| TagCount {
                tag,
                count: usize::try_from(count).unwrap_or(0),
            })
            .collect())
    }

    /// The item with `id`, as the card shows it, as of `now` (Unix seconds).
    pub fn card(&self, id: i64, now: i64) -> Result<ItemCard, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        let item = vault.get(id)?;
        Ok(ItemCard::of(&item, vault.earlier_versions(id)?, now))
    }

    /// One field of the item with `id`, and what it is called, as of `now`
    /// (Unix seconds): what Copy puts on the clipboard and Reveal shows.
    ///
    /// `None` takes what the item is mostly about, as `sefy get` does: a
    /// note's text, the first secret field of a record. A one-time password
    /// key gives the code it makes now, never the key - the key is set up
    /// once and the code is what is wanted every time after, which is also
    /// what `sefy fill` hands over.
    pub fn value(
        &self,
        id: i64,
        field: Option<&str>,
        now: u64,
    ) -> Result<(Zeroizing<String>, String), Failure> {
        self.value_of(id, None, field, now)
    }

    /// [`Session::value`], from an earlier version of the item when `version`
    /// names one: what the history's Show and Copy take.
    pub fn value_of(
        &self,
        id: i64,
        version: Option<&str>,
        field: Option<&str>,
        now: u64,
    ) -> Result<(Zeroizing<String>, String), Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        let item = vault.get(id)?;
        let title = &item.summary.title;
        let earlier;
        let payload = match version {
            None => &item.payload,
            Some(wanted) => {
                earlier = vault
                    .history(id)?
                    .into_iter()
                    .find(|version| version.uuid == wanted)
                    .ok_or_else(|| refused(format!("{title:?} has no such version")))?;
                &earlier.payload
            }
        };
        match payload {
            Payload::Note { text } => match field {
                None => Ok((Zeroizing::new(text.clone()), "text".to_owned())),
                Some(_) => Err(refused(format!("{title:?} is a note; it has no fields"))),
            },
            Payload::Fields { kind, fields } => {
                let name = match field {
                    Some(name) => name,
                    None => kind
                        .template()
                        .and_then(|template| template.default_field())
                        .map(|field| field.name)
                        .ok_or_else(|| {
                            refused(format!("{title:?} has no field that is the secret"))
                        })?,
                };
                let stored = fields
                    .iter()
                    .find(|field| field.name == name)
                    .ok_or_else(|| refused(format!("{title:?} has no {name:?}")))?;
                if stored.name == sefy_core::otp::FIELD {
                    let totp = Totp::parse(&stored.value).map_err(|_| {
                        refused(format!(
                            "the one-time password key of {title:?} cannot make a code"
                        ))
                    })?;
                    return Ok((Zeroizing::new(totp.code_at(now)), "code".to_owned()));
                }
                Ok((Zeroizing::new(stored.value.clone()), stored.name.clone()))
            }
            Payload::File { .. } => Err(refused(format!(
                "{title:?} is a file; files leave the vault with sefy extract"
            ))),
            Payload::Unknown { kind } => Err(refused(format!(
                "{title:?} is a {kind}, which a newer sefy wrote; update sefy to open it"
            ))),
        }
    }

    /// Every version of the item with `id`, oldest first, the current one last,
    /// as of `now` (Unix seconds).
    pub fn history(&self, id: i64, now: i64) -> Result<Vec<VersionRow>, Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        let versions = vault.history(id)?;
        Ok(versions
            .iter()
            .enumerate()
            .map(|(index, version)| VersionRow {
                number: index + 1,
                uuid: version.uuid.clone(),
                made_at: version.made_at,
                device: version.device.clone(),
                conflict: version.conflict,
                current: version.current,
                changed: index
                    .checked_sub(1)
                    .map(|before| {
                        sefy_core::history::changed(&versions[before].payload, &version.payload)
                    })
                    .unwrap_or_default(),
                body: CardBody::of(&version.payload, now),
            })
            .collect())
    }

    /// Changes the open vault, on top of the file as it is now.
    ///
    /// Whatever the command line or a sync client wrote since the window last
    /// looked is read first, so `change` never works from a version the file has
    /// moved on from. A file that will not open stops it there; `save` holds the
    /// line for the moment between this and the write.
    pub fn write<T>(
        &self,
        change: impl FnOnce(&mut Vault) -> Result<T, Failure>,
    ) -> Result<T, Failure> {
        let mut slot = self.slot();
        let vault = slot.as_mut().ok_or_else(Failure::locked)?;
        vault.reload().map_err(cannot_catch_up)?;
        change(vault)
    }

    /// [`Session::write`], if the vault open now is still the one `opened` was
    /// read from: a transfer that took a minute must not fold into another vault
    /// unlocked meanwhile.
    fn write_to<T>(
        &self,
        opened: u64,
        change: impl FnOnce(&mut Vault) -> Result<T, Failure>,
    ) -> Result<T, Failure> {
        let mut slot = self.slot();
        if self.opened.load(Ordering::SeqCst) != opened {
            return Err(Failure::locked());
        }
        let vault = slot.as_mut().ok_or_else(Failure::locked)?;
        vault.reload().map_err(cannot_catch_up)?;
        change(vault)
    }

    /// Looks at the file, and reads it again if it is no longer what is held.
    ///
    /// The stretching, when there is any, happens with the vault left free for
    /// the page: it is one word from a keeper that already knows the key when
    /// the command line wrote through it, and most of a second when it did not.
    pub fn look_again(&self) -> Seen {
        let (path, opened) = {
            let slot = self.slot();
            let Some(vault) = slot.as_ref() else {
                return Seen::Nothing;
            };
            if vault.is_current() {
                // A file that would not open has gone back to being this one.
                let was_unreadable = self
                    .unreadable
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take()
                    .is_some();
                return if was_unreadable {
                    Seen::Moved
                } else {
                    Seen::Nothing
                };
            }
            (
                vault.path().to_path_buf(),
                self.opened.load(Ordering::SeqCst),
            )
        };

        let version = version_on_disk(&path);
        if *self
            .unreadable
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            == Some(version)
        {
            return Seen::Nothing;
        }

        match Vault::open_through(&path, Arc::new(Looking(self.keeper.clone()))) {
            Ok(mut read) => {
                read.set_device(sefy_core::device::name());
                let mut slot = self.slot();
                if self.opened.load(Ordering::SeqCst) != opened {
                    return Seen::Nothing;
                }
                let Some(held) = slot.as_mut() else {
                    return Seen::Nothing;
                };
                // The window's own write got in meanwhile, and is newer.
                if held.is_current() {
                    return Seen::Nothing;
                }
                if let Ok(policy) = read.policy() {
                    self.keeper
                        .set_idle(Duration::from_secs(policy.lock_after_seconds));
                }
                *held = read;
                *self
                    .unreadable
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = None;
                Seen::Moved
            }
            // The password went while this was looking; the lock says the rest.
            Err(Error::KeysGone) => Seen::Nothing,
            Err(_) => {
                *self
                    .unreadable
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(version);
                Seen::Unreadable
            }
        }
    }

    /// Brings back an earlier version of an item, whole, and says whether that
    /// changed anything. What it replaces becomes a version of its own, so this
    /// can be undone the same way.
    pub fn restore(&self, id: i64, version: &str) -> Result<bool, Failure> {
        self.write(|vault| {
            let changed = vault.restore(id, version, None)?;
            if changed {
                vault.save()?;
            }
            Ok(changed)
        })
    }

    /// How long what the open vault lets out stays out; what a new vault says
    /// when none is open.
    pub fn policy(&self) -> Result<Policy, Failure> {
        match self.slot().as_ref() {
            Some(vault) => Ok(vault.policy()?),
            None => Ok(Policy::default()),
        }
    }

    /// Sets how long what the open vault lets out stays out. The time left starts
    /// from the new number at once.
    pub fn set_policy(&self, policy: Policy) -> Result<(), Failure> {
        self.write(|vault| {
            vault.set_policy(policy)?;
            vault.save()?;
            Ok(())
        })?;
        self.keeper
            .set_idle(Duration::from_secs(policy.lock_after_seconds));
        Ok(())
    }

    /// The path of the open vault, and which opening of it this is.
    fn open_file(&self) -> Result<(PathBuf, u64), Failure> {
        let slot = self.slot();
        let vault = slot.as_ref().ok_or_else(Failure::locked)?;
        Ok((
            vault.path().to_path_buf(),
            self.opened.load(Ordering::SeqCst),
        ))
    }

    /// Changes the master password. Asks for the current one rather than taking
    /// the vault being unlocked for an answer, as `sefy change-password` does:
    /// retiring a password takes knowing it.
    pub fn change_password(
        &self,
        current: &[u8],
        new: &[u8],
    ) -> Result<sefy_core::PasswordChange, Failure> {
        if new.is_empty() {
            return Err(refused("a master password cannot be empty".to_owned()));
        }
        let (path, opened) = self.open_file()?;
        let mut vault = Vault::open(&path, current)?;
        vault.set_device(sefy_core::device::name());
        let change = vault.change_password(new)?;
        let idle = Duration::from_secs(vault.policy()?.lock_after_seconds);

        let mut slot = self.slot();
        // Locked while the password was stretched: the file has the new one, and
        // the window goes on being locked.
        if self.opened.load(Ordering::SeqCst) == opened && slot.is_some() {
            *slot = Some(vault);
            self.keeper.give(Zeroizing::new(new.to_vec()), idle);
        }
        Ok(change)
    }

    /// The vault at the open path, opened by the password typed for it, if the
    /// name typed is its own: what a wipe needs before anything is touched.
    ///
    /// The name exactly, every character: a near miss means the person was not
    /// reading what they were destroying. And the password, typed: a vault found
    /// unlocked is not a vault someone has the right to destroy.
    pub fn open_to_wipe(&self, password: &[u8], typed: &str) -> Result<Vault, Failure> {
        let (path, _) = self.open_file()?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if typed.trim() != name {
            return Err(refused(format!("that is not {name:?}; nothing was wiped")));
        }
        Ok(Vault::open(&path, password)?)
    }

    /// One transfer at a time, or a word about the one under way.
    fn one_transfer(&self) -> Result<OneTransfer<'_>, Failure> {
        if self.transferring.swap(true, Ordering::SeqCst) {
            return Err(refused("a transfer is already under way".to_owned()));
        }
        Ok(OneTransfer(&self.transferring))
    }

    /// Pull, then push, through `plugin`, as `sefy sync` does and in the same
    /// order, for the reason [`sefy_core::sync::sync`] gives.
    ///
    /// The vault is held only for the merge: the two ends of the transfer take as
    /// long as the network does, and the window stays usable through them. With
    /// `remote_password`, the copy at the remote is opened with that rather than
    /// with the vault's own.
    ///
    /// A result that cannot be sent back up is not a failed sync: what came down
    /// is in the vault, and the transfer says what is left to do.
    pub fn sync_through(
        &self,
        plugin: &Plugin,
        remote_password: Option<&[u8]>,
        now: i64,
    ) -> Result<Transfer, Failure> {
        let _one = self.one_transfer()?;
        let (path, opened) = self.open_file()?;
        let name = sync::remote_name();

        let fetched = sync::fetch(plugin, &name)?;
        let remote = match remote_password {
            Some(password) => fetched.open(password),
            None => fetched.open(&*self.keeper as &dyn KeySource),
        }
        .map_err(|error| match error {
            Error::WrongPasswordOrNotAVault | Error::TooSmall => Failure {
                kind: FailureKind::RemoteRefused,
                message: format!("that password does not open the copy at {}", plugin.name()),
            },
            other => other.into(),
        })?;

        let merged = self.write_to(opened, |vault| {
            let merged = sefy_core::merge(vault, &remote)?;
            sync::stamp(vault, plugin, "sync")?;
            Ok(merged)
        })?;

        let mut transfer = Transfer::new(now, plugin.name(), "sync").with_merge(&merged);
        transfer.said.extend(fetched.transport.message);
        match sync::send(&path, plugin, &name) {
            Ok(sent) => transfer.said.extend(sent.message),
            Err(error) => transfer.send_failure = Some(error.to_string()),
        }
        self.remember(transfer.clone());
        Ok(transfer)
    }

    /// Sends this vault up through `plugin`, replacing what is there: how a first
    /// copy gets to a remote that has none, as `sefy push`.
    pub fn push_through(&self, plugin: &Plugin, now: i64) -> Result<Transfer, Failure> {
        let _one = self.one_transfer()?;
        let (path, opened) = self.open_file()?;
        let name = sync::remote_name();

        self.write_to(opened, |vault| {
            sync::stamp(vault, plugin, "push")?;
            Ok(())
        })?;

        let sent = sync::send(&path, plugin, &name)?;
        let mut transfer = Transfer::new(now, plugin.name(), "push");
        transfer.said.extend(sent.message);
        self.remember(transfer.clone());
        Ok(transfer)
    }

    fn remember(&self, transfer: Transfer) {
        *self
            .last_transfer
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(transfer);
    }

    /// What the last transfer did, since the vault was unlocked.
    pub fn last_transfer(&self) -> Option<Transfer> {
        self.last_transfer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The slot, even after a panic elsewhere held the lock: what is in it is
    /// a vault or nothing, and neither is left half-changed by a panic.
    fn slot(&self) -> MutexGuard<'_, Option<Vault>> {
        self.vault.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// How well `title` matches what was typed: the lower, the better.
///
/// The core's search matches text anywhere - in the title, in a note, in a
/// public field - and that is right for finding things. It is not the order
/// a person expects once they have typed `mail`: the item called `mailbox`
/// first, and the note that mentions mail somewhere after it. So: the whole
/// title, then a title that starts with it, then a word in the title that
/// does, then anywhere in the title, then everything the core found inside.
pub fn rank(title: &str, typed: &str) -> u8 {
    let title = title.to_lowercase();
    let typed = typed.trim().to_lowercase();
    if title == typed {
        0
    } else if title.starts_with(&typed) {
        1
    } else if title
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(&typed))
    {
        2
    } else if title.contains(&typed) {
        3
    } else {
        4
    }
}

fn refused(message: String) -> Failure {
    Failure {
        kind: FailureKind::Other,
        message,
    }
}

/// A file that moved on and will not open is not a wrong password, whatever the
/// core calls it: nobody just typed one.
fn cannot_catch_up(error: Error) -> Failure {
    match error {
        Error::KeysGone => Failure::locked(),
        Error::WrongPasswordOrNotAVault | Error::TooSmall => refused(
            "the file on disk changed and this window cannot read the new version; \
             nothing was written"
                .to_owned(),
        ),
        other => other.into(),
    }
}

/// The file a second launch was started on, for the window that is already
/// there: as [`opening`] reads one, relative to where that launch stood.
pub fn handed_over(args: &[String], launched_in: &Path) -> Option<String> {
    let file = args.iter().skip(1).find(|arg| !arg.starts_with('-'))?;
    if file.is_empty() {
        return None;
    }
    Some(launched_in.join(file).display().to_string())
}

/// The file a second launch handed over, until the page takes it.
///
/// Kept rather than only sent: a launch can come before the page is listening,
/// and an event sent to nobody is gone. The page takes what is here once it
/// listens, and again each time the event wakes it.
#[derive(Default)]
pub struct Handover(Mutex<Option<String>>);

impl Handover {
    /// Leaves `file` for the page, in place of any it did not take.
    pub fn offer(&self, file: String) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(file);
    }

    /// The file left for the page, once.
    pub fn take(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// Opens the vault at `path` — the slow part of unlocking, which stretches
/// the password on purpose and so belongs off the window's thread.
pub fn open(path: &Path, password: &[u8]) -> Result<Vault, Failure> {
    Vault::open(path, password).map_err(Failure::from)
}

/// The file the window was started on: the first argument that is not a flag,
/// as "Open with" and a terminal pass it, or else `SEFY_VAULT`, the one the
/// command line uses.
///
/// Nothing else, and nothing remembered: sefy keeps no list of the vaults it
/// has opened. A list like that, in a predictable place under the user's
/// profile, would name exactly the files the format spends all its effort
/// making unremarkable — to anyone who looks there, and to any scanner that
/// knows where sefy keeps its things.
pub fn opening(args: impl IntoIterator<Item = OsString>, vault_env: Option<OsString>) -> Opening {
    let argument = args
        .into_iter()
        .skip(1)
        .find(|arg| !arg.to_string_lossy().starts_with('-'));
    let file = argument
        .or(vault_env)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        // A path relative to wherever the launcher stood would mean another
        // file once the dialog moves elsewhere.
        .map(|path| std::path::absolute(&path).unwrap_or(path))
        .map(|path| path.display().to_string());
    Opening { file }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWORD: &[u8] = b"correct horse battery staple";

    fn password() -> Zeroizing<Vec<u8>> {
        Zeroizing::new(PASSWORD.to_vec())
    }

    fn vault_in(directory: &tempfile::TempDir) -> PathBuf {
        let path = directory.path().join("notes.bak");
        let mut vault = Vault::create(&path, PASSWORD).unwrap();
        vault
            .add(sefy_core::NewItem::new(
                "bank",
                sefy_core::Payload::Note {
                    text: "1234".into(),
                },
            ))
            .unwrap();
        vault.save().unwrap();
        path
    }

    #[test]
    fn an_unlocked_vault_answers_until_it_is_locked() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();

        let status = session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        assert_eq!(status.name, "notes.bak");
        assert_eq!(status.items, 1);
        assert!(session.is_open());
        assert_eq!(session.status().unwrap(), status);

        session.lock();
        assert!(!session.is_open());
        assert_eq!(session.status().unwrap_err().kind, FailureKind::Locked);
    }

    #[test]
    fn while_unlocked_the_window_answers_for_the_password_and_after_it_does_not() {
        use sefy_core::agent::LockState;
        use sefy_core::{Error, KeySource};

        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        let keeper = session.keeper();
        assert_eq!(keeper.state(), LockState::Locked);

        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        // What the command line would do: open the file with the keeper's key.
        let opened = Vault::open_through(&path, keeper.clone()).unwrap();
        assert_eq!(opened.list().unwrap().len(), 1);
        let LockState::Unlocked { idle, .. } = keeper.state() else {
            panic!("an unlocked window holds the password");
        };
        assert_eq!(idle, sefy_core::agent::IDLE);

        session.lock();
        assert_eq!(keeper.state(), LockState::Locked);
        assert!(matches!(
            keeper.key_for(&[1; sefy_core::crypto::SALT_LEN]),
            Err(Error::KeysGone)
        ));
    }

    #[test]
    fn a_launch_on_the_open_file_is_not_a_launch_on_another() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        // Nothing open: nothing to give way.
        assert!(!session.holds_other_than(&path));

        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        assert!(!session.holds_other_than(&path));
        // The same file, spelled through its folder.
        let spelled = directory.path().join(".").join("notes.bak");
        assert!(!session.holds_other_than(&spelled));

        let other = directory.path().join("other.bak");
        Vault::create(&other, PASSWORD).unwrap();
        assert!(session.holds_other_than(&other));
    }

    #[test]
    fn a_second_launch_names_its_file_from_where_it_stood() {
        let stood_in = Path::new("somewhere");
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(
            handed_over(&args(&["sefy-gui", "--flag", "notes.bak"]), stood_in),
            Some(stood_in.join("notes.bak").display().to_string())
        );
        assert_eq!(handed_over(&args(&["sefy-gui"]), stood_in), None);
        assert_eq!(handed_over(&args(&["sefy-gui", ""]), stood_in), None);

        // An absolute path stays what it is.
        let absolute = std::env::current_dir().unwrap().join("notes.bak");
        let spelled = absolute.display().to_string();
        assert_eq!(
            handed_over(&args(&["sefy-gui", &spelled]), stood_in),
            Some(spelled)
        );
    }

    #[test]
    fn what_was_handed_over_is_taken_once_and_the_latest_wins() {
        let handover = Handover::default();
        assert_eq!(handover.take(), None);
        handover.offer("first".into());
        handover.offer("second".into());
        assert_eq!(handover.take().as_deref(), Some("second"));
        assert_eq!(handover.take(), None);
    }

    #[test]
    fn a_wrong_password_is_refused_and_leaves_nothing_open() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();

        let failure = open(&path, b"not it").unwrap_err();
        assert_eq!(failure.kind, FailureKind::Refused);
        assert!(!session.is_open());
    }

    #[test]
    fn any_other_file_is_refused_the_same_way() {
        // A file that is not a vault and a vault under another password are
        // one answer: there is nothing in either to tell them apart.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("holiday.jpg");
        std::fs::write(&path, vec![7u8; 4096]).unwrap();
        assert_eq!(
            open(&path, PASSWORD).unwrap_err().kind,
            FailureKind::Refused
        );

        let tiny = directory.path().join("tiny");
        std::fs::write(&tiny, b"x").unwrap();
        assert_eq!(
            open(&tiny, PASSWORD).unwrap_err().kind,
            FailureKind::Refused
        );
    }

    #[test]
    fn a_file_that_is_not_there_is_missing_rather_than_refused() {
        let directory = tempfile::tempdir().unwrap();
        let failure = open(&directory.path().join("gone.bak"), PASSWORD).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Missing);
    }

    #[test]
    fn unlocking_another_vault_closes_the_first() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let session = Session::default();

        session
            .hold(open(&vault_in(&first), PASSWORD).unwrap(), password())
            .unwrap();
        let other = second.path().join("archive.dat");
        Vault::create(&other, PASSWORD).unwrap();
        session
            .hold(open(&other, PASSWORD).unwrap(), password())
            .unwrap();

        let status = session.status().unwrap();
        assert_eq!(status.name, "archive.dat");
        assert_eq!(status.items, 0);
    }

    /// A vault with a login carrying a one-time password key, a note, a
    /// file and a card, open in a session.
    fn browsing(directory: &tempfile::TempDir) -> Session {
        use sefy_core::{Field, NewItem, Payload};
        let path = directory.path().join("notes.bak");
        let mut vault = Vault::create(&path, PASSWORD).unwrap();
        vault
            .add(
                NewItem::new(
                    "mailbox",
                    Payload::fields(
                        ItemKind::Login,
                        [
                            Field::public("login", "kmiller-dev"),
                            Field::secret("password", "hunter2-not-real"),
                            Field::secret("totp", "JBSWY3DPEHPK3PXP"),
                        ],
                    ),
                )
                .with_tags(["work"]),
            )
            .unwrap();
        vault
            .add(NewItem::new(
                "recovery codes",
                Payload::Note {
                    text: "mailbox: F7Q2-XXXX".into(),
                },
            ))
            .unwrap();
        vault
            .add(NewItem::new(
                "scan.pdf",
                Payload::File {
                    filename: "scan.pdf".into(),
                    bytes: vec![1, 2, 3],
                },
            ))
            .unwrap();
        vault
            .add(NewItem::new(
                "travel card",
                Payload::fields(
                    ItemKind::Card,
                    [
                        Field::secret("number", "4111111111111111"),
                        Field::public("expires", "2030-01"),
                    ],
                ),
            ))
            .unwrap();
        vault.save().unwrap();
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        session
    }

    fn titles(rows: &[ItemRow]) -> Vec<&str> {
        rows.iter().map(|row| row.title.as_str()).collect()
    }

    fn id_of(session: &Session, title: &str) -> i64 {
        session
            .items(&Filter::default())
            .unwrap()
            .into_iter()
            .find(|row| row.title == title)
            .unwrap()
            .id
    }

    #[test]
    fn a_title_that_matches_comes_before_a_match_inside() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let filter = Filter {
            text: Some("mailbox".into()),
            ..Filter::default()
        };
        // The note says mailbox in its text and is newer; the login is called
        // mailbox and comes first anyway.
        assert_eq!(
            titles(&session.items(&filter).unwrap()),
            ["mailbox", "recovery codes"]
        );
    }

    #[test]
    fn the_rail_narrows_by_kind_and_by_tag() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let notes = Filter {
            kind: Some("note".into()),
            ..Filter::default()
        };
        assert_eq!(titles(&session.items(&notes).unwrap()), ["recovery codes"]);
        let work = Filter {
            tag: Some("work".into()),
            ..Filter::default()
        };
        assert_eq!(titles(&session.items(&work).unwrap()), ["mailbox"]);
    }

    #[test]
    fn a_secret_is_never_what_the_list_is_searched_by() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let filter = Filter {
            text: Some("hunter2".into()),
            ..Filter::default()
        };
        assert!(session.items(&filter).unwrap().is_empty());
    }

    #[test]
    fn the_card_names_every_secret_and_carries_none() {
        use crate::wire::{CardBody, CardField};
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let card = session.card(id_of(&session, "mailbox"), 0).unwrap();

        let wire = serde_json::to_string(&card).unwrap();
        for secret in ["hunter2-not-real", "JBSWY3DPEHPK3PXP"] {
            assert!(!wire.contains(secret), "the card carries {secret}: {wire}");
        }
        let CardBody::Record { fields } = card.body else {
            panic!("a login is a record");
        };
        assert_eq!(
            fields,
            [
                CardField::Plain {
                    name: "login".into(),
                    value: "kmiller-dev".into(),
                    days_left: None,
                },
                CardField::Secret {
                    name: "password".into()
                },
                CardField::Code {
                    name: "totp".into(),
                    period: 30
                },
            ]
        );
    }

    #[test]
    fn the_date_a_card_stops_working_is_counted_down() {
        use crate::wire::{CardBody, CardField};
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        // 2029-12-31 00:00 UTC: the card runs to the end of January 2030.
        let card = session
            .card(id_of(&session, "travel card"), 1_893_369_600)
            .unwrap();
        let CardBody::Record { fields, .. } = card.body else {
            panic!("a card is a record");
        };
        let Some(CardField::Plain { days_left, .. }) = fields
            .iter()
            .find(|field| matches!(field, CardField::Plain { name, .. } if name == "expires"))
        else {
            panic!("the card shows when it stops working");
        };
        assert_eq!(*days_left, Some(31));
    }

    #[test]
    fn a_copy_takes_what_sefy_get_would() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);

        let (value, what) = session.value(id_of(&session, "mailbox"), None, 0).unwrap();
        assert_eq!(
            (value.as_str(), what.as_str()),
            ("hunter2-not-real", "password")
        );

        let (value, what) = session
            .value(id_of(&session, "recovery codes"), None, 0)
            .unwrap();
        assert_eq!(
            (value.as_str(), what.as_str()),
            ("mailbox: F7Q2-XXXX", "text")
        );

        let (value, what) = session
            .value(id_of(&session, "mailbox"), Some("login"), 0)
            .unwrap();
        assert_eq!((value.as_str(), what.as_str()), ("kmiller-dev", "login"));
    }

    #[test]
    fn a_one_time_password_gives_its_code_and_never_its_key() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let id = id_of(&session, "mailbox");
        let (first, what) = session.value(id, Some("totp"), 59).unwrap();
        assert_eq!(what, "code");
        assert_eq!(first.len(), 6);
        assert!(first.chars().all(|c| c.is_ascii_digit()));
        // Another period, another code.
        let (later, _) = session.value(id, Some("totp"), 1_111_111_109).unwrap();
        assert_ne!(first, later);
    }

    #[test]
    fn a_file_is_not_copied() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let failure = session
            .value(id_of(&session, "scan.pdf"), None, 0)
            .unwrap_err();
        assert!(
            failure.message.contains("sefy extract"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn a_locked_session_shows_and_copies_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let id = id_of(&session, "mailbox");
        session.lock();
        assert_eq!(
            session.items(&Filter::default()).unwrap_err().kind,
            FailureKind::Locked
        );
        assert_eq!(session.card(id, 0).unwrap_err().kind, FailureKind::Locked);
        assert_eq!(
            session.value(id, None, 0).unwrap_err().kind,
            FailureKind::Locked
        );
    }

    /// `SEFY_REMOTE_NAME` is left alone: these tests pass the default through.
    fn stand_in_transport(directory: &Path, remote: &Path) -> Plugin {
        let manifest = r#"{"protocol_version":1,"name":"file","version":"0.1.0","operations":["push","pull"]}"#;
        let remote = remote.display().to_string();
        #[cfg(windows)]
        {
            let manifest_path = directory.join("manifest.json");
            std::fs::write(&manifest_path, manifest).unwrap();
            let script = directory.join("transport.ps1");
            std::fs::write(
                &script,
                format!(
                    "$request = [Console]::In.ReadToEnd() | ConvertFrom-Json\r\n\
                     if ($request.operation -eq 'push') {{ Copy-Item -LiteralPath $request.file -Destination '{remote}' -Force }}\r\n\
                     else {{ Copy-Item -LiteralPath '{remote}' -Destination $request.file -Force }}\r\n"
                ),
            )
            .unwrap();
            std::fs::write(
                directory.join("sefy-plugin-file.cmd"),
                format!(
                    "@echo off\r\n\
                     if \"%1\"==\"--manifest\" (type \"{manifest}\" & exit /b 0)\r\n\
                     powershell -NoProfile -ExecutionPolicy Bypass -File \"{script}\"\r\n",
                    manifest = manifest_path.display(),
                    script = script.display(),
                ),
            )
            .unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = directory.join("sefy-plugin-file");
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\n\
                     if [ \"$1\" = \"--manifest\" ]; then echo '{manifest}'; exit 0; fi\n\
                     request=$(cat)\n\
                     file=$(printf '%s' \"$request\" | sed -e 's/.*\"file\":\"//' -e 's/\".*//')\n\
                     case \"$request\" in\n\
                     *'\"operation\":\"push\"'*) cp \"$file\" '{remote}' ;;\n\
                     *) cp '{remote}' \"$file\" ;;\n\
                     esac\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        sefy_core::plugin::find(
            sefy_core::plugin::discover_in(&[directory.to_path_buf()]),
            "file",
        )
        .expect("the stand-in transport is found")
    }

    #[test]
    fn the_window_catches_up_with_what_the_command_line_wrote() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        assert_eq!(session.look_again(), Seen::Nothing, "nothing changed yet");

        // What `sefy add` does while the window is open.
        let mut outside = Vault::open(&path, PASSWORD).unwrap();
        outside
            .add(sefy_core::NewItem::new(
                "from outside",
                Payload::Note { text: "x".into() },
            ))
            .unwrap();
        outside.save().unwrap();

        assert_eq!(session.look_again(), Seen::Moved);
        assert_eq!(session.status().unwrap().items, 2);
        assert_eq!(session.look_again(), Seen::Nothing, "it is caught up now");
    }

    #[test]
    fn a_write_from_the_window_keeps_what_the_command_line_wrote() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();

        let mut outside = Vault::open(&path, PASSWORD).unwrap();
        outside
            .add(sefy_core::NewItem::new(
                "from outside",
                Payload::Note { text: "x".into() },
            ))
            .unwrap();
        outside.save().unwrap();

        // Before the window has looked: the write itself has to catch up.
        session
            .set_policy(Policy {
                lock_after_seconds: 300,
                clipboard_seconds: 90,
            })
            .unwrap();

        let reopened = Vault::open(&path, PASSWORD).unwrap();
        assert_eq!(reopened.list().unwrap().len(), 2, "the write erased it");
        assert_eq!(reopened.policy().unwrap().clipboard_seconds, 90);
    }

    #[test]
    fn looking_at_the_file_is_not_someone_using_the_vault() {
        use sefy_core::agent::LockState;

        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        let vault = open(&path, PASSWORD).unwrap();
        let given = std::time::Instant::now();
        session.hold(vault, password()).unwrap();
        let keeper = session.keeper();
        std::thread::sleep(Duration::from_millis(1100));

        let mut outside = Vault::open(&path, PASSWORD).unwrap();
        outside
            .add(sefy_core::NewItem::new(
                "from outside",
                Payload::Note { text: "x".into() },
            ))
            .unwrap();
        outside.save().unwrap();
        assert_eq!(session.look_again(), Seen::Moved);

        // Taken from the answer rather than assumed: reading the file again takes
        // up what it says about this too.
        let LockState::Unlocked { left, idle } = keeper.state() else {
            panic!("it is still unlocked");
        };
        // Everything since the password was given is on the clock. Measured
        // against that rather than against a number, because the look itself
        // stretches the password, which can take a second under load.
        let on_the_clock = idle - left;
        let since_given = given.elapsed();
        assert!(
            on_the_clock + Duration::from_millis(500) >= since_given,
            "the idle time started over: {on_the_clock:?} on the clock, {since_given:?} since it was given"
        );
    }

    #[test]
    fn a_file_that_will_not_open_is_said_once_and_refuses_writes() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        let good = std::fs::read(&path).unwrap();

        // Halfway through a sync client's replacing it, or another password.
        let other = directory.path().join("other.bak");
        Vault::create(&other, b"another password").unwrap();
        std::fs::copy(&other, &path).unwrap();

        assert_eq!(session.look_again(), Seen::Unreadable);
        assert_eq!(session.look_again(), Seen::Nothing, "said once");
        assert_eq!(session.status().unwrap().items, 1, "what was read stays");

        let refusal = session
            .set_policy(Policy {
                lock_after_seconds: 300,
                clipboard_seconds: 90,
            })
            .unwrap_err();
        assert!(
            refusal.message.contains("nothing was written"),
            "{refusal:?}"
        );
        assert_eq!(refusal.kind, FailureKind::Other);

        // The file comes back, and the window says so.
        std::fs::write(&path, &good).unwrap();
        assert_eq!(session.look_again(), Seen::Moved);
        assert_eq!(session.look_again(), Seen::Nothing);
    }

    #[test]
    fn a_setting_reaches_the_keeper_and_the_file() {
        use sefy_core::agent::LockState;

        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        let set = Policy {
            lock_after_seconds: 60,
            clipboard_seconds: 15,
        };
        session.set_policy(set).unwrap();
        assert_eq!(session.policy().unwrap(), set);

        let LockState::Unlocked { idle, .. } = session.keeper().state() else {
            panic!("it is still unlocked");
        };
        assert_eq!(idle, Duration::from_secs(60));

        // The next unlock goes by it.
        session.lock();
        assert_eq!(session.policy().unwrap(), Policy::default());
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        let LockState::Unlocked { idle, .. } = session.keeper().state() else {
            panic!("it is unlocked");
        };
        assert_eq!(idle, Duration::from_secs(60));
    }

    #[test]
    fn a_version_written_here_says_where() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let id = id_of(&session, "recovery codes");
        session
            .write(|vault| {
                vault.update(
                    id,
                    None,
                    Some(Payload::Note {
                        text: "changed".into(),
                    }),
                    None,
                )?;
                vault.save()?;
                Ok(())
            })
            .unwrap();
        let versions = session.history(id, 0).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(
            versions.last().unwrap().device,
            sefy_core::device::name(),
            "the window's versions go unmarked"
        );
    }

    #[test]
    fn the_history_names_secrets_and_carries_none() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let id = id_of(&session, "mailbox");
        let (before, _) = session.value(id, Some("password"), 0).unwrap();
        session
            .write(|vault| {
                let item = vault.get(id)?;
                let Payload::Fields { kind, mut fields } = item.payload else {
                    panic!("a login is a record");
                };
                fields
                    .iter_mut()
                    .find(|field| field.name == "password")
                    .unwrap()
                    .value = "new-not-real".into();
                vault.update(id, None, Some(Payload::Fields { kind, fields }), None)?;
                vault.save()?;
                Ok(())
            })
            .unwrap();

        let versions = session.history(id, 0).unwrap();
        let wire = serde_json::to_string(&versions).unwrap();
        for secret in [before.as_str(), "new-not-real", "JBSWY3DPEHPK3PXP"] {
            assert!(
                !wire.contains(secret),
                "the history carries {secret}: {wire}"
            );
        }
        assert_eq!(versions[1].changed, ["password"]);
        assert!(versions[1].current);
        assert!(!versions[0].current);
        assert_eq!((versions[0].number, versions[1].number), (1, 2));

        // Asked for, one field of one version comes across, and only then.
        let (old, what) = session
            .value_of(id, Some(&versions[0].uuid), Some("password"), 0)
            .unwrap();
        assert_eq!((old.as_str(), what.as_str()), (before.as_str(), "password"));
        let (now, _) = session.value(id, Some("password"), 0).unwrap();
        assert_eq!(now.as_str(), "new-not-real");
    }

    #[test]
    fn restoring_brings_it_back_and_can_be_undone() {
        let directory = tempfile::tempdir().unwrap();
        let session = browsing(&directory);
        let id = id_of(&session, "recovery codes");
        session
            .write(|vault| {
                vault.update(
                    id,
                    None,
                    Some(Payload::Note {
                        text: "second".into(),
                    }),
                    None,
                )?;
                vault.save()?;
                Ok(())
            })
            .unwrap();
        let versions = session.history(id, 0).unwrap();
        assert!(session.restore(id, &versions[0].uuid).unwrap());
        let (value, _) = session.value(id, None, 0).unwrap();
        assert_eq!(value.as_str(), "mailbox: F7Q2-XXXX");

        let after = session.history(id, 0).unwrap();
        assert_eq!(after.len(), 3, "what it replaced is a version of its own");

        // Nothing to bring back: nothing is written.
        let current = after.last().unwrap().uuid.clone();
        assert!(!session.restore(id, &current).unwrap());
    }

    #[test]
    fn a_new_password_takes_the_old_one_and_replaces_it_everywhere() {
        use sefy_core::agent::LockState;

        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();

        let refused = session.change_password(b"not it", b"new").unwrap_err();
        assert_eq!(refused.kind, FailureKind::Refused);
        assert!(Vault::open(&path, PASSWORD).is_ok(), "the file was changed");

        assert_eq!(
            session.change_password(PASSWORD, b"").unwrap_err().kind,
            FailureKind::Other,
            "an empty password would lock the vault for good"
        );

        session.change_password(PASSWORD, b"new one").unwrap();
        assert!(
            Vault::open(&path, PASSWORD).is_err(),
            "the old one still opens it"
        );
        assert!(Vault::open(&path, b"new one").is_ok());

        // The window goes on: the keeper answers for the new password, and the
        // command line through it can still seal.
        assert!(session.is_open());
        assert!(matches!(
            session.keeper().state(),
            LockState::Unlocked { .. }
        ));
        let mut through = Vault::open_through(&path, session.keeper()).unwrap();
        through
            .add(sefy_core::NewItem::new(
                "after",
                Payload::Note { text: "x".into() },
            ))
            .unwrap();
        through.save().unwrap();
    }

    #[test]
    fn a_wipe_takes_the_name_and_the_password_both() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();

        for typed in ["", "notes", "NOTES.BAK"] {
            let refused = session.open_to_wipe(PASSWORD, typed).err().unwrap();
            assert!(refused.message.contains("nothing was wiped"), "{refused:?}");
        }
        assert_eq!(
            session
                .open_to_wipe(b"not it", "notes.bak")
                .err()
                .unwrap()
                .kind,
            FailureKind::Refused
        );
        assert!(path.exists());

        let vault = session.open_to_wipe(PASSWORD, "notes.bak").unwrap();
        let wiped = vault.wipe();
        assert!(wiped.complete());
        assert!(!path.exists());
    }

    /// A vault at a remote holding one note of its own, under `remote_password`.
    fn remote_with(directory: &Path, remote_password: &[u8]) -> PathBuf {
        let remote = directory.join("remote.bin");
        let mut vault = Vault::create(&remote, remote_password).unwrap();
        vault
            .add(sefy_core::NewItem::new(
                "their note",
                Payload::Note { text: "x".into() },
            ))
            .unwrap();
        vault.save().unwrap();
        remote
    }

    #[test]
    fn a_sync_folds_in_what_came_and_sends_the_result_back() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let remote = remote_with(directory.path(), PASSWORD);
        let plugin = stand_in_transport(directory.path(), &remote);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();

        let transfer = session.sync_through(&plugin, None, 1_000).unwrap();
        assert_eq!(transfer.operation, "sync");
        assert_eq!(transfer.send_failure, None);
        assert_eq!(transfer.added.len(), 1);
        assert_eq!(transfer.added[0].title, "their note");
        assert!(transfer.kept_copy.is_some(), "a merge keeps a copy first");
        assert_eq!(session.last_transfer(), Some(transfer));

        // Both ends hold both notes, and the file says it travelled.
        assert_eq!(session.status().unwrap().items, 2);
        let there = Vault::open(&remote, PASSWORD).unwrap();
        assert_eq!(there.list().unwrap().len(), 2);
        let stamp = session.status().unwrap().last_sync.unwrap();
        assert_eq!(
            (stamp.transport.as_str(), stamp.operation.as_str()),
            ("file", "sync")
        );

        // What it did is gone with the vault: it has titles in it.
        session.lock();
        assert_eq!(session.last_transfer(), None);
    }

    #[test]
    fn a_copy_under_another_password_is_asked_for() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let remote = remote_with(directory.path(), b"the remote's password");
        let plugin = stand_in_transport(directory.path(), &remote);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        let before = std::fs::read(&remote).unwrap();

        let refused = session.sync_through(&plugin, None, 1_000).unwrap_err();
        assert_eq!(refused.kind, FailureKind::RemoteRefused);
        assert_eq!(session.status().unwrap().items, 1, "nothing was folded in");
        assert_eq!(std::fs::read(&remote).unwrap(), before, "nothing was sent");

        let transfer = session
            .sync_through(&plugin, Some(b"the remote's password".as_slice()), 1_000)
            .unwrap();
        assert_eq!(transfer.added.len(), 1);
    }

    #[test]
    fn a_transfer_does_not_fold_into_a_vault_opened_since() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();
        let (_, opened) = session.open_file().unwrap();

        // Locked and unlocked on another file while the network took its time.
        let other = directory.path().join("other.bak");
        Vault::create(&other, PASSWORD).unwrap();
        session
            .hold(open(&other, PASSWORD).unwrap(), password())
            .unwrap();

        let failure = session
            .write_to(opened, |vault| {
                vault.add(sefy_core::NewItem::new(
                    "stray",
                    Payload::Note { text: "x".into() },
                ))?;
                vault.save()?;
                Ok(())
            })
            .unwrap_err();
        assert_eq!(failure.kind, FailureKind::Locked);
        assert_eq!(
            session.status().unwrap().items,
            0,
            "it went into the wrong vault"
        );
    }

    #[test]
    fn one_transfer_at_a_time() {
        let session = Session::default();
        let first = session.one_transfer().unwrap();
        assert!(session.one_transfer().is_err());
        drop(first);
        assert!(session.one_transfer().is_ok());
    }

    #[test]
    fn a_first_copy_goes_up_with_a_push() {
        let directory = tempfile::tempdir().unwrap();
        let path = vault_in(&directory);
        let remote = directory.path().join("remote.bin");
        let plugin = stand_in_transport(directory.path(), &remote);
        let session = Session::default();
        session
            .hold(open(&path, PASSWORD).unwrap(), password())
            .unwrap();

        let transfer = session.push_through(&plugin, 1_000).unwrap();
        assert_eq!(transfer.operation, "push");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            std::fs::read(&remote).unwrap(),
            "what travels is the sealed file, carrying the note about this very trip"
        );
        assert_eq!(
            session.status().unwrap().last_sync.unwrap().operation,
            "push"
        );
    }

    #[test]
    fn ranking_goes_whole_title_then_start_then_word_then_anywhere() {
        assert_eq!(rank("Mailbox", "mailbox"), 0);
        assert_eq!(rank("mailbox archive", "mail"), 1);
        assert_eq!(rank("work · mailroom", "mail"), 2);
        assert_eq!(rank("email", "mail"), 3);
        assert_eq!(rank("recovery codes", "mail"), 4);
    }

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_file_named_at_launch_comes_before_the_environment() {
        let opening = opening(
            args(&["sefy-gui", "--some-flag", "notes.bak"]),
            Some("elsewhere.dat".into()),
        );
        let file = opening.file.unwrap();
        assert!(file.ends_with("notes.bak"), "{file}");
        assert!(
            Path::new(&file).is_absolute(),
            "{file} would mean another file once the dialog moves"
        );
    }

    #[test]
    fn without_an_argument_the_window_opens_on_sefy_vault() {
        let opening = opening(args(&["sefy-gui"]), Some("elsewhere.dat".into()));
        assert!(opening.file.unwrap().ends_with("elsewhere.dat"));
    }

    #[test]
    fn with_neither_the_person_picks_the_file() {
        assert_eq!(opening(args(&["sefy-gui"]), None).file, None);
        assert_eq!(opening(args(&["sefy-gui"]), Some("".into())).file, None);
    }
}
