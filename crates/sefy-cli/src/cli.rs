//! Command-line surface: what `sefy` accepts and what each option means.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// An inconspicuous encrypted store for notes, credentials and files.
#[derive(Debug, Parser)]
#[command(name = "sefy", version, about, long_about = None)]
pub struct Cli {
    /// Vault file to work on; falls back to $SEFY_VAULT.
    ///
    /// There is deliberately no default path: a vault at a predictable
    /// location would be exactly the giveaway the format avoids.
    #[arg(
        long,
        short = 'f',
        global = true,
        env = "SEFY_VAULT",
        value_name = "FILE"
    )]
    pub vault: Option<PathBuf>,

    /// Read the master password from this environment variable.
    ///
    /// For scripts and CI. Passing a password as an argument is not supported:
    /// it would land in the shell history and in every process listing.
    #[arg(long, global = true, value_name = "VAR")]
    pub password_env: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

/// What the user asked sefy to do.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a new vault.
    Init,

    /// Add an item to the vault.
    Add {
        #[command(subcommand)]
        kind: AddKind,
    },

    /// Copy a secret to the clipboard, or print it with --stdout.
    Get(GetArgs),

    /// Show an item without revealing its secret fields.
    Show {
        /// Item id, exact title, or text to search for.
        reference: String,
    },

    /// List items in the vault.
    Ls(ListArgs),

    /// Search items by text, kind and tags.
    Find(FindArgs),

    /// Change an item's title, contents or tags.
    Edit(EditArgs),

    /// Remove an item.
    Rm {
        /// Item id, exact title, or text to search for.
        reference: String,

        /// Do not ask for confirmation.
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Write a stored file back to disk.
    Extract {
        /// Item id, exact title, or text to search for.
        reference: String,

        /// Where to write it; defaults to the stored filename in the current
        /// directory.
        #[arg(long, short = 'o', value_name = "PATH")]
        output: Option<PathBuf>,

        /// Overwrite the destination if it exists.
        #[arg(long)]
        force: bool,
    },

    /// List the tags in use.
    Tags,

    /// Write the vault's contents out as plain, unencrypted JSON.
    Export {
        /// Where to write it; omit to print to stdout.
        #[arg(long, short = 'o', value_name = "PATH")]
        output: Option<PathBuf>,

        /// Required: acknowledge that this writes every secret in the clear.
        ///
        /// Export exists so a vault is never a trap, but the file it produces
        /// is exactly as sensitive as the vault — and unlike the vault, it
        /// protects nothing. A warning printed after the fact would be too
        /// late, so the acknowledgement comes first.
        #[arg(long)]
        i_know_this_writes_plaintext: bool,

        /// Overwrite the destination if it exists.
        #[arg(long)]
        force: bool,
    },

    /// Add the contents of an export to this vault.
    ///
    /// An entry the vault already holds under the same identity is skipped, so
    /// importing the same export twice does not duplicate it. Contents are
    /// never overwritten: an export may well be older than what is here, and
    /// bringing newer contents across is what `merge` is for.
    Import {
        /// Export file to read; omit to read from stdin.
        #[arg(value_name = "PATH")]
        input: Option<PathBuf>,
    },

    /// Fold another vault file into this one.
    ///
    /// For two copies that drifted apart: items missing here are copied across,
    /// newer contents from there replace older ones here, and an item changed
    /// on both sides is kept twice rather than resolved by guesswork. Nothing
    /// is ever deleted — "gone from there" and "added here" look identical from
    /// this side.
    Merge {
        /// The other vault file.
        #[arg(value_name = "FILE")]
        other: PathBuf,

        /// Read the other vault's password from this environment variable.
        ///
        /// Distinct from --password-env: a copy from another machine may well
        /// be under a different password, and reusing one variable for both
        /// would make that impossible to express.
        #[arg(long, value_name = "VAR")]
        other_password_env: Option<String>,
    },

    /// Replace the master password.
    ChangePassword {
        /// Read the *new* password from this environment variable.
        ///
        /// Distinct from --password-env, which carries the current one: with a
        /// single variable the "new" password would be the old one.
        #[arg(long, value_name = "VAR")]
        new_password_env: Option<String>,
    },

    /// Print a shell completion script.
    Completions {
        /// Shell to generate for.
        shell: clap_complete::Shell,
    },
}

/// The kind of item being added.
#[derive(Debug, Subcommand)]
pub enum AddKind {
    /// A free-form note.
    Note {
        /// What to call it.
        title: String,

        /// The text; omit to read it from stdin, or pass --editor.
        #[arg(long, short = 't', conflicts_with = "editor")]
        text: Option<String>,

        /// Write the note in $EDITOR.
        #[arg(long, short = 'e')]
        editor: bool,

        /// Tags to attach; repeat or separate with commas.
        #[arg(long, value_delimiter = ',')]
        tag: Vec<String>,
    },

    /// A login, its password and where it is used.
    Credential {
        /// What to call it.
        title: String,

        /// Username, email, or whatever the service calls it.
        #[arg(long, short = 'l')]
        login: String,

        /// Where the account lives.
        #[arg(long, short = 'u')]
        url: Option<String>,

        /// Shared secret for time-based one-time passwords.
        #[arg(long)]
        totp: Option<String>,

        /// Anything else worth remembering.
        #[arg(long)]
        notes: Option<String>,

        /// Read the item's password from this environment variable instead of
        /// prompting.
        ///
        /// Deliberately separate from the global --password-env, which carries
        /// the master password.
        #[arg(long = "item-password-env", value_name = "VAR")]
        item_password_env: Option<String>,

        /// Tags to attach; repeat or separate with commas.
        #[arg(long, value_delimiter = ',')]
        tag: Vec<String>,
    },

    /// A file, stored byte for byte.
    File {
        /// File to read.
        path: PathBuf,

        /// What to call it; defaults to the file name.
        #[arg(long, short = 'T')]
        title: Option<String>,

        /// Tags to attach; repeat or separate with commas.
        #[arg(long, value_delimiter = ',')]
        tag: Vec<String>,
    },
}

/// Arguments of `sefy get`.
#[derive(Debug, Args)]
pub struct GetArgs {
    /// Item id, exact title, or text to search for.
    pub reference: String,

    /// Which field to take from a credential.
    #[arg(long, value_enum, default_value_t = Field::Password)]
    pub field: Field,

    /// Print the secret instead of copying it to the clipboard.
    ///
    /// Convenient for pipes, but the secret then lives in the terminal
    /// scrollback.
    #[arg(long)]
    pub stdout: bool,

    /// Seconds before the clipboard is cleared again; 0 leaves it there.
    ///
    /// sefy waits that long before exiting, and clears the clipboard only if
    /// the secret is still the value sitting on it.
    #[arg(long, value_name = "SECONDS", default_value_t = 45)]
    pub clear_after: u64,
}

/// A field of a credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Field {
    /// The password.
    Password,
    /// The login.
    Login,
    /// The URL.
    Url,
    /// The TOTP secret.
    Totp,
}

impl Field {
    /// Name of the field as it appears in messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Login => "login",
            Self::Url => "url",
            Self::Totp => "totp",
        }
    }
}

/// Arguments of `sefy ls`.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Keep only items of this kind.
    #[arg(long, value_enum)]
    pub kind: Option<Kind>,

    /// Keep only items carrying every one of these tags.
    #[arg(long, value_delimiter = ',')]
    pub tag: Vec<String>,
}

/// Arguments of `sefy find`.
#[derive(Debug, Args)]
pub struct FindArgs {
    /// Text to look for in titles, notes and credential fields.
    ///
    /// The contents of stored files are not searched.
    pub text: Option<String>,

    /// Keep only items of this kind.
    #[arg(long, value_enum)]
    pub kind: Option<Kind>,

    /// Keep only items carrying every one of these tags.
    #[arg(long, value_delimiter = ',')]
    pub tag: Vec<String>,
}

/// Arguments of `sefy edit`.
#[derive(Debug, Args)]
pub struct EditArgs {
    /// Item id, exact title, or text to search for.
    pub reference: String,

    /// New title.
    #[arg(long)]
    pub title: Option<String>,

    /// New text, for a note.
    #[arg(long, short = 't', conflicts_with = "editor")]
    pub text: Option<String>,

    /// Open the note in $EDITOR instead of passing its text on the command
    /// line.
    ///
    /// The draft is written to a temporary file, which is overwritten and
    /// removed as soon as the editor exits — but while the editor is open, that
    /// file holds the note in the clear, and an editor's own swap or backup
    /// files are outside sefy's reach.
    #[arg(long, short = 'e')]
    pub editor: bool,

    /// New login, for a credential.
    #[arg(long, short = 'l')]
    pub login: Option<String>,

    /// Prompt for a new password, for a credential.
    #[arg(long)]
    pub password: bool,

    /// Read the item's new password from this environment variable.
    #[arg(
        long = "item-password-env",
        value_name = "VAR",
        conflicts_with = "password"
    )]
    pub item_password_env: Option<String>,

    /// New URL, for a credential.
    #[arg(long, short = 'u')]
    pub url: Option<String>,

    /// New TOTP secret, for a credential.
    #[arg(long)]
    pub totp: Option<String>,

    /// New notes, for a credential.
    #[arg(long)]
    pub notes: Option<String>,

    /// Replace the item's tags; repeat or separate with commas.
    #[arg(long, value_delimiter = ',')]
    pub tag: Vec<String>,

    /// Remove every tag from the item.
    #[arg(long, conflicts_with = "tag")]
    pub clear_tags: bool,
}

/// Kind of item, as spelled on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    /// A free-form note.
    Note,
    /// A login and its password.
    Credential,
    /// A stored file.
    File,
}

impl From<Kind> for sefy_core::ItemKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Note => Self::Note,
            Kind::Credential => Self::Credential,
            Kind::File => Self::File,
        }
    }
}
