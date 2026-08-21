//! What each subcommand does once the vault is open.

use crate::cli::{AddKind, EditArgs, Field, FindArgs, GetArgs, ListArgs, PullArgs, RemoteArgs};
use crate::output;
use crate::session;
use anyhow::{Context, Result, bail};
use sefy_core::{Credential, NewItem, Payload, Query, Vault};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Creates a vault, refusing to touch a file that already exists.
pub fn init(path: &Path, password_env: Option<&str>) -> Result<()> {
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    let password = session::new_password(password_env)?;
    Vault::create(path, password.as_bytes())?;
    println!("created {}", path.display());
    Ok(())
}

/// Adds an item and saves the vault.
pub fn add(vault: &mut Vault, kind: AddKind) -> Result<()> {
    let (item, description) = match kind {
        AddKind::Note {
            title,
            text,
            editor,
            tag,
        } => {
            let text = match text {
                Some(text) => text,
                None if editor => crate::editor::edit("")?,
                None => read_stdin().context("cannot read the note text from stdin")?,
            };
            (
                NewItem::new(title.clone(), Payload::Note { text }).with_tags(tag),
                title,
            )
        }
        AddKind::Credential {
            title,
            login,
            url,
            totp,
            notes,
            item_password_env,
            tag,
        } => {
            // Only this item's own variable is consulted: falling back to
            // --password-env would silently store the master password as the
            // account's password.
            let password =
                session::secret("Password for this item: ", item_password_env.as_deref())?;
            (
                NewItem::new(
                    title.clone(),
                    Payload::Credential(Credential {
                        login,
                        password,
                        url,
                        totp,
                        notes,
                    }),
                )
                .with_tags(tag),
                title,
            )
        }
        AddKind::File { path, title, tag } => {
            let bytes =
                std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
            let filename = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_owned());
            let title = title.unwrap_or_else(|| filename.clone());
            (
                NewItem::new(title.clone(), Payload::File { filename, bytes }).with_tags(tag),
                title,
            )
        }
    };

    let id = vault.add(item)?;
    vault.save()?;
    println!("added {description:?} as {id}");
    Ok(())
}

/// Copies a secret to the clipboard, or prints it when asked to.
pub fn get(vault: &Vault, args: GetArgs) -> Result<()> {
    let summary = vault.resolve(&args.reference).map_err(output::explain)?;
    let item = vault.get(summary.id)?;

    let (value, description) = match &item.payload {
        Payload::Note { text } => (text.clone(), "text".to_owned()),
        Payload::Credential(credential) => {
            let value = match args.field {
                Field::Password => Some(credential.password.clone()),
                Field::Login => Some(credential.login.clone()),
                Field::Url => credential.url.clone(),
                Field::Totp => credential.totp.clone(),
            };
            match value {
                Some(value) => (value, args.field.as_str().to_owned()),
                None => bail!("{:?} has no {}", item.summary.title, args.field.as_str()),
            }
        }
        Payload::File { .. } => bail!(
            "{:?} is a file; write it to disk with: sefy extract {}",
            item.summary.title,
            item.summary.id
        ),
    };

    if args.stdout {
        println!("{value}");
        return Ok(());
    }

    if args.clear_after > 0 {
        // Said before the wait, not after: the user needs to know why the
        // command is sitting there.
        println!(
            "copied {description} of {:?} to the clipboard; clearing in {}s",
            item.summary.title, args.clear_after
        );
    } else {
        println!(
            "copied {description} of {:?} to the clipboard",
            item.summary.title
        );
    }
    flush_stdout();

    let hold = output::to_clipboard(&value, args.clear_after)?;
    if hold.cleared {
        println!("clipboard cleared");
    }
    Ok(())
}

/// Makes sure a message is on screen before a wait, not buffered behind it.
fn flush_stdout() {
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

/// Shows an item's fields, keeping its secrets hidden.
pub fn show(vault: &Vault, reference: &str) -> Result<()> {
    let summary = vault.resolve(reference).map_err(output::explain)?;
    let item = vault.get(summary.id)?;

    field("id", &item.summary.id.to_string());
    field("title", &item.summary.title);
    field("kind", item.summary.kind.as_str());
    if !item.summary.tags.is_empty() {
        field("tags", &item.summary.tags.join(", "));
    }

    match &item.payload {
        Payload::Note { text } => {
            println!("---");
            println!("{text}");
        }
        Payload::Credential(credential) => {
            field("login", &credential.login);
            // Never printed here; `sefy get` is the one way a secret leaves.
            field("password", "<hidden — use sefy get>");
            if let Some(url) = &credential.url {
                field("url", url);
            }
            if credential.totp.is_some() {
                field("totp", "<hidden — use sefy get --field totp>");
            }
            if let Some(notes) = &credential.notes {
                field("notes", notes);
            }
        }
        Payload::File { filename, bytes } => {
            field("file", filename);
            field("size", &format!("{} bytes", bytes.len()));
        }
    }
    Ok(())
}

/// Prints one labelled line of `sefy show`, aligned to a fixed column.
///
/// A single place to pad from: hand-spaced labels drift the moment one of them
/// is longer than the rest, and `password` already had. The width leaves a
/// space after the longest label rather than butting against it.
fn field(label: &str, value: &str) {
    const LABEL_WIDTH: usize = "password:".len() + 1;
    println!("{:<LABEL_WIDTH$}{value}", format!("{label}:"));
}

/// Lists items, optionally narrowed by kind and tags.
pub fn ls(vault: &Vault, args: ListArgs) -> Result<()> {
    let mut query = Query::all().tags(args.tag);
    if let Some(kind) = args.kind {
        query = query.kind(kind.into());
    }
    output::table(&vault.search(&query)?);
    Ok(())
}

/// Searches items by text, kind and tags.
pub fn find(vault: &Vault, args: FindArgs) -> Result<()> {
    let mut query = Query::all().tags(args.tag);
    if let Some(text) = args.text {
        query = query.text(text);
    }
    if let Some(kind) = args.kind {
        query = query.kind(kind.into());
    }
    output::table(&vault.search(&query)?);
    Ok(())
}

/// Changes an item's title, contents or tags.
pub fn edit(vault: &mut Vault, args: EditArgs) -> Result<()> {
    let summary = vault.resolve(&args.reference).map_err(output::explain)?;
    let existing = vault.get(summary.id)?;

    let payload = build_edited_payload(&existing.payload, &args)?;
    let tags = if args.clear_tags {
        Some(Vec::new())
    } else if args.tag.is_empty() {
        None
    } else {
        Some(args.tag.clone())
    };

    if args.title.is_none() && payload.is_none() && tags.is_none() {
        bail!("nothing to change; pass --title, --tag, or a field to edit");
    }

    vault.update(summary.id, args.title, payload, tags)?;
    vault.save()?;
    println!("updated {}", summary.id);
    Ok(())
}

/// Applies the edit flags to an item's current payload.
///
/// Returns `None` when no flag touches the payload, so the item keeps what it
/// has. Flags meant for another kind of item are an error rather than a silent
/// no-op.
fn build_edited_payload(existing: &Payload, args: &EditArgs) -> Result<Option<Payload>> {
    let wants_new_password = args.password || args.item_password_env.is_some();
    let wants_credential_field = args.login.is_some()
        || wants_new_password
        || args.url.is_some()
        || args.totp.is_some()
        || args.notes.is_some();

    match existing {
        Payload::Note { text: current } => {
            if wants_credential_field {
                bail!(
                    "this item is a note; --login, --password, --url, --totp and --notes \
                     apply to credentials"
                );
            }
            if args.editor {
                return Ok(Some(Payload::Note {
                    text: crate::editor::edit(current)?,
                }));
            }
            Ok(args.text.clone().map(|text| Payload::Note { text }))
        }
        Payload::Credential(credential) => {
            if args.text.is_some() || args.editor {
                bail!("this item is a credential; --text and --editor apply to notes");
            }
            if !wants_credential_field {
                return Ok(None);
            }

            let mut updated = credential.clone();
            if let Some(login) = args.login.clone() {
                updated.login = login;
            }
            if wants_new_password {
                updated.password = session::secret(
                    "New password for this item: ",
                    args.item_password_env.as_deref(),
                )?;
            }
            if let Some(url) = args.url.clone() {
                updated.url = Some(url);
            }
            if let Some(totp) = args.totp.clone() {
                updated.totp = Some(totp);
            }
            if let Some(notes) = args.notes.clone() {
                updated.notes = Some(notes);
            }
            Ok(Some(Payload::Credential(updated)))
        }
        Payload::File { .. } => {
            if args.text.is_some() || args.editor || wants_credential_field {
                bail!("this item is a file; only --title and tags can be edited");
            }
            Ok(None)
        }
    }
}

/// Removes an item, asking first unless told not to.
pub fn rm(vault: &mut Vault, reference: &str, yes: bool) -> Result<()> {
    let summary = vault.resolve(reference).map_err(output::explain)?;

    if !yes
        && !confirm(&format!(
            "remove {:?} ({})? [y/N] ",
            summary.title, summary.id
        ))?
    {
        println!("kept");
        return Ok(());
    }

    vault.remove(summary.id)?;
    vault.save()?;
    println!("removed {}", summary.id);
    Ok(())
}

/// Writes a stored file back to disk.
pub fn extract(
    vault: &Vault,
    reference: &str,
    output_path: Option<PathBuf>,
    force: bool,
) -> Result<()> {
    let summary = vault.resolve(reference).map_err(output::explain)?;
    let item = vault.get(summary.id)?;

    let Payload::File { filename, bytes } = &item.payload else {
        bail!(
            "{:?} is a {}, not a file",
            item.summary.title,
            item.summary.kind
        );
    };

    let destination = output_path.unwrap_or_else(|| PathBuf::from(filename));
    if destination.exists() && !force {
        bail!(
            "{} already exists; pass --force to overwrite",
            destination.display()
        );
    }

    std::fs::write(&destination, bytes)
        .with_context(|| format!("cannot write {}", destination.display()))?;
    println!("wrote {} ({} bytes)", destination.display(), bytes.len());
    Ok(())
}

/// Lists the tags in use with their item counts.
pub fn tags(vault: &Vault) -> Result<()> {
    let tags = vault.tags()?;
    if tags.is_empty() {
        println!("no tags");
        return Ok(());
    }
    let width = tags
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(4);
    for (name, count) in tags {
        println!("{name:<width$}  {count}");
    }
    Ok(())
}

/// Writes the vault's contents out as plain JSON.
pub fn export(
    vault: &Vault,
    output_path: Option<PathBuf>,
    acknowledged: bool,
    force: bool,
) -> Result<()> {
    if !acknowledged {
        bail!(
            "export writes every secret in this vault in the clear\n\
             the resulting file protects nothing — encrypt it, or delete it when done\n\
             pass --i-know-this-writes-plaintext to go ahead"
        );
    }

    let json = sefy_core::exchange::to_json(&sefy_core::exchange::export(vault)?)?;

    match output_path {
        Some(path) => {
            if path.exists() && !force {
                bail!(
                    "{} already exists; pass --force to overwrite",
                    path.display()
                );
            }
            std::fs::write(&path, json.as_bytes())
                .with_context(|| format!("cannot write {}", path.display()))?;
            eprintln!("wrote {} in the clear", path.display());
        }
        None => println!("{json}"),
    }
    Ok(())
}

/// Adds the contents of an export to the vault.
pub fn import(vault: &mut Vault, input: Option<PathBuf>) -> Result<()> {
    let json = match input {
        Some(path) => std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?,
        None => read_stdin().context("cannot read the export from stdin")?,
    };

    let export = sefy_core::exchange::from_json(&json)?;
    let report = sefy_core::exchange::import(vault, &export)?;

    println!("imported {}", output::count(report.added, "item"));
    if report.skipped > 0 {
        // Silence here would read as "imported nothing" on a re-import, when
        // what actually happened is that the vault already had it all.
        println!(
            "{} already here, left alone",
            output::count(report.skipped, "item")
        );
    }
    Ok(())
}

/// Folds another vault file into this one.
pub fn merge(vault: &mut Vault, other: &Path, other_password_env: Option<&str>) -> Result<()> {
    if other == vault.path() {
        bail!("that is this vault; merging a file into itself would do nothing");
    }

    // Asked for separately: a copy from another machine may be under a
    // different password, and assuming otherwise would just fail confusingly.
    let password = session::secret(
        &format!("Password for {}: ", other.display()),
        other_password_env,
    )?;
    let source = session::open(other, &password)?;

    let report = sefy_core::merge(vault, &source)?;

    report_merge(&report, "nothing to merge; the two vaults already agree");
    Ok(())
}

/// Replaces the master password and rewrites the file under it.
pub fn change_password(vault: &mut Vault, password_env: Option<&str>) -> Result<()> {
    let password = session::new_password(password_env)?;
    vault.change_password(password.as_bytes())?;
    println!("password changed");
    Ok(())
}

/// Sends the vault file to the remote.
pub fn push(vault: &Vault, args: RemoteArgs) -> Result<()> {
    let plugin = transport(args.transport.as_deref())?;
    let report = sefy_core::push(vault, &plugin, &args.name)?;

    println!("pushed {:?} through {}", args.name, plugin.name());
    if let Some(message) = report.message {
        println!("{message}");
    }
    Ok(())
}

/// Fetches the remote copy and folds it in.
pub fn pull(vault: &mut Vault, args: PullArgs, master: &str) -> Result<()> {
    let plugin = transport(args.remote.transport.as_deref())?;
    let remote_password = remote_password(&args, master)?;

    let report = sefy_core::pull(
        vault,
        &plugin,
        &args.remote.name,
        remote_password.as_bytes(),
    )?;

    println!("pulled {:?} through {}", args.remote.name, plugin.name());
    if let Some(message) = &report.transport.message {
        println!("{message}");
    }
    report_merge(
        &report.merged,
        "nothing came back that this vault did not already have",
    );
    Ok(())
}

/// Pulls, then pushes the result back.
pub fn sync(vault: &mut Vault, args: PullArgs, master: &str) -> Result<()> {
    let plugin = transport(args.remote.transport.as_deref())?;
    let remote_password = remote_password(&args, master)?;

    let report = sefy_core::sync(
        vault,
        &plugin,
        &args.remote.name,
        remote_password.as_bytes(),
    )?;

    println!("synced {:?} through {}", args.remote.name, plugin.name());
    for message in [
        report.pulled.transport.message.as_deref(),
        report.pushed.message.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        println!("{message}");
    }
    report_merge(
        &report.pulled.merged,
        "nothing came back that this vault did not already have",
    );
    Ok(())
}

/// The master password of the copy on the other side.
///
/// A pull brings back a copy of *this* vault, so the same password is the
/// ordinary case and the default. Both ways of saying otherwise are honoured.
fn remote_password(args: &PullArgs, master: &str) -> Result<String> {
    if let Some(variable) = args.remote_password_env.as_deref() {
        return session::secret("", Some(variable));
    }
    if args.ask_remote_password {
        return session::secret("Password of the remote copy: ", None);
    }
    Ok(master.to_owned())
}

/// Picks the transport to use.
///
/// Named, or the only one installed. Guessing between several would mean
/// choosing where somebody's vault goes, and a wrong guess there is not a
/// mistake that announces itself.
fn transport(name: Option<&str>) -> Result<sefy_core::Plugin> {
    let installed = sefy_core::plugin::discover();

    if let Some(name) = name {
        let found = installed
            .into_iter()
            .find(|plugin| plugin.name() == name || plugin.executable == name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no transport called {name:?}\n\
                     run `sefy plugin list` to see what is installed."
                )
            })?;
        return Ok(found);
    }

    let usable: Vec<sefy_core::Plugin> = installed
        .into_iter()
        .filter(|plugin| plugin.usable)
        .collect();

    match usable.len() {
        0 => bail!(
            "no usable transport installed\n\
             a transport is an executable named {}<name>; `sefy plugin list` \
             says where sefy looks and why anything found is unusable.",
            sefy_core::plugin::PREFIX
        ),
        1 => Ok(usable.into_iter().next().expect("just counted")),
        _ => {
            let names: Vec<&str> = usable.iter().map(sefy_core::Plugin::name).collect();
            bail!(
                "several transports are installed: {}\n\
                 say which one with --transport <NAME>.",
                names.join(", ")
            )
        }
    }
}

/// Prints what a merge did.
///
/// Shared by `merge`, `pull` and `sync`: the outcome is the same thing in all
/// three, and a conflict has to read the same way whichever brought it in.
/// Only the line for "the two already agree" differs, since what the user did
/// differs.
fn report_merge(report: &sefy_core::MergeReport, nothing_to_do: &str) {
    if report.is_empty() {
        println!("{nothing_to_do}");
        return;
    }

    println!(
        "merged: {} added, {} updated, {} unchanged",
        report.added, report.updated, report.unchanged
    );

    if !report.conflicts.is_empty() {
        // Loud on purpose, exactly as in `merge`: a conflict means two versions
        // of one secret now sit in the vault, and only the person who made them
        // can say which is right.
        println!(
            "\n{} changed on both sides and could not be resolved here.",
            output::count(report.conflicts.len(), "item")
        );
        println!("This vault's version was kept; the incoming one is beside it:");
        for conflict in &report.conflicts {
            println!(
                "  {:?} → also kept as {:?}",
                conflict.title, conflict.kept_as
            );
        }
        println!("Compare them, keep the right one, and remove the other.");
    }
}

/// Lists the transports installed on this machine.
///
/// Everything found is shown, usable or not. A plugin that is present but
/// broken looks exactly like one that was never installed if it is left out —
/// and the two call for opposite fixes.
pub fn plugin_list(show_paths: bool) -> Result<()> {
    let paths = sefy_core::plugin::search_paths();
    let plugins = sefy_core::plugin::discover_in(&paths);

    if show_paths {
        println!("looked in:");
        for path in &paths {
            println!("  {}", path.display());
        }
        println!();
    }

    if plugins.is_empty() {
        println!("no plugins installed");
        println!(
            "a plugin is an executable named {}<name>, on PATH or in {}",
            sefy_core::plugin::PREFIX,
            match sefy_core::plugin::plugin_directory() {
                Some(directory) => directory.display().to_string(),
                None => "sefy's data directory".to_owned(),
            }
        );
        return Ok(());
    }

    let name_width = plugins
        .iter()
        .map(|plugin| plugin.name().chars().count())
        .max()
        .unwrap_or(4);

    for plugin in &plugins {
        let version = plugin
            .manifest
            .as_ref()
            .map_or("?", |manifest| manifest.version.as_str());

        let state = if plugin.usable {
            let mut operations: Vec<&str> = plugin
                .manifest
                .iter()
                .flat_map(|manifest| &manifest.operations)
                .map(|operation| operation.as_str())
                .collect();
            operations.sort_unstable();
            operations.join(", ")
        } else {
            format!(
                "unusable: {}",
                plugin.reason.as_deref().unwrap_or("no reason given")
            )
        };

        println!(
            "{:<name_width$}  {:<8}  {}",
            plugin.name(),
            version,
            state,
            name_width = name_width
        );
    }

    Ok(())
}

/// Reads everything from stdin as text.
fn read_stdin() -> Result<String> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    Ok(text.trim_end_matches(['\n', '\r']).to_owned())
}

/// Asks a yes/no question on the terminal.
fn confirm(question: &str) -> Result<bool> {
    use std::io::{IsTerminal, Write};

    if !std::io::stdin().is_terminal() {
        bail!("cannot ask for confirmation: input is not a terminal; pass --yes");
    }

    print!("{question}");
    std::io::stdout().flush()?;

    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
