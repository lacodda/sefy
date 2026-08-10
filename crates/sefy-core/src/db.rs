//! The database living inside the encrypted blob.
//!
//! The connection is always in-memory: bytes arrive decrypted, are handed to
//! SQLite's deserialize interface, and go back out through serialize. SQLite is
//! never pointed at a path, so no plaintext page ever reaches the disk.

use crate::error::{Error, Result};
use crate::model::{Credential, Item, ItemKind, ItemSummary, NewItem, Payload, Query};
use rusqlite::{Connection, MAIN_DB, OptionalExtension, params};
use std::io::Cursor;

/// Schema version of the database inside the blob.
///
/// Distinct from the file format version: the envelope can stay the same while
/// the tables under it grow.
const SCHEMA_VERSION: i64 = 1;

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
    if version >= SCHEMA_VERSION {
        return Ok(());
    }

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
    connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

/// Inserts an item and returns its identifier.
pub fn insert_item(connection: &mut Connection, item: NewItem, now: i64) -> Result<i64> {
    let transaction = connection.transaction()?;
    let kind = item.payload.kind();

    transaction.execute(
        "INSERT INTO items (title, kind, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![item.title, kind.as_str(), now],
    )?;
    let id = transaction.last_insert_rowid();

    insert_payload(&transaction, id, &item.payload)?;
    set_tags(&transaction, id, &item.tags)?;

    transaction.commit()?;
    Ok(id)
}

fn insert_payload(connection: &Connection, id: i64, payload: &Payload) -> Result<()> {
    match payload {
        Payload::Note { text } => {
            connection.execute(
                "INSERT INTO notes (item_id, text) VALUES (?1, ?2)",
                params![id, text],
            )?;
        }
        Payload::Credential(credential) => {
            connection.execute(
                "INSERT INTO credentials (item_id, login, password, url, totp, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    credential.login,
                    credential.password,
                    credential.url,
                    credential.totp,
                    credential.notes
                ],
            )?;
        }
        Payload::File { filename, bytes } => {
            connection.execute(
                "INSERT INTO files (item_id, filename, bytes) VALUES (?1, ?2, ?3)",
                params![id, filename, bytes],
            )?;
        }
    }
    Ok(())
}

fn delete_payload(connection: &Connection, id: i64) -> Result<()> {
    connection.execute("DELETE FROM notes WHERE item_id = ?1", params![id])?;
    connection.execute("DELETE FROM credentials WHERE item_id = ?1", params![id])?;
    connection.execute("DELETE FROM files WHERE item_id = ?1", params![id])?;
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
        if kind.as_str() != existing_kind {
            return Err(Error::ItemKindMismatch {
                id,
                actual: ItemKind::parse(&existing_kind)
                    .map(ItemKind::as_str)
                    .unwrap_or("unknown"),
                expected: kind.as_str(),
            });
        }
        delete_payload(&transaction, id)?;
        insert_payload(&transaction, id, &payload)?;
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
        ItemKind::Credential => connection.query_row(
            "SELECT login, password, url, totp, notes FROM credentials WHERE item_id = ?1",
            params![id],
            |row| {
                Ok(Payload::Credential(Credential {
                    login: row.get(0)?,
                    password: row.get(1)?,
                    url: row.get(2)?,
                    totp: row.get(3)?,
                    notes: row.get(4)?,
                }))
            },
        )?,
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
    };
    Ok(Item { summary, payload })
}

/// Reads one item without its payload.
pub fn get_summary(connection: &Connection, id: i64) -> Result<ItemSummary> {
    let row = connection
        .query_row(
            "SELECT id, title, kind, created_at, updated_at FROM items WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?;
    let (id, title, kind, created_at, updated_at) = row.ok_or(Error::ItemNotFound(id))?;

    Ok(ItemSummary {
        id,
        title,
        kind: ItemKind::parse(&kind).ok_or(Error::ItemNotFound(id))?,
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
         LEFT JOIN notes n       ON n.item_id = i.id
         LEFT JOIN credentials c ON c.item_id = i.id
         WHERE 1 = 1",
    );
    let mut arguments: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(kind) = query.kind {
        arguments.push(Box::new(kind.as_str().to_owned()));
        sql.push_str(&format!(" AND i.kind = ?{}", arguments.len()));
    }

    if let Some(text) = query
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        // Attachment bytes are deliberately left out: a blob match would say
        // nothing useful and would mean scanning every file in the vault.
        arguments.push(Box::new(format!("%{}%", escape_like(text))));
        let placeholder = arguments.len();
        sql.push_str(&format!(
            " AND (i.title LIKE ?{p} ESCAPE '\\'
                OR n.text  LIKE ?{p} ESCAPE '\\'
                OR c.login LIKE ?{p} ESCAPE '\\'
                OR c.url   LIKE ?{p} ESCAPE '\\'
                OR c.notes LIKE ?{p} ESCAPE '\\')",
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
}
