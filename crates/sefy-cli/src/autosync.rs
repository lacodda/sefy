//! Syncing after a write, for a machine that is one of several.
//!
//! Turned on with `--auto-sync on` or `SEFY_AUTO_SYNC=on`. After a command
//! changes the vault on disk, sefy does what `sefy sync` would do here with no
//! options: the same transport, the same remote name, pull then push.
//!
//! Two rules shape everything below.
//!
//! **The change comes first.** By the time a sync starts, the command's own
//! write is on disk. A sync that fails — no network, a remote under another
//! password — is reported and does not fail the command: the change is made,
//! and a non-zero status would invite a script to make it twice. The next sync
//! carries it up.
//!
//! **It speaks on stderr.** `sefy gen --save --stdout` hands a password to a
//! pipe, and a line about a transfer appended to it would corrupt it.

use crate::cli::{DEFAULT_REMOTE_NAME, REMOTE_NAME_ENV, Switch, TRANSPORT_ENV};
use crate::commands;
use sefy_core::Vault;
use zeroize::Zeroizing;

/// Whether to sync after a write, and what has been synced already.
pub struct AutoSync {
    on: bool,
    /// The password the remote copy is opened with: the one this vault was
    /// opened with. After `change-password` that is the *old* one, which is
    /// exactly what the remote copy is still sealed under until this sync
    /// replaces it.
    remote_password: Zeroizing<String>,
    /// The vault's write count when it was last synced, or when it was opened.
    synced_at: u64,
}

impl AutoSync {
    /// Syncing after writes as the command line or the environment asked.
    pub fn new(switch: Option<Switch>, password: &str) -> Self {
        Self {
            on: switch == Some(Switch::On),
            remote_password: Zeroizing::new(password.to_owned()),
            synced_at: 0,
        }
    }

    /// Syncing after writes, off: for transfers, which are syncs already.
    pub fn off() -> Self {
        Self::new(Some(Switch::Off), "")
    }

    /// Whether syncing after writes was asked for.
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Syncs the vault if it has been written since it was opened or last
    /// synced. Never fails: what went wrong is said, and the write stands.
    pub fn catch_up(&mut self, vault: &mut Vault) {
        if !self.on || vault.writes() == self.synced_at {
            return;
        }

        let outcome = self.sync(vault);
        // Counted after the sync, whose own stamp is a write too: otherwise
        // the next check would mistake that for a change still to send.
        self.synced_at = vault.writes();

        match outcome {
            Ok(lines) => {
                for line in lines {
                    eprintln!("{line}");
                }
            }
            Err(error) => {
                eprintln!(
                    "warning: the change is saved here but did not reach the remote: {error:#}\n\
                     it goes up with the next `sefy sync`."
                );
            }
        }
    }

    fn sync(&self, vault: &mut Vault) -> anyhow::Result<Vec<String>> {
        let plugin = commands::transport(remote_setting(TRANSPORT_ENV).as_deref())?;
        let name = remote_setting(REMOTE_NAME_ENV).unwrap_or_else(|| DEFAULT_REMOTE_NAME.into());

        let report = match sefy_core::sync(vault, &plugin, &name, self.remote_password.as_bytes()) {
            Ok(report) => report,
            Err(sefy_core::Error::WrongPasswordOrNotAVault) => anyhow::bail!(
                "the remote copy does not open under this vault's password\n\
                 sync it once with `sefy sync --ask-remote-password`"
            ),
            Err(other) => return Err(other.into()),
        };

        let mut lines = vec![format!("synced {name:?} through {}", plugin.name())];
        if report.pulled.merged.changed() || report.pulled.merged.unsupported > 0 {
            lines.extend(commands::merge_lines(
                &report.pulled.merged,
                commands::NOTHING_CAME_BACK,
            ));
        }
        Ok(lines)
    }
}

/// A transfer setting from the environment, as `sefy sync` reads it: an
/// empty variable is no setting at all.
fn remote_setting(variable: &str) -> Option<String> {
    std::env::var(variable)
        .ok()
        .filter(|value| !value.is_empty())
}
