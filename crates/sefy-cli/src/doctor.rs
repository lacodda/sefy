//! `sefy doctor`: whether what sefy needs on this machine is in working order.
//!
//! Every check runs, whatever the ones before it found: the point is the whole
//! picture, and a doctor that stopped at the first problem would hide the
//! second one until the first was fixed. Nothing is changed and nothing is
//! sent — the transport check is a pull into a scratch file, which is the one
//! way to know a sync would work without doing one.

use crate::autosync::AutoSync;
use crate::cli::{RemoteArgs, Switch};
use crate::commands::{self, copies_line, size};
use crate::{device, output, session};
use anyhow::{Result, bail};
use sefy_core::{Plugin, Vault};
use std::path::{Path, PathBuf};

/// How a check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Works.
    Ok,
    /// Works, with something worth knowing.
    Warn,
    /// Does not work; sefy will fail at it.
    Fail,
    /// Nothing to check here, and that is not a problem.
    Skip,
}

impl Verdict {
    fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }
}

/// One line of the report.
struct Check {
    name: &'static str,
    verdict: Verdict,
    detail: String,
}

impl Check {
    fn new(name: &'static str, verdict: Verdict, detail: impl Into<String>) -> Self {
        Self {
            name,
            verdict,
            detail: detail.into(),
        }
    }
}

/// Runs every check and prints the report.
pub fn run(
    vault_path: Option<PathBuf>,
    password_env: Option<&str>,
    remote: RemoteArgs,
    auto_sync: Option<Switch>,
) -> Result<()> {
    let mut checks = Vec::new();

    let (vault, password) = match vault_path.as_deref() {
        Some(path) => open_vault(path, password_env, &mut checks),
        None => {
            checks.push(Check::new(
                "vault",
                Verdict::Skip,
                "no vault given; pass --vault <FILE> or set SEFY_VAULT",
            ));
            (None, None)
        }
    };
    if let Some(path) = vault_path.as_deref() {
        checks.push(copies(path, password.as_deref()));
    }
    checks.push(device());

    let installed = sefy_core::plugin::discover();
    checks.push(plugins(&installed));
    let transport = transport(&installed, &remote, vault.as_ref(), password.as_deref());
    let reachable = transport.verdict == Verdict::Ok || transport.verdict == Verdict::Warn;
    checks.push(transport);
    checks.push(auto_sync_check(auto_sync, &remote, reachable));
    checks.push(clipboard());

    let name_width = checks
        .iter()
        .map(|check| check.name.len())
        .max()
        .unwrap_or(0);
    for check in &checks {
        let mut lines = check.detail.lines();
        println!(
            "{:<5} {:<name_width$}  {}",
            check.verdict.label(),
            check.name,
            lines.next().unwrap_or_default()
        );
        // Further lines are what to do about it, under the detail they belong to.
        for line in lines {
            println!("{:<5} {:<name_width$}  {line}", "", "");
        }
    }

    let failed = checks
        .iter()
        .filter(|check| check.verdict == Verdict::Fail)
        .count();
    if failed > 0 {
        bail!(
            "{} of {} failed",
            output::count(failed, "check"),
            checks.len()
        );
    }
    Ok(())
}

/// Opens the vault, reporting how that went, and hands it back if it opened.
fn open_vault(
    path: &Path,
    password_env: Option<&str>,
    checks: &mut Vec<Check>,
) -> (Option<Vault>, Option<String>) {
    if !path.is_file() {
        checks.push(Check::new(
            "vault",
            Verdict::Fail,
            format!("{} is not there", path.display()),
        ));
        return (None, None);
    }

    let password = match session::password(password_env) {
        Ok(password) => password,
        Err(error) => {
            checks.push(Check::new("vault", Verdict::Fail, format!("{error:#}")));
            return (None, None);
        }
    };

    let vault = match session::open(path, &password) {
        Ok(vault) => vault,
        Err(error) => {
            checks.push(Check::new("vault", Verdict::Fail, format!("{error:#}")));
            return (None, None);
        }
    };

    let (verdict, detail) = match vault.stats() {
        Ok(stats) if stats.schema > sefy_core::db::SCHEMA_VERSION => (
            Verdict::Warn,
            format!(
                "{} opens, but a newer sefy wrote it (schema {})\n\
                 update sefy before changing it here",
                path.display(),
                stats.schema
            ),
        ),
        Ok(stats) => (
            Verdict::Ok,
            format!(
                "{} opens: {}, schema {}",
                path.display(),
                output::count(stats.items, "item"),
                stats.schema
            ),
        ),
        Err(error) => (
            Verdict::Fail,
            format!("{} opens but cannot be read: {error}", path.display()),
        ),
    };
    checks.push(Check::new("vault", verdict, detail));

    // A write that was cut short leaves its temporary file behind. It holds
    // ciphertext and the next save replaces it, so it is worth a mention and
    // nothing more.
    let mut leftover = path.as_os_str().to_owned();
    leftover.push(".sefy-tmp");
    if Path::new(&leftover).exists() {
        checks.push(Check::new(
            "leftover",
            Verdict::Warn,
            format!(
                "{} is left from a write that was cut short\n\
                 it is sealed like the vault; the next change replaces it",
                Path::new(&leftover).display()
            ),
        ));
    }

    (Some(vault), Some(password))
}

/// Whether the copies beside the vault are there and would open if needed.
///
/// A copy that does not open under the vault's password is a safety net with
/// a hole in it, and this is the moment to find out — not the day it is
/// needed.
fn copies(vault: &Path, password: Option<&str>) -> Check {
    let copies = sefy_core::copies::list(vault);
    let summary = copies_line(vault);
    let Some(password) = password else {
        return Check::new("copies", Verdict::Ok, summary);
    };

    let unopened: Vec<String> = copies
        .iter()
        .filter(|copy| Vault::open(&copy.path, password.as_bytes()).is_err())
        .map(|copy| copy.path.display().to_string())
        .collect();
    if unopened.is_empty() {
        return Check::new("copies", Verdict::Ok, summary);
    }
    Check::new(
        "copies",
        Verdict::Warn,
        format!(
            "{summary}\n{} not open under this vault's password: {}",
            if unopened.len() == 1 { "does" } else { "do" },
            unopened.join(", ")
        ),
    )
}

fn device() -> Check {
    match device::name() {
        Some(name) => Check::new(
            "device",
            Verdict::Ok,
            format!("versions written here are marked {name:?}"),
        ),
        None => Check::new(
            "device",
            Verdict::Warn,
            "this machine has no name to mark versions with\n\
             a conflict will not be able to say which machine a version came from",
        ),
    }
}

fn plugins(installed: &[Plugin]) -> Check {
    if installed.is_empty() {
        return Check::new(
            "plugins",
            Verdict::Skip,
            "none installed; push, pull and sync need a transport",
        );
    }

    let names: Vec<String> = installed
        .iter()
        .map(|plugin| match &plugin.manifest {
            Some(manifest) => format!("{} {}", plugin.name(), manifest.version),
            None => plugin.name().to_owned(),
        })
        .collect();
    let broken: Vec<String> = installed
        .iter()
        .filter(|plugin| !plugin.usable)
        .map(|plugin| {
            format!(
                "{}: {}",
                plugin.name(),
                plugin.reason.as_deref().unwrap_or("unusable")
            )
        })
        .collect();

    if broken.is_empty() {
        Check::new("plugins", Verdict::Ok, names.join(", "))
    } else {
        Check::new(
            "plugins",
            Verdict::Warn,
            format!("{}\n{}", names.join(", "), broken.join("\n")),
        )
    }
}

/// Whether the transport sync would use reaches the remote copy, and whether
/// that copy opens and is in step with this one.
fn transport(
    installed: &[Plugin],
    remote: &RemoteArgs,
    vault: Option<&Vault>,
    password: Option<&str>,
) -> Check {
    if installed.is_empty() {
        return Check::new("transport", Verdict::Skip, "no transport to check");
    }

    let plugin = match commands::transport(remote.transport.as_deref()) {
        Ok(plugin) => plugin,
        Err(error) => return Check::new("transport", Verdict::Fail, format!("{error:#}")),
    };
    if !plugin.supports(sefy_core::plugin::Operation::Pull) {
        return Check::new(
            "transport",
            Verdict::Skip,
            format!(
                "{} only pushes; checking it would replace the remote copy",
                plugin.name()
            ),
        );
    }

    let fetched = match sefy_core::fetch(&plugin, &remote.name) {
        Ok(fetched) => fetched,
        Err(error) => return Check::new("transport", Verdict::Fail, error.to_string()),
    };
    let reached = format!(
        "{} fetched {:?} ({})",
        plugin.name(),
        remote.name,
        size(fetched.sealed.len() as u64)
    );

    let (Some(vault), Some(password)) = (vault, password) else {
        return Check::new("transport", Verdict::Ok, reached);
    };
    let theirs = match fetched.open(password.as_bytes()) {
        Ok(theirs) => theirs,
        Err(sefy_core::Error::WrongPasswordOrNotAVault) => {
            return Check::new(
                "transport",
                Verdict::Warn,
                format!(
                    "{reached}, which does not open under this vault's password\n\
                     pull and sync need --ask-remote-password; syncing after a write will fail"
                ),
            );
        }
        Err(error) => {
            return Check::new("transport", Verdict::Fail, format!("{reached}: {error}"));
        }
    };

    match sefy_core::sync::both_ways(vault, theirs) {
        Ok((coming, going)) if !coming.changed() && !going.changed() => Check::new(
            "transport",
            Verdict::Ok,
            format!("{reached}; it opens and is in step with this vault"),
        ),
        Ok((coming, going)) => Check::new(
            "transport",
            Verdict::Ok,
            format!(
                "{reached}; it opens, and a sync would change {} here and {} there\n\
                 see which with `sefy sync --dry-run`",
                output::count(changes(&coming), "item"),
                output::count(changes(&going), "item")
            ),
        ),
        Err(error) => Check::new(
            "transport",
            Verdict::Fail,
            format!("{reached}, but comparing it failed: {error}"),
        ),
    }
}

/// How many items a merge adds, updates or settles a conflict on.
fn changes(report: &sefy_core::MergeReport) -> usize {
    report.added.len() + report.updated.len() + report.conflicts.len()
}

fn auto_sync_check(switch: Option<Switch>, remote: &RemoteArgs, reachable: bool) -> Check {
    if !AutoSync::new(switch, "").is_on() {
        return Check::new(
            "auto-sync",
            Verdict::Skip,
            "off; --auto-sync on or SEFY_AUTO_SYNC=on syncs after every change",
        );
    }
    // The transport check above picked its transport by the rule `sefy sync`
    // uses with no options — the rule auto-sync uses too — so its answer is
    // this one's.
    if !reachable {
        return Check::new(
            "auto-sync",
            Verdict::Fail,
            "on, but the transport check above did not pass; every change will warn",
        );
    }
    Check::new(
        "auto-sync",
        Verdict::Ok,
        format!("on: every change here is synced as {:?}", remote.name),
    )
}

/// Whether the clipboard can be reached, without putting anything on it.
///
/// A test value written and taken back would land in the clipboard history
/// some desktops keep, so this only opens the clipboard and reads it — the
/// two things that fail on a machine with no clipboard at all.
fn clipboard() -> Check {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(error) => {
            return Check::new(
                "clipboard",
                Verdict::Fail,
                format!(
                    "cannot reach it: {error}\nget, otp and gen can print with --stdout instead"
                ),
            );
        }
    };
    match clipboard.get_text() {
        Ok(_) | Err(arboard::Error::ContentNotAvailable) => {
            Check::new("clipboard", Verdict::Ok, "reachable")
        }
        Err(error) => Check::new(
            "clipboard",
            Verdict::Fail,
            format!("reachable, but cannot be read: {error}"),
        ),
    }
}
