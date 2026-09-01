//! The database living inside the encrypted blob.
//!
//! The connection is always in-memory: bytes arrive decrypted, are handed to
//! SQLite's deserialize interface, and go back out through serialize. SQLite is
//! never pointed at a path, so no plaintext page ever reaches the disk.

use crate::error::{Error, Result};
use crate::model::{
    Field, Item, ItemKind, ItemSummary, LEGACY_LOGIN_NAME, NewItem, Payload, Query,
};
use rusqlite::{Connection, MAIN_DB, OptionalExtension, params};
use std::io::Cursor;

/// Schema version of the database inside the blob.
///
/// Distinct from the file format version: the envelope can stay the same while
/// the tables under it grow. Version 2 added `items.uuid`, which is why a vault
/// written by 0.1.x still opens. Version 3 replaced the `credentials` table
/// with `fields`, so that a kind of record costs a template rather than a
/// table — a vault written by 0.6.0 or earlier is migrated on load, not
/// rejected.
const SCHEMA_VERSION: i64 = 3;

/// Opens an empty in-memory database with the current schema.
pub fn create() -> Result<Connection> {
    let connection = Connection::open_in_memory()?;
    configure(&connection)?;
    migrate(&connection)?;
    Ok(connection)
}

/// Loads a serialized database into memory and brings it up to date.
pub fn load(database: &[u8]) -> Result<Connection> {
    let mut connection = Connection::open_in_memory()?;
    connection.deserialize_read_exact(MAIN_DB, Cursor::new(database), database.len(), false)?;
    configure(&connection)?;
    migrate(&connection)?;
    Ok(connection)
}

/// Serializes the in-memory database back to bytes.
pub fn dump(connection: &Connection) -> Result<Vec<u8>> {
    Ok(connection.serialize(MAIN_DB)?.to_vec())
}

fn configure(connection: &Connection) -> Result<()> {
    // Journaling has nothing to protect here — the database is a memory buffer
    // that is either sealed to disk whole or not at all.
    connection.execute_batch(
        "PRAGMA journal_mode = MEMORY;
         PRAGMA temp_store = MEMORY;
         PRAGMA foreign_keys = ON;",
    )?;
    Ok(())
}

fn migrate(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if version < SCHEMA_VERSION {
        // Steps run in order and each is idempotent, so a vault written by any
        // earlier version arrives at the current schema by the same path.
        migrate_to_v1(connection)?;
        migrate_to_v2(connection)?;
        migrate_to_v3(connection)?;
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }

    // Unconditional, even at the current version: a migrated vault stays
    // readable *and writable* by an older build, which knows nothing of what
    // was added here. So an up-to-date `user_version` does not prove every row
    // has been brought along — only that this database passed through here
    // once. Both passes below only ever do work on a row that needs it.
    assign_missing_uuids(connection)?;
    move_credentials_into_fields(connection)?;
    Ok(())
}

/// Adds `items.uuid`: an identity that survives leaving this vault.
///
/// Row ids cannot serve: they are per-vault autoincrements, so the same item
/// carries different ids on two machines while unrelated items collide. Merging
/// and re-importing both need to recognise an item, which is what this is for.
fn migrate_to_v2(connection: &Connection) -> Result<()> {
    let already_there = connection
        .prepare("SELECT 1 FROM pragma_table_info('items') WHERE name = 'uuid'")?
        .exists([])?;
    if !already_there {
        connection.execute_batch("ALTER TABLE items ADD COLUMN uuid TEXT")?;
    }

    assign_missing_uuids(connection)?;

    // Unique rather than merely indexed: two items sharing an identity would
    // make a merge ambiguous, and there is no sane way to guess which is which.
    // NULLs are exempt in SQLite, which is why the pass above runs first.
    connection.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS idx_items_uuid ON items(uuid)")?;
    Ok(())
}

/// Gives an identity to every item that lacks one.
///
/// Two ways a row gets here: it predates the column, or an older build inserted
/// it after this vault was already migrated. Both are answered the same way,
/// and both only ever happen once per row.
fn assign_missing_uuids(connection: &Connection) -> Result<()> {
    let missing: Vec<i64> = connection
        .prepare("SELECT id FROM items WHERE uuid IS NULL OR uuid = ''")?
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;

    for id in missing {
        connection.execute(
            "UPDATE items SET uuid = ?1 WHERE id = ?2",
            params![new_uuid()?, id],
        )?;
    }
    Ok(())
}

/// Adds `fields`, the one table every record made of named fields lives in.
///
/// Up to 0.6.0 a credential had its own table with five fixed columns, which
/// made every further kind — a card, an SSH key — cost another table, another
/// payload variant and another set of flags. Fields cost a template instead.
fn migrate_to_v3(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS fields (
             item_id  INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             name     TEXT    NOT NULL,
             value    TEXT    NOT NULL,
             secret   INTEGER NOT NULL,
             position INTEGER NOT NULL,
             PRIMARY KEY (item_id, name)
         );

         CREATE INDEX IF NOT EXISTS idx_fields_item ON fields(item_id, position);",
    )?;

    move_credentials_into_fields(connection)?;
    Ok(())
}

/// One row of the pre-0.7.0 `credentials` table.
type LegacyCredential = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Turns every remaining `credentials` row into fields, then empties the table.
///
/// The table itself stays — dropping it would make an older build's own writes
/// fail outright rather than be carried across — but its rows do not. A row
/// left behind would still be readable by 0.6.0 through `get`, while that same
/// build's `show` and `ls` called the item a kind they do not know: one binary
/// giving two answers to "do you understand this item". Moving the contents and
/// leaving nothing behind makes the older build wrong about the item
/// consistently, which is the honest half of the two.
///
/// This runs on every open rather than behind a version check, so a row 0.6.0
/// inserts *after* this vault was migrated is picked up on the next open
/// instead of being lost. `user_version` says "this database passed through the
/// migration once", never "every row has been brought along" — the lesson the
/// uuid migration taught in 0.2.0.
fn move_credentials_into_fields(connection: &Connection) -> Result<()> {
    let table_exists = connection
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'credentials'")?
        .exists([])?;
    if !table_exists {
        return Ok(());
    }

    let leftovers: Vec<LegacyCredential> = connection
        .prepare(
            "SELECT item_id, login, password, url, totp, notes FROM credentials
                 WHERE item_id NOT IN (SELECT item_id FROM fields)",
        )?
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;

    for (item_id, login, password, url, totp, notes) in leftovers {
        let mut fields = vec![
            Field::public("login", login),
            Field::secret("password", password),
        ];
        // An absent optional stays absent: writing it as an empty field would
        // turn "this login has no URL" into "its URL is the empty string".
        if let Some(url) = url {
            fields.push(Field::public("url", url));
        }
        if let Some(totp) = totp {
            fields.push(Field::secret("totp", totp));
        }
        if let Some(notes) = notes {
            fields.push(Field::public("notes", notes));
        }
        write_fields(connection, item_id, &fields)?;
    }

    // The kind is renamed alongside, so the vault says `login` everywhere and
    // an item does not read as one kind in its row and another in its fields.
    connection.execute(
        "UPDATE items SET kind = ?1 WHERE kind = ?2",
        params![ItemKind::Login.as_str(), LEGACY_LOGIN_NAME],
    )?;

    // Emptied only once the contents are safely in `fields`. A crash between
    // the two leaves the vault holding both copies, which the next open
    // resolves exactly the same way rather than losing anything.
    connection.execute("DELETE FROM credentials", [])?;
    Ok(())
}

/// A random UUID (version 4), rendered in the usual hyphenated form.
pub fn new_uuid() -> Result<String> {
    let mut bytes = crate::crypto::random_bytes::<16>()?;
    // Version 4, variant 1 — the bits that say "this was made from randomness".
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

fn migrate_to_v1(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS items (
             id         INTEGER PRIMARY KEY,
             title      TEXT    NOT NULL,
             kind       TEXT    NOT NULL,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS notes (
             item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
             text    TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS credentials (
             item_id  INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
             login    TEXT NOT NULL,
             password TEXT NOT NULL,
             url      TEXT,
             totp     TEXT,
             notes    TEXT
         );

         CREATE TABLE IF NOT EXISTS files (
             item_id  INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
             filename TEXT NOT NULL,
             bytes    BLOB NOT NULL
         );

         CREATE TABLE IF NOT EXISTS tags (
             id   INTEGER PRIMARY KEY,
             name TEXT NOT NULL UNIQUE
         );

         CREATE TABLE IF NOT EXISTS item_tags (
             item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
             tag_id  INTEGER NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
             PRIMARY KEY (item_id, tag_id)
         );

         CREATE INDEX IF NOT EXISTS idx_items_kind  ON items(kind);
         CREATE INDEX IF NOT EXISTS idx_items_title ON items(title);
         CREATE INDEX IF NOT EXISTS idx_item_tags_tag ON item_tags(tag_id);",
    )?;
    Ok(())
}

/// Inserts an item and returns its identifier.
pub fn insert_item(connection: &mut Connection, item: NewItem, now: i64) -> Result<i64> {
    insert_item_with_uuid(connection, item, &new_uuid()?, now, now)
}

/// Inserts an item under an identity and timestamps decided by the caller.
///
/// Merging and importing need this: an item arriving from another vault keeps
/// the identity and the history it already had, or the two copies would stop
/// being the same item the moment they travelled.
pub fn insert_item_with_uuid(
    connection: &mut Connection,
    item: NewItem,
    uuid: &str,
    created_at: i64,
    updated_at: i64,
) -> Result<i64> {
    let transaction = connection.transaction()?;
    let kind = item.payload.kind();

    transaction.execute(
        "INSERT INTO items (uuid, title, kind, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![uuid, item.title, kind.as_str(), created_at, updated_at],
    )?;
    let id = transaction.last_insert_rowid();

    insert_payload(&transaction, id, &item.payload)?;
    set_tags(&transaction, id, &item.tags)?;

    transaction.commit()?;
    Ok(id)
}

/// Finds the item carrying this identity, if the vault holds one.
pub fn find_by_uuid(connection: &Connection, uuid: &str) -> Result<Option<i64>> {
    Ok(connection
        .query_row(
            "SELECT id FROM items WHERE uuid = ?1",
            params![uuid],
            |row| row.get(0),
        )
        .optional()?)
}

fn insert_payload(connection: &Connection, id: i64, payload: &Payload) -> Result<()> {
    match payload {
        Payload::Note { text } => {
            connection.execute(
                "INSERT INTO notes (item_id, text) VALUES (?1, ?2)",
                params![id, text],
            )?;
        }
        Payload::Fields { fields, .. } => write_fields(connection, id, fields)?,
        Payload::File { filename, bytes } => {
            connection.execute(
                "INSERT INTO files (item_id, filename, bytes) VALUES (?1, ?2, ?3)",
                params![id, filename, bytes],
            )?;
        }
        // Refused rather than skipped. There is no table to put this in, and
        // writing the row without its contents would turn an item this build
        // merely cannot read into one that is genuinely empty — destroying data
        // in the name of tolerating it.
        Payload::Unknown { kind } => {
            return Err(Error::UnknownItemKind {
                id,
                kind: kind.clone(),
            });
        }
    }
    Ok(())
}

/// Writes a record's fields, keeping the order they were given in.
///
/// Position is stored rather than derived: the template's order is what a
/// record is meant to be read in, and a field the template never heard of has
/// to sit somewhere stable too.
fn write_fields(connection: &Connection, id: i64, fields: &[Field]) -> Result<()> {
    for (position, field) in fields.iter().enumerate() {
        connection.execute(
            "INSERT OR REPLACE INTO fields (item_id, name, value, secret, position)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, field.name, field.value, field.secret, position as i64],
        )?;
    }
    Ok(())
}

fn read_fields(connection: &Connection, id: i64) -> Result<Vec<Field>> {
    let mut statement = connection.prepare(
        "SELECT name, value, secret FROM fields WHERE item_id = ?1 ORDER BY position, name",
    )?;
    let fields = statement
        .query_map(params![id], |row| {
            Ok(Field {
                name: row.get(0)?,
                value: row.get(1)?,
                secret: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(fields)
}

fn delete_payload(connection: &Connection, id: i64) -> Result<()> {
    connection.execute("DELETE FROM notes WHERE item_id = ?1", params![id])?;
    connection.execute("DELETE FROM fields WHERE item_id = ?1", params![id])?;
    connection.execute("DELETE FROM files WHERE item_id = ?1", params![id])?;
    // The pre-v3 table is emptied for this item too: leaving the old row behind
    // would let the migration pass resurrect the contents that were just
    // replaced, the next time this vault is opened.
    let has_credentials = connection
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'credentials'")?
        .exists([])?;
    if has_credentials {
        connection.execute("DELETE FROM credentials WHERE item_id = ?1", params![id])?;
    }
    Ok(())
}

fn set_tags(connection: &Connection, id: i64, tags: &[String]) -> Result<()> {
    connection.execute("DELETE FROM item_tags WHERE item_id = ?1", params![id])?;
    for tag in normalize_tags(tags) {
        connection.execute(
            "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
            params![tag],
        )?;
        let tag_id: i64 =
            connection.query_row("SELECT id FROM tags WHERE name = ?1", params![tag], |row| {
                row.get(0)
            })?;
        connection.execute(
            "INSERT OR IGNORE INTO item_tags (item_id, tag_id) VALUES (?1, ?2)",
            params![id, tag_id],
        )?;
    }
    prune_tags(connection)?;
    Ok(())
}

/// Drops tag rows no item refers to any more.
///
/// Tags exist to group items; one left behind by an edit or a deletion would
/// otherwise linger in `sefy tags` forever.
fn prune_tags(connection: &Connection) -> Result<()> {
    connection.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM item_tags)",
        [],
    )?;
    Ok(())
}

/// Trims tags, drops empty ones and removes duplicates, keeping order stable.
fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        if !seen.iter().any(|kept: &String| kept == tag) {
            seen.push(tag.to_owned());
        }
    }
    seen
}

/// Replaces the title, payload and tags of an existing item.
pub fn update_item(
    connection: &mut Connection,
    id: i64,
    title: Option<String>,
    payload: Option<Payload>,
    tags: Option<Vec<String>>,
    now: i64,
) -> Result<()> {
    let transaction = connection.transaction()?;

    let existing_kind: Option<String> = transaction
        .query_row("SELECT kind FROM items WHERE id = ?1", params![id], |row| {
            row.get(0)
        })
        .optional()?;
    let existing_kind = existing_kind.ok_or(Error::ItemNotFound(id))?;

    if let Some(title) = title {
        transaction.execute(
            "UPDATE items SET title = ?2 WHERE id = ?1",
            params![id, title],
        )?;
    }

    if let Some(payload) = payload {
        let kind = payload.kind();
        // Changing an item's kind would leave callers holding an id whose shape
        // silently changed; edits stay within the kind the item was created as.
        // Compared as parsed kinds, so a row still saying `credential` matches
        // the `login` it is read as.
        if kind != ItemKind::parse(&existing_kind) {
            return Err(Error::ItemKindMismatch {
                id,
                actual: existing_kind,
                expected: kind.as_str().to_owned(),
            });
        }
        delete_payload(&transaction, id)?;
        insert_payload(&transaction, id, &payload)?;
        transaction.execute(
            "UPDATE items SET kind = ?2 WHERE id = ?1",
            params![id, kind.as_str()],
        )?;
    }

    if let Some(tags) = tags {
        set_tags(&transaction, id, &tags)?;
    }

    transaction.execute(
        "UPDATE items SET updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    transaction.commit()?;
    Ok(())
}

/// Removes an item and everything hanging off it.
pub fn delete_item(connection: &Connection, id: i64) -> Result<()> {
    let removed = connection.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    if removed == 0 {
        return Err(Error::ItemNotFound(id));
    }
    prune_tags(connection)?;
    Ok(())
}

/// Reads one item with its payload.
pub fn get_item(connection: &Connection, id: i64) -> Result<Item> {
    let summary = get_summary(connection, id)?;
    let payload = match summary.kind {
        ItemKind::Note => {
            let text = connection.query_row(
                "SELECT text FROM notes WHERE item_id = ?1",
                params![id],
                |row| row.get(0),
            )?;
            Payload::Note { text }
        }
        ItemKind::Login | ItemKind::Card | ItemKind::SshKey => Payload::Fields {
            kind: summary.kind.clone(),
            fields: read_fields(connection, id)?,
        },
        ItemKind::File => connection.query_row(
            "SELECT filename, bytes FROM files WHERE item_id = ?1",
            params![id],
            |row| {
                Ok(Payload::File {
                    filename: row.get(0)?,
                    bytes: row.get(1)?,
                })
            },
        )?,
        // Written by a newer sefy: the row exists, and so does whatever table
        // holds its contents, but this build knows neither the table nor the
        // shape. The item is reported as itself rather than as an error.
        ItemKind::Unknown(ref name) => Payload::Unknown { kind: name.clone() },
    };
    Ok(Item { summary, payload })
}

/// Reads one item without its payload.
pub fn get_summary(connection: &Connection, id: i64) -> Result<ItemSummary> {
    let row = connection
        .query_row(
            "SELECT id, uuid, title, kind, created_at, updated_at FROM items WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()?;
    let (id, uuid, title, kind, created_at, updated_at) = row.ok_or(Error::ItemNotFound(id))?;

    Ok(ItemSummary {
        id,
        uuid,
        title,
        kind: ItemKind::parse(&kind),
        tags: tags_of(connection, id)?,
        created_at,
        updated_at,
    })
}

fn tags_of(connection: &Connection, id: i64) -> Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT t.name FROM tags t
         JOIN item_tags it ON it.tag_id = t.id
         WHERE it.item_id = ?1
         ORDER BY t.name",
    )?;
    let tags = statement
        .query_map(params![id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tags)
}

/// Finds items matching a query, newest first.
pub fn search(connection: &Connection, query: &Query) -> Result<Vec<ItemSummary>> {
    let mut sql = String::from(
        "SELECT DISTINCT i.id FROM items i
         LEFT JOIN notes n ON n.item_id = i.id
         WHERE 1 = 1",
    );
    let mut arguments: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(kind) = &query.kind {
        arguments.push(Box::new(kind.as_str().to_owned()));
        let placeholder = arguments.len();
        if *kind == ItemKind::Login {
            // A vault last written by 0.6.0 still says `credential` in its
            // rows until it is opened for writing, and `ls --kind login` has
            // to find those too.
            arguments.push(Box::new(LEGACY_LOGIN_NAME.to_owned()));
            sql.push_str(&format!(
                " AND i.kind IN (?{placeholder}, ?{})",
                arguments.len()
            ));
        } else {
            sql.push_str(&format!(" AND i.kind = ?{placeholder}"));
        }
    }

    if let Some(text) = query
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        // Attachment bytes are deliberately left out: a blob match would say
        // nothing useful and would mean scanning every file in the vault. So
        // are secret field values — a password is not something to find an item
        // by, and matching one would tell a bystander it is in there.
        arguments.push(Box::new(format!("%{}%", escape_like(text))));
        let placeholder = arguments.len();
        sql.push_str(&format!(
            " AND (i.title LIKE ?{p} ESCAPE '\\'
                OR n.text  LIKE ?{p} ESCAPE '\\'
                OR EXISTS (SELECT 1 FROM fields f
                           WHERE f.item_id = i.id AND f.secret = 0
                             AND f.value LIKE ?{p} ESCAPE '\\'))",
            p = placeholder
        ));
    }

    for tag in normalize_tags(&query.tags) {
        arguments.push(Box::new(tag));
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM item_tags it
                          JOIN tags t ON t.id = it.tag_id
                          WHERE it.item_id = i.id AND t.name = ?{})",
            arguments.len()
        ));
    }

    sql.push_str(" ORDER BY i.updated_at DESC, i.id DESC");

    let mut statement = connection.prepare(&sql)?;
    let bindings: Vec<&dyn rusqlite::ToSql> = arguments.iter().map(AsRef::as_ref).collect();
    let ids = statement
        .query_map(bindings.as_slice(), |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    ids.into_iter()
        .map(|id| get_summary(connection, id))
        .collect()
}

/// Escapes the wildcards SQL `LIKE` would otherwise interpret.
fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Lists every tag in the vault with how many items carry it.
pub fn list_tags(connection: &Connection) -> Result<Vec<(String, i64)>> {
    let mut statement = connection.prepare(
        "SELECT t.name, COUNT(it.item_id) FROM tags t
         LEFT JOIN item_tags it ON it.tag_id = t.id
         GROUP BY t.id
         ORDER BY t.name",
    )?;
    let tags = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_tags_trims_and_deduplicates() {
        let tags = vec![
            " bank ".to_owned(),
            "bank".to_owned(),
            String::new(),
            "  ".to_owned(),
            "mail".to_owned(),
        ];
        assert_eq!(normalize_tags(&tags), vec!["bank", "mail"]);
    }

    #[test]
    fn escape_like_neutralizes_wildcards() {
        assert_eq!(escape_like("100%_a\\b"), "100\\%\\_a\\\\b");
    }

    #[test]
    fn dump_and_load_preserve_items() {
        let mut connection = create().unwrap();
        let id = insert_item(
            &mut connection,
            NewItem::new(
                "note",
                Payload::Note {
                    text: "text".to_owned(),
                },
            ),
            10,
        )
        .unwrap();

        let bytes = dump(&connection).unwrap();
        let reloaded = load(&bytes).unwrap();

        assert_eq!(get_item(&reloaded, id).unwrap().summary.title, "note");
    }

    #[test]
    fn fields_keep_the_order_they_were_written_in() {
        let mut connection = create().unwrap();
        let id = insert_item(
            &mut connection,
            NewItem::new(
                "card",
                Payload::fields(
                    ItemKind::Card,
                    [
                        Field::secret("number", "4111"),
                        Field::public("holder", "Ada"),
                        Field::public("expiry", "01/29"),
                    ],
                ),
            ),
            10,
        )
        .unwrap();

        let Payload::Fields { fields, .. } = get_item(&connection, id).unwrap().payload else {
            panic!("expected a record made of fields");
        };
        let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
        assert_eq!(names, ["number", "holder", "expiry"]);
        assert!(fields[0].secret);
        assert!(!fields[1].secret);
    }

    #[test]
    fn a_secret_field_is_not_searchable() {
        let mut connection = create().unwrap();
        insert_item(
            &mut connection,
            NewItem::new(
                "mail",
                Payload::fields(
                    ItemKind::Login,
                    [
                        Field::public("login", "ada"),
                        Field::secret("password", "hunter2"),
                    ],
                ),
            ),
            10,
        )
        .unwrap();

        assert_eq!(
            search(&connection, &Query::all().text("ada"))
                .unwrap()
                .len(),
            1
        );
        assert!(
            search(&connection, &Query::all().text("hunter2"))
                .unwrap()
                .is_empty()
        );
    }
}
