//! What a vault holds: notes, structured records and file attachments, each
//! tagged.
//!
//! Everything but a note and a file is a **set of named fields**. A login, a
//! payment card and an SSH key differ in which fields they carry and which of
//! those are secret — not in how they are stored. That is why adding a kind
//! costs a template rather than a table, a payload variant and a new set of
//! flags each time.

use zeroize::Zeroize;

/// Kind of payload an item carries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemKind {
    /// Free-form text.
    Note,
    /// An account: who you are, the secret, and where it is used.
    Login,
    /// A payment card.
    Card,
    /// An SSH key pair and its passphrase.
    SshKey,
    /// A wireless network and how to join it.
    Wifi,
    /// A token for an API: the kind of secret a program presents, not a person.
    ApiToken,
    /// A bank account.
    Bank,
    /// Arbitrary bytes kept verbatim.
    File,
    /// A kind this build does not know, carrying the name it was stored under.
    ///
    /// Vaults travel between machines, and the two ends need not run the same
    /// version. An item written by a newer sefy therefore has to be *something*
    /// here rather than a parse failure: refusing it would take down every
    /// listing, export and merge in a vault that is otherwise perfectly
    /// readable — which is exactly what 0.5.0 and earlier did.
    ///
    /// Such an item can be listed, searched, exported and merged, but not read
    /// or edited: this build does not know the shape behind the name and will
    /// not guess at it.
    Unknown(String),
}

impl ItemKind {
    /// Stable identifier used in the database and on the command line.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Note => "note",
            Self::Login => "login",
            Self::Card => "card",
            Self::SshKey => "ssh-key",
            Self::Wifi => "wifi",
            Self::ApiToken => "api-token",
            Self::Bank => "bank",
            Self::File => "file",
            Self::Unknown(name) => name,
        }
    }

    /// Reads the identifier produced by [`ItemKind::as_str`].
    ///
    /// Never fails: an unrecognized name becomes [`ItemKind::Unknown`], because
    /// a name this build has not heard of is a fact about this build, not a
    /// broken vault.
    ///
    /// `credential` is accepted as a spelling of [`ItemKind::Login`]: that is
    /// what the kind was called up to 0.6.0, and vaults, exports and scripts
    /// written then are still in use.
    pub fn parse(value: &str) -> Self {
        match value {
            "note" => Self::Note,
            "login" | LEGACY_LOGIN_NAME => Self::Login,
            "card" => Self::Card,
            "ssh-key" => Self::SshKey,
            "wifi" => Self::Wifi,
            "api-token" => Self::ApiToken,
            "bank" => Self::Bank,
            "file" => Self::File,
            other => Self::Unknown(other.to_owned()),
        }
    }

    /// Whether this build understands the payload behind the name.
    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }

    /// The fields this kind is made of, or `None` for kinds that are not.
    ///
    /// A note and a file carry a body rather than fields; an unknown kind
    /// carries a shape this build has never seen.
    pub fn template(&self) -> Option<&'static Template> {
        TEMPLATES.iter().find(|template| &template.kind == self)
    }

    /// Whether items of this kind are records made of named fields.
    ///
    /// Asked instead of listing the kinds by name, so a new kind of record is
    /// a row in the template table and nothing else.
    pub fn is_record(&self) -> bool {
        self.template().is_some()
    }

    /// Every kind this build knows how to create.
    pub fn known() -> [Self; 8] {
        [
            Self::Note,
            Self::Login,
            Self::Card,
            Self::SshKey,
            Self::Wifi,
            Self::ApiToken,
            Self::Bank,
            Self::File,
        ]
    }
}

/// What [`ItemKind::Login`] was called up to 0.6.0.
///
/// Kept as a name to read, never one to write: a vault whose rows still say
/// `credential` opens, and so does an export from then, but everything sefy
/// writes from now on says `login`.
pub const LEGACY_LOGIN_NAME: &str = "credential";

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The fields a kind of record is made of.
///
/// A template says what `sefy add` offers, what `sefy show` hides and what
/// order `sefy show` prints in. It does not constrain what a record may hold:
/// a field outside the template is kept and shown like any other, because a
/// store that drops what it was handed is worse than one that is untidy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// Kind this template describes.
    pub kind: ItemKind,
    /// Its fields, in the order they are meant to be read.
    pub fields: &'static [FieldSpec],
}

impl Template {
    /// Looks up one of the template's fields by name.
    pub fn field(&self, name: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|field| field.name == name)
    }

    /// The field `sefy get` takes when none was named.
    ///
    /// The first secret one: for a login that is the password, for a card the
    /// number, for an SSH key the private key. A kind whose fields are all
    /// public has no default — there is no obvious "the secret" to take.
    pub fn default_field(&self) -> Option<&FieldSpec> {
        self.fields.iter().find(|field| field.secret)
    }

    /// The fields `sefy fill` hands over, in the order a form asks for them.
    pub fn fill_order(&self) -> impl Iterator<Item = &FieldSpec> {
        self.fields.iter().filter(|field| field.fill)
    }
}

/// One field of a template: its name, what it is for, and whether it is secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    /// Name the field is stored and addressed under.
    pub name: &'static str,
    /// What it holds, as shown in help text.
    pub description: &'static str,
    /// Whether its value is a secret: hidden by `sefy show`, prompted for
    /// rather than passed on the command line, and what `sefy get` copies.
    pub secret: bool,
    /// Whether `sefy fill` hands it over: something typed into a form when
    /// the record is used, as a login is and a note is not.
    pub fill: bool,
}

impl FieldSpec {
    const fn new(name: &'static str, description: &'static str, secret: bool, fill: bool) -> Self {
        Self {
            name,
            description,
            secret,
            fill,
        }
    }
}

// Spelled out once, so a row of the table below reads as what it says.
const PUBLIC: bool = false;
const SECRET: bool = true;
const FILL: bool = true;
const KEEP: bool = false;

/// Every kind that is made of fields, and what those fields are.
///
/// The order within a kind is the order `sefy show` prints, so it reads
/// top-down the way the record is thought about: who, then the secret, then
/// where.
static TEMPLATES: &[Template] = &[
    Template {
        kind: ItemKind::Login,
        fields: &[
            FieldSpec::new(
                "login",
                "username, email, or whatever the service calls it",
                PUBLIC,
                FILL,
            ),
            FieldSpec::new("password", "the secret itself", SECRET, FILL),
            FieldSpec::new("url", "where the account lives", PUBLIC, KEEP),
            // Filled as the code it makes at that moment, never as itself.
            FieldSpec::new(
                crate::otp::FIELD,
                "key for one-time passwords",
                SECRET,
                FILL,
            ),
            FieldSpec::new("notes", "anything else worth remembering", PUBLIC, KEEP),
        ],
    },
    Template {
        kind: ItemKind::Card,
        fields: &[
            FieldSpec::new("number", "the card number", SECRET, FILL),
            FieldSpec::new("holder", "name embossed on the card", PUBLIC, FILL),
            FieldSpec::new("expiry", "expiry date, as printed", PUBLIC, FILL),
            FieldSpec::new("cvv", "verification code on the back", SECRET, FILL),
            // Typed at a terminal, never into a form.
            FieldSpec::new("pin", "the PIN", SECRET, KEEP),
            FieldSpec::new("notes", "bank, account, anything else", PUBLIC, KEEP),
        ],
    },
    Template {
        kind: ItemKind::SshKey,
        fields: &[
            FieldSpec::new(
                "private-key",
                "the private key, as it appears in the file",
                SECRET,
                KEEP,
            ),
            FieldSpec::new(
                "passphrase",
                "passphrase protecting the private key",
                SECRET,
                FILL,
            ),
            FieldSpec::new("public-key", "the public key", PUBLIC, KEEP),
            FieldSpec::new("host", "where the key is used", PUBLIC, KEEP),
            FieldSpec::new("notes", "anything else worth remembering", PUBLIC, KEEP),
        ],
    },
    Template {
        kind: ItemKind::Wifi,
        fields: &[
            FieldSpec::new("ssid", "the network's name", PUBLIC, FILL),
            FieldSpec::new("password", "the network key", SECRET, FILL),
            FieldSpec::new("security", "WPA2, WPA3, open", PUBLIC, KEEP),
            FieldSpec::new("notes", "where it is, whose it is", PUBLIC, KEEP),
        ],
    },
    Template {
        kind: ItemKind::ApiToken,
        fields: &[
            FieldSpec::new("token", "the token itself", SECRET, FILL),
            FieldSpec::new("url", "where it is issued and revoked", PUBLIC, KEEP),
            FieldSpec::new("scopes", "what it is allowed to do", PUBLIC, KEEP),
            FieldSpec::new("expires", "when it stops working", PUBLIC, KEEP),
            FieldSpec::new("notes", "what uses it", PUBLIC, KEEP),
        ],
    },
    Template {
        kind: ItemKind::Bank,
        fields: &[
            FieldSpec::new("account", "account number or IBAN", SECRET, FILL),
            FieldSpec::new("holder", "whose name the account is in", PUBLIC, FILL),
            FieldSpec::new("bank", "the bank", PUBLIC, FILL),
            FieldSpec::new("routing", "SWIFT/BIC, routing or sort code", PUBLIC, FILL),
            FieldSpec::new("notes", "branch, contact, anything else", PUBLIC, KEEP),
        ],
    },
];

/// One field of a stored record: a name, a value, and whether it is secret.
///
/// Secrecy travels with the value rather than being looked up in a template,
/// so a field the template never heard of is still hidden when it should be,
/// and a record written by another sefy keeps the secrecy it was written with.
#[derive(Debug, Clone, PartialEq, Eq, Zeroize)]
pub struct Field {
    /// What the field is called; unique within a record.
    pub name: String,
    /// What it holds.
    pub value: String,
    /// Whether the value must stay off the screen and out of listings.
    #[zeroize(skip)]
    pub secret: bool,
}

impl Field {
    /// A field holding something anyone may see.
    pub fn public(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            secret: false,
        }
    }

    /// A field holding a secret.
    pub fn secret(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            secret: true,
        }
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
    /// A record made of named fields: a login, a card, an SSH key.
    Fields {
        /// Which kind of record this is.
        kind: ItemKind,
        /// Its fields, in the order they are meant to be read.
        fields: Vec<Field>,
    },
    /// A stored file.
    File {
        /// Name the file had when it was added, used when extracting it.
        filename: String,
        /// File contents, kept byte for byte.
        bytes: Vec<u8>,
    },
    /// An item whose kind this build does not know.
    ///
    /// The contents stay in the vault untouched; there is nothing here because
    /// this build cannot say what shape they have. Carrying the name lets the
    /// item be listed and passed along without being understood.
    Unknown {
        /// The kind name the item was stored under.
        kind: String,
    },
}

impl Payload {
    /// A record of `kind` made of `fields`.
    pub fn fields(kind: ItemKind, fields: impl IntoIterator<Item = Field>) -> Self {
        Self::Fields {
            kind,
            fields: fields.into_iter().collect(),
        }
    }

    /// Kind matching this payload.
    pub fn kind(&self) -> ItemKind {
        match self {
            Self::Note { .. } => ItemKind::Note,
            Self::Fields { kind, .. } => kind.clone(),
            Self::File { .. } => ItemKind::File,
            Self::Unknown { kind } => ItemKind::Unknown(kind.clone()),
        }
    }

    /// Looks up one field of a record by name.
    pub fn field(&self, name: &str) -> Option<&Field> {
        match self {
            Self::Fields { fields, .. } => fields.iter().find(|field| field.name == name),
            _ => None,
        }
    }
}

impl Zeroize for Payload {
    fn zeroize(&mut self) {
        match self {
            Self::Note { text } => text.zeroize(),
            Self::Fields { fields, .. } => fields.zeroize(),
            Self::File { filename, bytes } => {
                filename.zeroize();
                bytes.zeroize();
            }
            // Nothing secret is held here — only the kind's name, which is not
            // a secret and is needed to describe the item.
            Self::Unknown { .. } => {}
        }
    }
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
    /// Case-insensitive substring matched against titles, note text and the
    /// non-secret fields of records. Secret values and file contents are never
    /// scanned.
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
        for kind in ItemKind::known() {
            assert_eq!(ItemKind::parse(kind.as_str()), kind);
        }
    }

    #[test]
    fn an_unheard_of_kind_keeps_its_name_instead_of_failing() {
        let kind = ItemKind::parse("passport");
        assert_eq!(kind, ItemKind::Unknown("passport".to_owned()));
        assert_eq!(kind.as_str(), "passport");
        assert!(!kind.is_known());
        // It survives a round trip like any other name.
        assert_eq!(ItemKind::parse(kind.as_str()), kind);
    }

    #[test]
    fn the_old_name_for_a_login_still_reads() {
        assert_eq!(ItemKind::parse(LEGACY_LOGIN_NAME), ItemKind::Login);
        // But it is never what sefy writes.
        assert_eq!(ItemKind::Login.as_str(), "login");
    }

    #[test]
    fn only_field_backed_kinds_have_a_template() {
        assert!(ItemKind::Login.template().is_some());
        assert!(ItemKind::Card.template().is_some());
        assert!(ItemKind::SshKey.template().is_some());
        assert!(ItemKind::Wifi.template().is_some());
        assert!(ItemKind::ApiToken.template().is_some());
        assert!(ItemKind::Bank.template().is_some());
        assert!(ItemKind::Note.template().is_none());
        assert!(ItemKind::File.template().is_none());
        assert!(
            ItemKind::Unknown("passport".to_owned())
                .template()
                .is_none()
        );
    }

    #[test]
    fn every_template_names_its_fields_uniquely() {
        for template in TEMPLATES {
            let mut seen: Vec<&str> = Vec::new();
            for field in template.fields {
                assert!(
                    !seen.contains(&field.name),
                    "{} names {} twice",
                    template.kind,
                    field.name
                );
                seen.push(field.name);
            }
        }
    }

    #[test]
    fn the_default_field_is_the_first_secret_one() {
        assert_eq!(
            ItemKind::Login
                .template()
                .unwrap()
                .default_field()
                .unwrap()
                .name,
            "password"
        );
        assert_eq!(
            ItemKind::Card
                .template()
                .unwrap()
                .default_field()
                .unwrap()
                .name,
            "number"
        );
        assert_eq!(
            ItemKind::SshKey
                .template()
                .unwrap()
                .default_field()
                .unwrap()
                .name,
            "private-key"
        );
    }

    #[test]
    fn every_kind_but_a_note_and_a_file_is_a_record_with_a_template_row() {
        for kind in ItemKind::known() {
            let expected = !matches!(kind, ItemKind::Note | ItemKind::File);
            assert_eq!(kind.is_record(), expected, "{kind}");
        }
        // And every row describes a kind this build knows.
        for template in TEMPLATES {
            assert!(
                ItemKind::known().contains(&template.kind),
                "{}",
                template.kind
            );
        }
    }

    #[test]
    fn a_one_time_password_key_is_secret_wherever_a_template_has_one() {
        for template in TEMPLATES {
            if let Some(field) = template.field(crate::otp::FIELD) {
                assert!(field.secret, "{} keeps its key in the open", template.kind);
            }
        }
    }

    #[test]
    fn a_login_fills_in_the_order_a_sign_in_form_asks() {
        let order: Vec<&str> = ItemKind::Login
            .template()
            .unwrap()
            .fill_order()
            .map(|field| field.name)
            .collect();
        assert_eq!(order, ["login", "password", crate::otp::FIELD]);
    }

    #[test]
    fn a_field_is_looked_up_by_name() {
        let payload = Payload::fields(
            ItemKind::Login,
            [
                Field::public("login", "ada"),
                Field::secret("password", "s"),
            ],
        );
        assert_eq!(payload.field("login").unwrap().value, "ada");
        assert!(payload.field("password").unwrap().secret);
        assert!(payload.field("url").is_none());
    }
}
