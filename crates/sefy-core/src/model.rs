//! What a vault holds: notes, credentials and file attachments, each tagged.

use zeroize::Zeroize;

/// Kind of payload an item carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    /// Free-form text.
    Note,
    /// Login, password, URL and an optional TOTP secret.
    Credential,
    /// Arbitrary bytes kept verbatim.
    File,
}

impl ItemKind {
    /// Stable identifier used in the database and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Credential => "credential",
            Self::File => "file",
        }
    }

    /// Parses the identifier produced by [`ItemKind::as_str`].
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "note" => Some(Self::Note),
            "credential" => Some(Self::Credential),
            "file" => Some(Self::File),
            _ => None,
        }
    }
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The payload of an item, in the shape its kind implies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// Free-form text.
    Note {
        /// The text itself.
        text: String,
    },
    /// A set of login details.
    Credential(Credential),
    /// A stored file.
    File {
        /// Name the file had when it was added, used when extracting it.
        filename: String,
        /// File contents, kept byte for byte.
        bytes: Vec<u8>,
    },
}

impl Payload {
    /// Kind matching this payload.
    pub fn kind(&self) -> ItemKind {
        match self {
            Self::Note { .. } => ItemKind::Note,
            Self::Credential(_) => ItemKind::Credential,
            Self::File { .. } => ItemKind::File,
        }
    }
}

impl Zeroize for Payload {
    fn zeroize(&mut self) {
        match self {
            Self::Note { text } => text.zeroize(),
            Self::Credential(credential) => credential.zeroize(),
            Self::File { filename, bytes } => {
                filename.zeroize();
                bytes.zeroize();
            }
        }
    }
}

/// Login details for one account.
#[derive(Debug, Clone, Default, PartialEq, Eq, Zeroize)]
pub struct Credential {
    /// Username, email or whatever the service calls it.
    pub login: String,
    /// The secret itself.
    pub password: String,
    /// Where the account lives.
    pub url: Option<String>,
    /// Shared secret for time-based one-time passwords.
    pub totp: Option<String>,
    /// Anything else worth remembering about the account.
    pub notes: Option<String>,
}

/// An item without its payload: enough to list and search, cheap to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSummary {
    /// Identifier within this vault. Assigned by the database, so the same item
    /// carries different ids in two vaults — use [`ItemSummary::uuid`] to
    /// recognise it across them.
    pub id: i64,
    /// Identity that survives leaving this vault: stable across machines,
    /// exports and imports, and what merging matches on.
    pub uuid: String,
    /// What the item is called.
    pub title: String,
    /// Kind of payload behind the summary.
    pub kind: ItemKind,
    /// Tags attached to the item, sorted.
    pub tags: Vec<String>,
    /// Creation time, seconds since the Unix epoch.
    pub created_at: i64,
    /// Time of the last change, seconds since the Unix epoch.
    pub updated_at: i64,
}

/// A complete item: its summary and its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Listing fields.
    pub summary: ItemSummary,
    /// The secret material.
    pub payload: Payload,
}

/// A new item on its way into the vault.
#[derive(Debug, Clone)]
pub struct NewItem {
    /// What to call it.
    pub title: String,
    /// What it holds.
    pub payload: Payload,
    /// Tags to attach; duplicates and empty strings are ignored.
    pub tags: Vec<String>,
}

impl NewItem {
    /// Builds an item with a title and a payload, no tags.
    pub fn new(title: impl Into<String>, payload: Payload) -> Self {
        Self {
            title: title.into(),
            payload,
            tags: Vec::new(),
        }
    }

    /// Attaches tags to the item under construction.
    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }
}

/// Which items to return from a search.
#[derive(Debug, Clone, Default)]
pub struct Query {
    /// Case-insensitive substring matched against titles and, for notes and
    /// credentials, against their text. File contents are never scanned.
    pub text: Option<String>,
    /// Keep only items of this kind.
    pub kind: Option<ItemKind>,
    /// Keep only items carrying every one of these tags.
    pub tags: Vec<String>,
}

impl Query {
    /// A query matching every item in the vault.
    pub fn all() -> Self {
        Self::default()
    }

    /// Restricts the query to items matching `text`.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Restricts the query to one kind of item.
    pub fn kind(mut self, kind: ItemKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Restricts the query to items carrying all of `tags`.
    pub fn tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_round_trip() {
        for kind in [ItemKind::Note, ItemKind::Credential, ItemKind::File] {
            assert_eq!(ItemKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ItemKind::parse("passport"), None);
    }
}
