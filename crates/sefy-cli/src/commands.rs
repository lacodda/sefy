//! What each subcommand does once the vault is open.

use crate::cli::{AddKind, EditArgs, FindArgs, GetArgs, ListArgs, OpenArgs, PullArgs, RemoteArgs};
use crate::output;
use crate::session;
use anyhow::{Context, Result, bail};
use sefy_core::{Field, ItemKind, ItemSummary, NewItem, Payload, Query, Vault};
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
        AddKind::Login {
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
            let mut fields = vec![
                Field::public("login", login),
                Field::secret("password", password),
            ];
            push_optional(&mut fields, "url", url, false);
            push_optional(&mut fields, "totp", totp, true);
            push_optional(&mut fields, "notes", notes, false);
            (
                NewItem::new(title.clone(), Payload::fields(ItemKind::Login, fields))
                    .with_tags(tag),
                title,
            )
        }
        AddKind::Card {
            title,
            holder,
            expiry,
            notes,
            no_cvv,
            no_pin,
            number_env,
            cvv_env,
            pin_env,
            tag,
        } => {
            let number = session::secret("Card number: ", number_env.as_deref())?;
            let mut fields = vec![Field::secret("number", number)];
            push_optional(&mut fields, "holder", holder, false);
            push_optional(&mut fields, "expiry", expiry, false);
            if !no_cvv {
                fields.push(Field::secret(
                    "cvv",
                    session::secret("CVV: ", cvv_env.as_deref())?,
                ));
            }
            if !no_pin {
                fields.push(Field::secret(
                    "pin",
                    session::secret("PIN: ", pin_env.as_deref())?,
                ));
            }
            push_optional(&mut fields, "notes", notes, false);
            (
                NewItem::new(title.clone(), Payload::fields(ItemKind::Card, fields)).with_tags(tag),
                title,
            )
        }
        AddKind::SshKey {
            title,
            private_key,
            public_key,
            host,
            notes,
            no_passphrase,
            passphrase_env,
            tag,
        } => {
            let private = std::fs::read_to_string(&private_key)
                .with_context(|| format!("cannot read {}", private_key.display()))?;
            let mut fields = vec![Field::secret("private-key", private)];
            if !no_passphrase {
                fields.push(Field::secret(
                    "passphrase",
                    session::secret("Passphrase for the key: ", passphrase_env.as_deref())?,
                ));
            }
            // The conventional sibling is picked up when it is there, and its
            // absence is not an error: a key without its public half is still
            // worth storing, and the public half can be derived from it.
            let public_path = public_key.unwrap_or_else(|| sibling_public_key(&private_key));
            match std::fs::read_to_string(&public_path) {
                Ok(public) => fields.push(Field::public("public-key", public.trim())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("cannot read {}", public_path.display()));
                }
            }
            push_optional(&mut fields, "host", host, false);
            push_optional(&mut fields, "notes", notes, false);
            (
                NewItem::new(title.clone(), Payload::fields(ItemKind::SshKey, fields))
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
        Payload::Fields { kind, fields } => {
            let name = match args.field.clone() {
                Some(name) => name,
                // Nothing was named, so sefy takes what the kind is mostly
                // about. A record whose fields are all public has no such
                // field, and guessing at one would hand over the wrong value.
                None => kind
                    .template()
                    .and_then(|template| template.default_field())
                    .map(|field| field.name.to_owned())
                    .with_context(|| {
                        format!(
                            "{:?} has no default field; name one with --field",
                            item.summary.title
                        )
                    })?,
            };
            match fields.iter().find(|field| field.name == name) {
                Some(field) => (field.value.clone(), field.name.clone()),
                None => bail!(
                    "{:?} has no {name:?}; it holds: {}",
                    item.summary.title,
                    field_names(fields)
                ),
            }
        }
        Payload::File { .. } => bail!(
            "{:?} is a file; write it to disk with: sefy extract {}",
            item.summary.title,
            item.summary.id
        ),
        Payload::Unknown { kind } => bail!(
            "{:?} is a {kind}, which this version of sefy does not know
             it was written by a newer sefy — upgrade to read it
             (the item is safe: it is listed, exported and synced as it is)",
            item.summary.title
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
        Payload::Fields { fields, .. } => {
            for stored in fields {
                if stored.secret {
                    // Never printed here; `sefy get` is the one way a secret
                    // leaves, and it says which field to ask for by name.
                    field(
                        &stored.name,
                        &format!("<hidden — use sefy get --field {}>", stored.name),
                    );
                } else {
                    field(&stored.name, &stored.value);
                }
            }
        }
        Payload::File { filename, bytes } => {
            field("file", filename);
            field("size", &format!("{} bytes", bytes.len()));
        }
        Payload::Unknown { kind } => {
            println!("---");
            println!("This item is a {kind}, a kind this version of sefy does not know.");
            println!("A newer sefy wrote it. Upgrade to read its contents.");
            println!("Nothing is lost: the item keeps its place in this vault.");
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
    // Wide enough for the longest label any built-in template uses; a longer
    // one from a field the user named themselves simply pushes its own value
    // along rather than shifting every other line.
    const LABEL_WIDTH: usize = "private-key:".len() + 1;
    println!("{:<LABEL_WIDTH$}{value}", format!("{label}:"));
}

/// Adds a field when the user gave a value for it, and leaves it out when not.
///
/// An absent option stays absent rather than being stored empty: "this login
/// has no URL" and "its URL is the empty string" are different facts, and only
/// the first one is true.
fn push_optional(fields: &mut Vec<Field>, name: &str, value: Option<String>, secret: bool) {
    if let Some(value) = value {
        fields.push(if secret {
            Field::secret(name, value)
        } else {
            Field::public(name, value)
        });
    }
}

/// The path a public key conventionally sits at, beside its private half.
fn sibling_public_key(private_key: &Path) -> PathBuf {
    let mut name = private_key.as_os_str().to_os_string();
    name.push(".pub");
    PathBuf::from(name)
}

/// Names of a record's fields, for a message that says what is on offer.
fn field_names(fields: &[Field]) -> String {
    fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
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
    // Always a listing. `find` is what a script calls, and a command that
    // printed a table into a pipe but opened a menu on a terminal would be two
    // commands wearing one name. Browsing is `sefy` with no arguments.
    output::table(&vault.search(&query)?);
    Ok(())
}

/// Browses the vault: pick an item, then show it.
///
/// What `sefy` with no subcommand does. The command line is exact and
/// remembering is not - this is the way in for "it is in there somewhere",
/// and it is a separate command rather than a mode of `find` so that neither
/// has to ask where it is running.
pub fn browse(vault: &Vault) -> Result<()> {
    if !crate::picker::available() {
        bail!(
            "sefy with no command opens an interactive picker, and this is not a terminal\n\
             list items with: sefy ls"
        );
    }
    pick_from(vault, &vault.list()?)
}

/// Shows the picker over `items` and acts on what comes back.
///
/// Acting means `show`: the item without its secrets. Copying the password
/// straight to the clipboard would be the quicker path and the wrong default -
/// the picker is also how someone browses their own vault, and browsing should
/// not leave a secret on the clipboard of a machine they were only looking at.
pub fn pick_from(vault: &Vault, items: &[ItemSummary]) -> Result<()> {
    if items.is_empty() {
        println!("no items");
        return Ok(());
    }

    let Some(picked) = crate::picker::choose(items, "Item")? else {
        // Escape. Nothing to say - the user closed a menu.
        return Ok(());
    };

    show(vault, &picked.id.to_string())
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
    let touches_fields =
        !args.set.is_empty() || !args.set_secret.is_empty() || !args.unset.is_empty();

    match existing {
        Payload::Note { text: current } => {
            if touches_fields {
                bail!("this item is a note; --set, --set-secret and --unset apply to records");
            }
            if args.editor {
                return Ok(Some(Payload::Note {
                    text: crate::editor::edit(current)?,
                }));
            }
            Ok(args.text.clone().map(|text| Payload::Note { text }))
        }
        Payload::Fields { kind, fields } => {
            if args.text.is_some() || args.editor {
                bail!("this item is a {kind}; --text and --editor apply to notes");
            }
            if !touches_fields {
                return Ok(None);
            }
            Ok(Some(Payload::Fields {
                kind: kind.clone(),
                fields: edit_fields(kind, fields, args)?,
            }))
        }
        Payload::File { .. } => {
            if args.text.is_some() || args.editor || touches_fields {
                bail!("this item is a file; only --title and tags can be edited");
            }
            Ok(None)
        }
        // Title and tags live beside the contents, not inside them, so they can
        // still be changed — returning `None` here leaves the payload alone and
        // lets the caller apply them. Anything that would rewrite the contents
        // is refused: this build cannot read them and must not replace them.
        Payload::Unknown { kind } => {
            if args.text.is_some() || args.editor || touches_fields {
                bail!(
                    "this item is a {kind}, a kind this version of sefy does not know;
                     only --title and tags can be edited"
                );
            }
            Ok(None)
        }
    }
}

/// Applies `--set`, `--set-secret` and `--unset` to a record's fields.
///
/// A field keeps its place: an edit changes what a record says, not the order
/// it reads in. A field that is new lands at the end, unless the kind's
/// template has an opinion about where it belongs.
fn edit_fields(kind: &ItemKind, existing: &[Field], args: &EditArgs) -> Result<Vec<Field>> {
    let mut fields = existing.to_vec();

    for assignment in &args.set {
        let (name, value) = assignment
            .split_once('=')
            .with_context(|| format!("--set takes NAME=VALUE, not {assignment:?}"))?;
        // A value passed on the command line is public by default: it has
        // already been through the shell history, so calling it secret would
        // be a promise sefy cannot keep. An existing field keeps whatever
        // secrecy it was stored with — the user is changing the value, not
        // declaring the field harmless.
        set_field(kind, &mut fields, name, value.to_owned(), None);
    }

    for name in &args.set_secret {
        let value = session::secret(&format!("New value for {name}: "), None)?;
        set_field(kind, &mut fields, name, value, Some(true));
    }

    for name in &args.unset {
        let before = fields.len();
        fields.retain(|field| &field.name != name);
        if fields.len() == before {
            bail!(
                "no field named {name:?}; it holds: {}",
                field_names(existing)
            );
        }
    }

    if fields.is_empty() {
        bail!("a {kind} cannot be left without any field");
    }
    Ok(fields)
}

/// Writes one field, in place if it is already there and at the end if not.
fn set_field(
    kind: &ItemKind,
    fields: &mut Vec<Field>,
    name: &str,
    value: String,
    secret: Option<bool>,
) {
    if let Some(field) = fields.iter_mut().find(|field| field.name == name) {
        field.value = value;
        if let Some(secret) = secret {
            field.secret = secret;
        }
        return;
    }

    // A name the template knows brings the template's idea of secrecy with it,
    // so `--set totp=…` on a login is hidden without having to be told.
    let secret = secret.unwrap_or_else(|| {
        kind.template()
            .and_then(|template| template.field(name))
            .is_some_and(|field| field.secret)
    });
    fields.push(Field {
        name: name.to_owned(),
        value,
        secret,
    });
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
    if report.unsupported > 0 {
        println!(
            "{} of a kind this version does not know, not imported\n\
             upgrade sefy and import again",
            output::count(report.unsupported, "item")
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

/// Opens an item's site and puts its password on the clipboard.
///
/// The two halves of signing in, in the order they are used: the browser is
/// already loading while the password is waiting to be pasted. Doing it as one
/// command also removes the step where a user goes looking for the URL, finds
/// it with `show`, and copies it by hand from a terminal.
///
/// The clipboard is the same path `get` uses, timeout and all: a password left
/// on the clipboard after the browser is open is exactly the exposure the
/// timeout exists for, and the browser being open does not change it.
pub fn open(vault: &Vault, args: OpenArgs) -> Result<()> {
    let summary = vault.resolve(&args.reference).map_err(output::explain)?;
    let item = vault.get(summary.id)?;

    let Payload::Fields { kind, fields } = &item.payload else {
        bail!(
            "{:?} is a {}, which has no site to open",
            item.summary.title,
            item.summary.kind.as_str()
        );
    };

    let url = fields
        .iter()
        .find(|field| field.name == URL_FIELD)
        .with_context(|| {
            format!(
                "{:?} has no {URL_FIELD:?} field; it holds: {}\n\
                 add one with: sefy edit {} --set url=https://…",
                item.summary.title,
                field_names(fields),
                item.summary.id
            )
        })?;

    crate::browser::open(&url.value)?;
    println!("opened {}", url.value);

    if args.no_password {
        return Ok(());
    }

    // The password is a convenience on top of opening the site, so a record
    // without one is not an error: the site is open, which is what was asked.
    let Some(secret) = kind
        .template()
        .and_then(|template| template.default_field())
        .and_then(|spec| fields.iter().find(|field| field.name == spec.name))
    else {
        return Ok(());
    };

    println!(
        "copied {} of {:?} to the clipboard; clearing in {}s",
        secret.name, item.summary.title, args.clear_after
    );
    flush_stdout();

    let hold = output::to_clipboard(&secret.value, args.clear_after)?;
    if hold.cleared {
        println!("clipboard cleared");
    }
    Ok(())
}

/// The field `sefy open` looks for.
///
/// Named here rather than inline so the templates and this command cannot
/// drift apart silently — a gate checks that every kind which could be opened
/// declares it.
const URL_FIELD: &str = "url";

/// Reports what and where this vault is, without revealing any of it.
///
/// The question it answers is the one asked after a new machine, a restore or a
/// week away: is this the right file, is everything in it, and has it reached
/// the other side lately. Every line is about shape — counts, versions, the
/// last transfer — and never about contents: a status that named an item would
/// put a secret's title on a screen that was asked only whether the vault is
/// there.
pub fn status(vault: &Vault) -> Result<()> {
    let stats = vault.stats()?;

    println!("vault    {}", vault.path().display());
    if let Some(size) = file_size(vault.path()) {
        println!("size     {size}");
    }
    println!(
        "items    {}{}",
        output::count(stats.items, "item"),
        if stats.by_kind.is_empty() {
            String::new()
        } else {
            let breakdown: Vec<String> = stats
                .by_kind
                .iter()
                .map(|(kind, count)| format!("{count} {kind}"))
                .collect();
            format!("  ({})", breakdown.join(", "))
        }
    );
    println!("tags     {}", output::count(stats.tags, "tag"));

    // The schema the file carries, with a word about what it means, because
    // "schema 4" alone tells a user nothing they can act on.
    let schema = if stats.schema > sefy_core::db::SCHEMA_VERSION {
        format!("{} (written by a newer sefy)", stats.schema)
    } else {
        format!("{}", stats.schema)
    };
    println!("schema   {schema}");

    match &stats.last_sync {
        Some(stamp) => println!(
            "synced   {} through {} ({})",
            crate::when::moment(stamp.at, now()),
            stamp.transport,
            stamp.operation
        ),
        None => println!("synced   never"),
    }

    let plugins = sefy_core::plugin::discover_in(&sefy_core::plugin::search_paths());
    if plugins.is_empty() {
        println!("plugins  none installed");
    } else {
        let names: Vec<String> = plugins
            .iter()
            .map(|plugin| {
                if plugin.usable {
                    plugin.name().to_owned()
                } else {
                    format!("{} (unusable)", plugin.name())
                }
            })
            .collect();
        println!("plugins  {}", names.join(", "));
    }

    Ok(())
}

/// The vault file's size, in units a person reads.
///
/// Absent rather than an error if the file cannot be measured: the vault is
/// open, so it plainly exists, and a status that failed over one cosmetic line
/// would be worse than one missing it.
fn file_size(path: &Path) -> Option<String> {
    let bytes = std::fs::metadata(path).ok()?.len();
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    Some(if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    })
}

/// Unix seconds, for rendering how long ago something happened.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Sends the vault file to the remote.
pub fn push(vault: &mut Vault, args: RemoteArgs) -> Result<()> {
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
        let found = sefy_core::plugin::find(installed, name).ok_or_else(|| {
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

    if report.unsupported > 0 {
        println!(
            "{} left where it was: a kind this version of sefy does not know.\n\
             Nothing was lost — merge again from a build that knows it.",
            output::count(report.unsupported, "item")
        );
    }

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
