//! Passwords as rows under a header: what Chrome, Edge, Firefox, Safari and
//! most password managers export, and what browsers import.
//!
//! Columns are recognised by name rather than by which program wrote the
//! file. Chrome writes `name,url,username,password,note`, Firefox
//! `url,username,password,…,guid,timeCreated,…`, Safari
//! `Title,URL,Username,Password,Notes,OTPAuth`, Bitwarden
//! `folder,…,name,notes,fields,…,login_uri,login_username,login_password,login_totp`
//! — one vocabulary covers them all, and a program not listed here is read
//! just as well if it names its columns the way everyone does.
//!
//! A column outside the vocabulary becomes a field of the record, because
//! dropping what the file holds is worse than an untidy record. Columns that
//! are a browser's own bookkeeping — Firefox's `httpRealm`, a favourite flag —
//! are recognised as such, left out, and named in the report.

use super::{
    Entry, ExportReport, Exported, Outcome, Reading, Record, UNREADABLE_TOTP, gather, leave_out,
    normalize_uuid, title_or,
};
use crate::error::{Error, Result};
use crate::model::{ItemKind, NewItem, Payload};
use crate::otp;
use crate::time;
use crate::vault::Vault;
use zeroize::Zeroizing;

const FORMAT: &str = "a CSV of passwords";

/// What a recognised column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Title,
    Url,
    Login,
    Email,
    Password,
    Notes,
    Totp,
    /// A folder: one tag, its path kept whole.
    Folder,
    /// A KeePass group path, which starts with the root group's name: one
    /// tag, the path without the root, as a KeePass XML import makes it.
    Group,
    /// Tags, several to a cell.
    Tags,
    /// Whether the row is a login or a note.
    Type,
    /// An identity worth keeping: Firefox's guid.
    Guid,
    Created,
    Updated,
    /// Bitwarden's custom fields, one `name: value` to a line.
    Fields,
    /// Bookkeeping of the program that wrote the file, left out.
    Ignored,
    /// Anything else: kept as a field under the column's own name.
    Other,
}

/// Recognises a column by its header, whatever its case and separators.
fn column(header: &str) -> Column {
    let name: String = header
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    match name.as_str() {
        "name" | "title" => Column::Title,
        "url" | "uri" | "website" | "origin" | "loginuri" | "hostname" | "weburl" => Column::Url,
        "username" | "user" | "login" | "loginusername" | "account" => Column::Login,
        "email" => Column::Email,
        "password" | "loginpassword" => Column::Password,
        "note" | "notes" | "extra" | "comment" | "comments" => Column::Notes,
        "totp" | "otp" | "otpauth" | "logintotp" => Column::Totp,
        "folder" | "grouping" => Column::Folder,
        "group" => Column::Group,
        "tags" => Column::Tags,
        "type" => Column::Type,
        "guid" => Column::Guid,
        "created" | "timecreated" | "createtime" => Column::Created,
        "lastmodified" | "modifytime" | "timepasswordchanged" => Column::Updated,
        "fields" => Column::Fields,
        "httprealm" | "formactionorigin" | "timelastused" | "favorite" | "fav" | "reprompt"
        | "archived" | "icon" | "vault" => Column::Ignored,
        _ => Column::Other,
    }
}

/// Reads a CSV of passwords into entries.
pub(super) fn read(body: &str) -> Result<Reading> {
    let mut reader = ::csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(body.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|error| unreadable(error.to_string()))?
        .iter()
        .map(str::to_owned)
        .collect();
    let columns: Vec<Column> = headers.iter().map(|header| column(header)).collect();

    // A header has to name something a password lives in; anything else is a
    // spreadsheet of something else, and importing its rows as logins would
    // fill the vault with nonsense.
    if !columns
        .iter()
        .any(|column| matches!(column, Column::Password | Column::Login | Column::Url))
    {
        return Err(Error::UnrecognizedImport);
    }
    let has_login = columns.contains(&Column::Login);

    let mut reading = Reading {
        columns_left_out: headers
            .iter()
            .zip(&columns)
            .filter(|(_, column)| **column == Column::Ignored)
            .map(|(header, _)| header.clone())
            .collect(),
        ..Reading::default()
    };

    for (row, record) in reader.records().enumerate() {
        let record = record.map_err(|error| unreadable(error.to_string()))?;
        if record.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }
        let cell = |wanted: Column| -> &str {
            columns
                .iter()
                .position(|column| *column == wanted)
                .and_then(|index| record.get(index))
                .unwrap_or("")
        };

        let login = if has_login {
            cell(Column::Login)
        } else {
            cell(Column::Email)
        };
        let title = title_or(cell(Column::Title), cell(Column::Url), login);
        let kind = cell(Column::Type).trim().to_lowercase();
        let is_note = matches!(kind.as_str(), "note" | "securenote" | "secure note");

        let payload = if is_note {
            Payload::Note {
                text: cell(Column::Notes).to_owned(),
            }
        } else {
            let mut item = Record::new(ItemKind::Login);
            item.put("login", login, None);
            item.put("password", cell(Column::Password), None);
            item.put("url", cell(Column::Url), None);
            if !item.put_totp(cell(Column::Totp)) {
                reading.notice(&title, Outcome::Reshaped, UNREADABLE_TOTP);
            }
            item.put("notes", cell(Column::Notes), None);
            if has_login {
                item.put("email", cell(Column::Email), Some(false));
            }
            for line in cell(Column::Fields).lines() {
                if let Some((name, value)) = line.split_once(": ") {
                    item.put(name, value, None);
                }
            }
            for ((header, column), value) in headers.iter().zip(&columns).zip(record.iter()) {
                if *column == Column::Other {
                    item.put(header, value, None);
                }
            }
            match item.into_payload() {
                // A row holding nothing but notes is a note, as a KeePass
                // entry holding nothing but notes is - unless the file says
                // it is a login.
                Some(Payload::Fields { fields, .. })
                    if kind != "login" && fields.len() == 1 && fields[0].name == "notes" =>
                {
                    Payload::Note {
                        text: fields[0].value.clone(),
                    }
                }
                Some(payload) => payload,
                None => {
                    reading.notice(
                        &title,
                        Outcome::NotImported,
                        format!("row {}: nothing in it but a name", row + 2),
                    );
                    continue;
                }
            }
        };

        let mut tags = Vec::new();
        let folder = cell(Column::Folder).trim();
        if !folder.is_empty() {
            tags.push(folder.to_owned());
        }
        if let Some((_, path)) = cell(Column::Group).trim().split_once('/')
            && !path.trim().is_empty()
        {
            tags.push(path.trim().to_owned());
        }
        tags.extend(
            cell(Column::Tags)
                .split([',', ';'])
                .map(|tag| tag.trim().to_owned())
                .filter(|tag| !tag.is_empty()),
        );

        let mut entry = Entry::new(
            normalize_uuid(cell(Column::Guid)),
            NewItem::new(title, payload).with_tags(tags),
        );
        entry.created_at = moment(cell(Column::Created));
        entry.updated_at = moment(cell(Column::Updated));
        reading.push(entry);
    }
    Ok(reading)
}

fn unreadable(reason: String) -> Error {
    Error::UnreadableImport {
        format: FORMAT,
        reason,
    }
}

/// A time in a cell: Unix milliseconds (Firefox), Unix seconds, or a date.
fn moment(cell: &str) -> Option<i64> {
    let cell = cell.trim();
    if cell.is_empty() {
        return None;
    }
    match cell.parse::<i64>() {
        // Past 1e11 seconds is the year 5138; a number that large is
        // milliseconds.
        Ok(number) if number > 100_000_000_000 => Some(number / 1000),
        Ok(number) => Some(number),
        Err(_) => time::parse_rfc3339(cell),
    }
}

/// The columns a CSV export is written with: Chrome's, which every browser
/// and password manager imports, and one more for a one-time password key.
const HEADER: [&str; 6] = ["name", "url", "username", "password", "note", "totp"];

/// Writes the vault's logins as a CSV.
///
/// A CSV has a row per login and a column per field it knows; a note, a card
/// or a file has no row to go in, and a login's own extra fields no column.
/// Both are counted in the report rather than written somewhere they would be
/// misread.
pub(super) fn write(vault: &Vault) -> Result<Exported> {
    let mut report = ExportReport::default();
    let mut writer = ::csv::WriterBuilder::new().from_writer(Vec::new());
    writer.write_record(HEADER).map_err(written)?;

    for outgoing in gather(vault, false)? {
        let fields = match &outgoing.payload {
            Payload::Fields {
                kind: ItemKind::Login,
                fields,
            } => fields,
            Payload::Unknown { .. } => {
                report.unreadable += 1;
                continue;
            }
            other => {
                leave_out(&mut report, &other.kind());
                continue;
            }
        };

        let value = |name: &str| {
            fields
                .iter()
                .find(|field| field.name == name)
                .map_or("", |field| field.value.as_str())
        };
        let row = [
            outgoing.summary.title.as_str(),
            value("url"),
            value("login"),
            value("password"),
            value("notes"),
            value(otp::FIELD),
        ];
        writer.write_record(row).map_err(written)?;
        report.written += 1;
        if fields.iter().any(|field| {
            !matches!(
                field.name.as_str(),
                "login" | "password" | "url" | "notes" | otp::FIELD
            )
        }) {
            report.trimmed += 1;
        }
    }

    let known = ItemKind::known();
    report.left_out.sort_by_key(|(kind, _)| {
        known
            .iter()
            .position(|candidate| candidate == kind)
            .unwrap_or(known.len())
    });

    let bytes = writer
        .into_inner()
        .map_err(|error| written(error.into_error()))?;
    let text = String::from_utf8(bytes).map_err(|_| Error::Unexportable {
        title: String::new(),
        format: "CSV",
        reason: "the result is not UTF-8".to_owned(),
    })?;
    Ok(Exported {
        text: Zeroizing::new(text),
        report,
    })
}

fn written(error: impl std::fmt::Display) -> Error {
    Error::Unexportable {
        title: String::new(),
        format: "CSV",
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_are_recognised_whatever_their_spelling() {
        assert_eq!(column("login_uri"), Column::Url);
        assert_eq!(column("Login URI"), Column::Url);
        assert_eq!(column("User Name"), Column::Login);
        assert_eq!(column("OTPAuth"), Column::Totp);
        assert_eq!(column("httpRealm"), Column::Ignored);
        assert_eq!(column("Security question"), Column::Other);
    }

    #[test]
    fn a_moment_is_read_in_any_of_the_shapes_exports_use() {
        assert_eq!(moment("1709164800000"), Some(1_709_164_800));
        assert_eq!(moment("1709164800"), Some(1_709_164_800));
        assert_eq!(moment("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(moment(""), None);
        assert_eq!(moment("soon"), None);
    }
}
