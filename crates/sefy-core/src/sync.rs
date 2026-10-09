//! Moving a vault between this machine and a remote, through a transport.
//!
//! A transport ([`crate::plugin`]) knows how to put one opaque file somewhere
//! and fetch it back. It does not know what the file is, cannot read it, and
//! therefore cannot say what changed. Everything about *meaning* happens here,
//! with the vault open:
//!
//! - **push** hands the sealed file to the plugin. The remote copy is replaced.
//! - **pull** asks the plugin for the remote copy, opens it, and folds it into
//!   this vault with [`crate::merge`]. Nothing local is overwritten and nothing
//!   is deleted; an item changed on both sides is kept twice.
//! - **sync** is pull then push: take everything from over there, then publish
//!   the result. The order matters — pushing first would replace the remote
//!   copy with one that never saw its contents.
//!
//! # What touches the disk
//!
//! A pull needs somewhere to put the bytes the plugin fetches, because a
//! transport writes a file — that is the whole protocol. That file is the
//! *sealed* blob, exactly what sits at the remote and exactly what an onlooker
//! would find beside the vault: the plaintext invariant is untouched. It is
//! still removed as soon as it has been read, on the failure path as well as
//! the successful one, because a stray copy of a vault is a copy that outlives
//! the password change that was supposed to retire it.

//!
//! # Looking before leaping
//!
//! [`preview_pull`] and [`preview_sync`] fetch the remote copy as a pull does
//! and run the merge on a copy of this vault held in memory. Nothing is written
//! here, nothing is sent, and no transfer is recorded: the vault is exactly as
//! it was, and the report is the one the real operation would give.

use crate::error::{Error, Result};
use crate::merge::{self, MergeReport, merge};
use crate::plugin::{Operation, Plugin, Report, Request, invoke};
use crate::vault::Vault;
use std::path::Path;

/// What a pull did: what the transport said, and what the merge found.
#[derive(Debug, Clone)]
pub struct PullReport {
    /// The plugin's own line about the transfer, if it had one.
    pub transport: Report,
    /// What folding the remote copy into this vault changed.
    pub merged: MergeReport,
}

/// The remote copy as a transport delivered it: still sealed.
pub struct Fetched {
    /// The plugin's own line about the transfer, if it had one.
    pub transport: Report,
    /// The bytes of the file at the remote — ciphertext, exactly what an
    /// onlooker would find there.
    pub sealed: Vec<u8>,
}

impl Fetched {
    /// Opens the fetched copy, for reading only.
    ///
    /// It has no file of its own: it is folded from, compared, and dropped.
    pub fn open(&self, password: &[u8]) -> Result<Vault> {
        Vault::from_sealed(&self.sealed, password)
    }
}

/// What a sync would do in each direction.
#[derive(Debug, Clone)]
pub struct SyncPreview {
    /// The plugin's own line about fetching the remote copy, if it had one.
    pub transport: Report,
    /// What the pull would fold into this vault.
    pub coming: MergeReport,
    /// What the remote copy would gain from the push that follows: what this
    /// vault, once merged, has and the remote copy does not.
    pub going: MergeReport,
}

/// Sends this vault's file to the remote, replacing what is there.
///
/// The vault is stamped and saved first, so that the file going up is the one
/// carrying the note about this very transfer. Stamping afterwards would be
/// the obvious order and the wrong one: the copy at the remote would say it
/// had never been synced, and pulling it onto another machine would bring back
/// that answer as if it were true.
///
/// A caller with other unsaved changes in memory is still expected to have
/// saved them, the same as every other command that reads the file back — the
/// save here writes whatever is in memory, it does not make that the caller's
/// business.
pub fn push(vault: &mut Vault, plugin: &Plugin, name: &str) -> Result<Report> {
    push_labelled(vault, plugin, name, "push")
}

/// A push that records itself under a given name.
///
/// `sync` uses it to stamp the copy it uploads as a sync rather than as the
/// push it ends with — the user asked for one operation and the file should
/// say which one it was.
fn push_labelled(
    vault: &mut Vault,
    plugin: &Plugin,
    name: &str,
    operation: &str,
) -> Result<Report> {
    stamp(vault, plugin, operation)?;

    invoke(
        plugin,
        &Request {
            operation: Operation::Push,
            file: vault.path(),
            name,
        },
    )
}

/// Records the transfer in the vault and writes it to disk.
///
/// Its own function because both transfers need it and each calls it at a
/// different point: a push stamps before uploading, so that the copy going up
/// carries the note about its own journey.
fn stamp(vault: &mut Vault, plugin: &Plugin, operation: &str) -> Result<()> {
    vault.record_sync(plugin.name(), operation)?;
    vault.save()
}

/// Fetches the remote copy and folds it into this vault.
///
/// `remote_password` is separate from the vault's own because the copy on the
/// other side may well be under a different one — the same reason `merge` asks
/// for it separately. The merge writes the vault back itself, so a pull that
/// brought something across is on disk by the time it returns.
pub fn pull(
    vault: &mut Vault,
    plugin: &Plugin,
    name: &str,
    remote_password: &[u8],
) -> Result<PullReport> {
    let fetched = fetch(plugin, name)?;
    let remote = fetched.open(remote_password)?;
    let merged = merge(vault, &remote)?;

    // After the merge rather than inside it: `merge` folds one vault into
    // another and knows nothing about transports, which is what lets it be
    // called on a file from anywhere. The stamp is written whether or not the
    // merge changed anything — a pull that found the two in step still
    // reached the remote, and that is what the stamp says.
    stamp(vault, plugin, "pull")?;

    Ok(PullReport {
        transport: fetched.transport,
        merged,
    })
}

/// Fetches the remote copy without folding it in anywhere.
///
/// The bytes pass through a scratch file only because a transport writes a
/// file — that is the whole protocol — and the file is gone again before this
/// returns, on the failure path as well as the successful one. What comes back
/// is held in memory, still sealed.
pub fn fetch(plugin: &Plugin, name: &str) -> Result<Fetched> {
    let scratch = Scratch::new()?;

    let transport = invoke(
        plugin,
        &Request {
            operation: Operation::Pull,
            file: scratch.path(),
            name,
        },
    )?;

    // A transport that reports success without leaving a file is a bug in the
    // transport, but it would surface later as "wrong password or not a
    // vault", which sends the user looking in the wrong place entirely.
    if !scratch.path().exists() {
        return Err(Error::PluginFailed {
            name: plugin.name().to_owned(),
            reason: "it reported success but wrote no file".into(),
        });
    }

    let sealed = std::fs::read(scratch.path())
        .map_err(|source| Error::io("cannot read what the transport fetched", source))?;
    Ok(Fetched { transport, sealed })
}

/// What [`pull`] would fold into this vault, with nothing written.
///
/// The remote copy is fetched for real — there is no other way to know what
/// it holds — but this vault is left exactly as it was, and no transfer is
/// recorded.
pub fn preview_pull(
    vault: &Vault,
    plugin: &Plugin,
    name: &str,
    remote_password: &[u8],
) -> Result<PullReport> {
    let fetched = fetch(plugin, name)?;
    let remote = fetched.open(remote_password)?;
    Ok(PullReport {
        merged: merge::preview(vault, &remote)?,
        transport: fetched.transport,
    })
}

/// What [`sync`] would do in each direction, with nothing written or sent.
pub fn preview_sync(
    vault: &Vault,
    plugin: &Plugin,
    name: &str,
    remote_password: &[u8],
) -> Result<SyncPreview> {
    let fetched = fetch(plugin, name)?;
    let remote = fetched.open(remote_password)?;
    let (coming, going) = both_ways(vault, remote)?;
    Ok(SyncPreview {
        transport: fetched.transport,
        coming,
        going,
    })
}

/// What a sync between this vault and a remote copy would move each way.
///
/// The first half is the pull's merge. The second is what the push would give
/// the remote: the merged result folded into the remote copy, in memory. A push
/// replaces the remote file whole, and the merged result already holds all of
/// the remote's, so what the fold adds or moves on is exactly what the remote
/// gains.
pub fn both_ways(vault: &Vault, mut remote: Vault) -> Result<(MergeReport, MergeReport)> {
    let (merged, coming) = merge::folded(vault, &remote)?;
    let going = merge::fold(&mut remote, &merged)?;
    Ok((coming, going))
}

/// Pull, then push: take what is at the remote, then publish the result.
///
/// The order is not a preference. Pushing first would replace the remote copy
/// with one that never saw its contents — every secret added on the other
/// machine would be gone from the only copy that had it. Pulling first means
/// the file that goes up already contains both sides.
///
/// The push always runs. Whether the local file differs from the remote one is
/// not knowable from here — the format carries no counter in the clear, by
/// design — so a sync that changed nothing costs one upload rather than a
/// guess about whether it was needed.
pub fn sync(
    vault: &mut Vault,
    plugin: &Plugin,
    name: &str,
    remote_password: &[u8],
) -> Result<SyncReport> {
    let pulled = pull(vault, plugin, name, remote_password)?;
    // Stamped as a sync, overwriting the "pull" the first leg left: one
    // operation was asked for, and the file that goes up should say which.
    let pushed = push_labelled(vault, plugin, name, "sync")?;

    Ok(SyncReport { pulled, pushed })
}

/// What a sync did on each leg.
#[derive(Debug, Clone)]
pub struct SyncReport {
    /// What came down and what the merge made of it.
    pub pulled: PullReport,
    /// What the transport said about sending the result back up.
    pub pushed: Report,
}

/// A scratch path for the sealed blob a transport fetches.
///
/// Not a [`tempfile::NamedTempFile`]: the plugin has to create the file itself,
/// and on Windows an already-open handle stops it from doing so. What is kept
/// is the directory, which removes whatever the plugin left when it drops —
/// including on the error paths above.
struct Scratch {
    /// Never read: holding it is the point. When this value drops, the
    /// directory and everything the transport put in it goes with it.
    _directory: tempfile::TempDir,
    file: std::path::PathBuf,
}

impl Scratch {
    fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix(".sefy-")
            .tempdir()
            .map_err(|source| Error::io("cannot create a temporary directory", source))?;
        // A neutral name: a temporary directory listing should not announce
        // what the file inside it is either.
        let file = directory.path().join("blob");
        Ok(Self {
            _directory: directory,
            file,
        })
    }

    fn path(&self) -> &Path {
        &self.file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NewItem, Payload};
    use crate::plugin::discover_in;
    use std::path::PathBuf;

    const PASSWORD: &[u8] = b"master password";

    /// Writes a stand-in transport whose "remote" is another file on this
    /// machine, and which learns where to read or write **from the request**.
    ///
    /// The path is parsed out of the JSON on stdin rather than passed in
    /// through the environment. That is the barrier worth testing: a fixture
    /// told the path some other way would pass even if sefy sent the wrong one.
    ///
    /// Whatever path arrives is also recorded in `log`, so a test can ask
    /// afterwards what sefy actually handed over.
    fn file_transport(directory: &Path, remote: &Path, log: &Path) {
        let manifest = r#"{"protocol_version":1,"name":"file","version":"0.1.0","operations":["push","pull"]}"#;
        let remote = remote.display().to_string();
        let log = log.display().to_string();

        #[cfg(windows)]
        {
            let manifest_path = directory.join("manifest.json");
            std::fs::write(&manifest_path, manifest).unwrap();

            // PowerShell rather than cmd: the request has to be parsed, and
            // batch string handling would make the fixture the hard part.
            let script = directory.join("transport.ps1");
            std::fs::write(
                &script,
                format!(
                    "$request = [Console]::In.ReadToEnd() | ConvertFrom-Json\r\n\
                     Set-Content -LiteralPath '{log}' -Value $request.file -NoNewline\r\n\
                     if ($request.operation -eq 'push') {{ Copy-Item -LiteralPath $request.file -Destination '{remote}' -Force }}\r\n\
                     else {{ Copy-Item -LiteralPath '{remote}' -Destination $request.file -Force }}\r\n"
                ),
            )
            .unwrap();

            let path = directory.join("sefy-plugin-file.cmd");
            std::fs::write(
                &path,
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

        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = directory.join("sefy-plugin-file");
            std::fs::write(
                &path,
                format!(
                    // The path is cut out by name, not by position: a fixture
                    // keyed on field order would keep passing if `file` moved
                    // and started carrying something else.
                    r#"#!/bin/sh
if [ "$1" = "--manifest" ]; then
  printf '%s' '{manifest}'
  exit 0
fi
REQUEST=$(cat)
FILE=$(printf '%s' "$REQUEST" | sed 's/.*"file":"//; s/".*//')
printf '%s' "$FILE" > '{log}'
case "$REQUEST" in
  *'"operation":"push"'*) cp "$FILE" '{remote}' ;;
  *) cp '{remote}' "$FILE" ;;
esac
"#
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    /// A transport that answers `--manifest` and does nothing else.
    fn silent_transport(directory: &Path, operations: &str) {
        let manifest = format!(
            r#"{{"protocol_version":1,"name":"silent","version":"0.1.0","operations":{operations}}}"#
        );

        #[cfg(windows)]
        {
            let manifest_path = directory.join("silent.json");
            std::fs::write(&manifest_path, &manifest).unwrap();
            std::fs::write(
                directory.join("sefy-plugin-silent.cmd"),
                format!(
                    "@echo off\r\nif \"%1\"==\"--manifest\" (type \"{}\")\r\n",
                    manifest_path.display()
                ),
            )
            .unwrap();
        }

        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = directory.join("sefy-plugin-silent");
            std::fs::write(
                &path,
                format!(
                    r#"#!/bin/sh
if [ "$1" = "--manifest" ]; then
  printf '%s' '{manifest}'
fi
"#
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn installed(directory: &Path, name: &str) -> Plugin {
        discover_in(&[directory.to_path_buf()])
            .into_iter()
            .find(|plugin| plugin.name() == name)
            .unwrap_or_else(|| panic!("the {name} plugin must be discovered"))
    }

    fn note(vault: &mut Vault, title: &str, text: &str) {
        vault
            .add(NewItem::new(title, Payload::Note { text: text.into() }))
            .unwrap();
        vault.save().unwrap();
    }

    fn titles(vault: &Vault) -> Vec<String> {
        let mut names: Vec<String> = vault
            .list()
            .unwrap()
            .into_iter()
            .map(|summary| summary.title)
            .collect();
        names.sort();
        names
    }

    /// Where the transport was told to put the remote copy.
    fn fetched_path(log: &Path) -> PathBuf {
        let recorded = std::fs::read_to_string(log)
            .expect("the transport must have been asked to move something");
        PathBuf::from(recorded.trim())
    }

    /// A vault with one note, plus a copy of its file standing in for the
    /// remote — the two share an identity, which is what makes a later fold a
    /// merge rather than an import.
    fn remote_holding(directory: &Path, title: &str, password: &[u8]) -> PathBuf {
        let elsewhere = directory.join("elsewhere.bak");
        let mut theirs = Vault::create(&elsewhere, password).unwrap();
        note(&mut theirs, title, "from over there");
        let remote = directory.join("remote.bin");
        std::fs::copy(&elsewhere, &remote).unwrap();
        remote
    }

    #[test]
    fn a_push_puts_the_sealed_file_at_the_remote_byte_for_byte() {
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("notes.bak");
        let remote = directory.path().join("remote.bin");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "bank", "vault code 1234");
        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        push(&mut vault, &plugin, "vault").unwrap();

        assert_eq!(
            std::fs::read(&local).unwrap(),
            std::fs::read(&remote).unwrap(),
            "what travels is the sealed file, unchanged"
        );
    }

    #[test]
    fn what_the_transport_carries_is_never_readable_as_plaintext() {
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("notes.bak");
        let remote = directory.path().join("remote.bin");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "bank", "SECRETVALUE");
        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        push(&mut vault, &plugin, "vault").unwrap();

        let carried = std::fs::read(&remote).unwrap();
        assert!(
            !carried
                .windows(b"SECRETVALUE".len())
                .any(|window| window == b"SECRETVALUE"),
            "the remote holds ciphertext, not the note"
        );
    }

    #[test]
    fn a_pull_folds_the_remote_copy_in_and_keeps_what_is_here() {
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", PASSWORD);

        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "my note", "from here");

        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        let report = pull(&mut vault, &plugin, "vault", PASSWORD).unwrap();

        assert_eq!(report.merged.added.len(), 1);
        assert_eq!(titles(&vault), vec!["my note", "their note"]);

        // And it is on disk, not only in memory.
        let reopened = Vault::open(&local, PASSWORD).unwrap();
        assert_eq!(titles(&reopened), vec!["my note", "their note"]);
    }

    #[test]
    fn a_pull_leaves_no_copy_of_the_remote_vault_behind() {
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", PASSWORD);
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        let log = directory.path().join("log");
        file_transport(directory.path(), &remote, &log);
        let plugin = installed(directory.path(), "file");

        pull(&mut vault, &plugin, "vault", PASSWORD).unwrap();

        // Not a sweep of the temp directory: tests run in parallel and would
        // see each other's scratch. This is the exact path the transport was
        // handed, which is the only one this pull is answerable for.
        let fetched = fetched_path(&log);
        assert!(
            !fetched.exists(),
            "the fetched copy is still at {}",
            fetched.display()
        );
        assert!(
            !fetched.parent().unwrap().exists(),
            "the scratch directory outlived the pull"
        );
    }

    #[test]
    fn a_pull_under_the_wrong_password_changes_nothing_here() {
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", b"another password");
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "my note", "from here");
        let log = directory.path().join("log");
        file_transport(directory.path(), &remote, &log);
        let plugin = installed(directory.path(), "file");

        let error = pull(&mut vault, &plugin, "vault", PASSWORD).unwrap_err();

        assert!(matches!(error, Error::WrongPasswordOrNotAVault));
        assert_eq!(titles(&vault), vec!["my note"]);
        // The failure path has to clean up as thoroughly as the happy one: a
        // copy of somebody's vault left in the temp directory is exactly what
        // this product exists not to leave lying around.
        let fetched = fetched_path(&log);
        assert!(
            !fetched.exists(),
            "a failed pull left the fetched copy at {}",
            fetched.display()
        );
    }

    #[test]
    fn a_transport_reporting_success_without_a_file_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        silent_transport(directory.path(), r#"["push","pull"]"#);
        let plugin = installed(directory.path(), "silent");

        let error = pull(&mut vault, &plugin, "vault", PASSWORD).unwrap_err();

        // Not "wrong password or not a vault": that message would send someone
        // looking at their password when the transport is what went wrong.
        let message = error.to_string();
        assert!(message.contains("wrote no file"), "got: {message}");
    }

    #[test]
    fn a_sync_takes_the_other_side_first_and_publishes_the_result() {
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", PASSWORD);
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "my note", "from here");
        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        let report = sync(&mut vault, &plugin, "vault", PASSWORD).unwrap();

        assert_eq!(report.pulled.merged.added.len(), 1);

        // The copy now at the remote has to hold both sides. Pushing before
        // pulling would leave it holding only this machine's, and this is the
        // assertion that would catch it.
        let published = directory.path().join("published.bak");
        std::fs::copy(&remote, &published).unwrap();
        let published = Vault::open(&published, PASSWORD).unwrap();
        assert_eq!(titles(&published), vec!["my note", "their note"]);
    }

    #[test]
    fn a_transport_that_only_pushes_is_refused_a_pull_before_it_runs() {
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        silent_transport(directory.path(), r#"["push"]"#);
        let plugin = installed(directory.path(), "silent");

        let error = pull(&mut vault, &plugin, "vault", PASSWORD).unwrap_err();

        assert!(error.to_string().contains("pull"), "got: {error}");
    }

    #[test]
    fn a_push_stamps_the_vault_and_the_copy_that_goes_up() {
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("notes.bak");
        let remote = directory.path().join("remote.bin");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "bank", "vault code 1234");
        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        assert!(
            vault.last_sync().unwrap().is_none(),
            "a fresh vault has never synced"
        );

        push(&mut vault, &plugin, "vault").unwrap();

        let stamp = vault.last_sync().unwrap().expect("the push left a stamp");
        assert_eq!(stamp.transport, "file");
        assert_eq!(stamp.operation, "push");

        // The half that is easy to get wrong: the file at the remote has to
        // carry the stamp too. Written afterwards, the copy over there would
        // say it had never synced, and pulling it onto another machine would
        // bring that answer back as though it were true.
        let fetched = directory.path().join("fetched.bak");
        std::fs::copy(&remote, &fetched).unwrap();
        let published = Vault::open(&fetched, PASSWORD).unwrap();
        assert_eq!(
            published.last_sync().unwrap().map(|s| s.operation),
            Some("push".to_owned()),
            "the copy at the remote carries the stamp"
        );
    }

    #[test]
    fn a_pull_records_itself_on_disk_along_with_what_it_brought() {
        // Reopened from the file rather than read out of the live vault: a
        // stamp that only reached memory would pass every in-process check and
        // be gone by the next command.
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", PASSWORD);
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "my note", "from here");
        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        pull(&mut vault, &plugin, "vault", PASSWORD).unwrap();
        drop(vault);

        // Reopened from disk, which is what makes this test worth having.
        let reopened = Vault::open(&local, PASSWORD).unwrap();
        let stamp = reopened
            .last_sync()
            .unwrap()
            .expect("the pull left a stamp");
        assert_eq!(stamp.operation, "pull");
        assert_eq!(stamp.transport, "file");
        // And what the vault holds is the merged result: the stamp must not
        // cost the pull whatever it went to fetch.
        assert_eq!(titles(&reopened), vec!["my note", "their note"]);
    }

    #[test]
    fn a_sync_is_recorded_as_one_operation_not_as_its_last_leg() {
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", PASSWORD);
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "my note", "from here");
        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        sync(&mut vault, &plugin, "vault", PASSWORD).unwrap();

        // Not "push", although a push is how it ends: the user asked for a
        // sync and the record should say what they asked for.
        assert_eq!(
            vault.last_sync().unwrap().map(|s| s.operation),
            Some("sync".to_owned())
        );
    }

    #[test]
    fn a_failed_transfer_leaves_no_claim_that_it_happened() {
        // The stamp says "this vault reached a remote". A transport that
        // refused the operation did not, and a vault claiming otherwise would
        // send someone looking for their secrets somewhere they never went.
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        silent_transport(directory.path(), r#"["push"]"#);
        let plugin = installed(directory.path(), "silent");

        pull(&mut vault, &plugin, "vault", PASSWORD).unwrap_err();

        assert!(
            vault.last_sync().unwrap().is_none(),
            "a refused pull must not look like a sync that happened"
        );
    }

    #[test]
    fn a_previewed_pull_says_what_would_come_and_changes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", PASSWORD);
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "my note", "from here");
        let log = directory.path().join("log");
        file_transport(directory.path(), &remote, &log);
        let plugin = installed(directory.path(), "file");
        let before = std::fs::read(&local).unwrap();
        let remote_before = std::fs::read(&remote).unwrap();

        let report = preview_pull(&vault, &plugin, "vault", PASSWORD).unwrap();

        assert_eq!(report.merged.added.len(), 1);
        assert_eq!(report.merged.added[0].title, "their note");
        assert_eq!(
            std::fs::read(&local).unwrap(),
            before,
            "nothing written here"
        );
        assert_eq!(
            std::fs::read(&remote).unwrap(),
            remote_before,
            "nothing sent there"
        );
        assert_eq!(titles(&vault), vec!["my note"]);
        assert!(
            vault.last_sync().unwrap().is_none(),
            "a preview is not a transfer and must not be recorded as one"
        );
        assert!(!fetched_path(&log).exists(), "the fetched copy is gone");
    }

    #[test]
    fn a_previewed_sync_says_what_each_side_would_gain() {
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", PASSWORD);
        // The local vault starts as a copy of the remote one, so the two share
        // the note; then each side gains something of its own.
        let local = directory.path().join("notes.bak");
        std::fs::copy(&remote, &local).unwrap();
        let mut vault = Vault::open(&local, PASSWORD).unwrap();
        note(&mut vault, "only here", "from here");
        let mut theirs = Vault::open(&remote, PASSWORD).unwrap();
        note(&mut theirs, "only there", "from there");
        drop(theirs);
        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        let preview = preview_sync(&vault, &plugin, "vault", PASSWORD).unwrap();

        let coming: Vec<&str> = preview
            .coming
            .added
            .iter()
            .map(|item| item.title.as_str())
            .collect();
        let going: Vec<&str> = preview
            .going
            .added
            .iter()
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(coming, vec!["only there"]);
        assert_eq!(going, vec!["only here"]);
        assert!(preview.going.conflicts.is_empty(), "{:?}", preview.going);

        // And the real sync then does exactly that.
        let done = sync(&mut vault, &plugin, "vault", PASSWORD).unwrap();
        assert_eq!(done.pulled.merged.added.len(), 1);
        let published = directory.path().join("published.bak");
        std::fs::copy(&remote, &published).unwrap();
        let published = Vault::open(&published, PASSWORD).unwrap();
        assert_eq!(
            titles(&published),
            vec!["only here", "only there", "their note"]
        );
    }

    #[test]
    fn a_previewed_sync_of_two_copies_in_step_moves_nothing_either_way() {
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", PASSWORD);
        let local = directory.path().join("notes.bak");
        std::fs::copy(&remote, &local).unwrap();
        let vault = Vault::open(&local, PASSWORD).unwrap();
        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        let preview = preview_sync(&vault, &plugin, "vault", PASSWORD).unwrap();

        assert!(!preview.coming.changed(), "{:?}", preview.coming);
        assert!(!preview.going.changed(), "{:?}", preview.going);
    }

    #[test]
    fn a_pull_that_brings_something_keeps_the_vault_as_it_was_beside_it() {
        let directory = tempfile::tempdir().unwrap();
        let remote = remote_holding(directory.path(), "their note", PASSWORD);
        let local = directory.path().join("notes.bak");
        let mut vault = Vault::create(&local, PASSWORD).unwrap();
        note(&mut vault, "my note", "from here");
        file_transport(directory.path(), &remote, &directory.path().join("log"));
        let plugin = installed(directory.path(), "file");

        let report = pull(&mut vault, &plugin, "vault", PASSWORD).unwrap();

        let kept = report.merged.kept_copy.expect("a copy was kept");
        let before = Vault::open(&kept, PASSWORD).unwrap();
        assert_eq!(
            titles(&before),
            vec!["my note"],
            "the state before the pull"
        );
    }
}
