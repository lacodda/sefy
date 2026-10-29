//! Printing things, and the one path by which a secret leaves the program.

use anyhow::Result;
use sefy_core::clipboard::{Clipboard, Held};
use sefy_core::{Error, ItemSummary};

/// Puts a secret on the clipboard, and takes it back off after `seconds`.
///
/// Zero seconds means leaving it there until something else overwrites it.
/// How it is put there - marked so that no clipboard history keeps it, held
/// by this process where the desktop needs that - is the core's
/// (`sefy_core::clipboard`), shared with the desktop app.
pub fn to_clipboard(value: &str, seconds: u64) -> Result<Held> {
    Ok(open_clipboard()?.hold(value, seconds)?)
}

/// Reaches the clipboard, or says how to do without it.
pub fn open_clipboard() -> Result<Clipboard> {
    Clipboard::open().map_err(|error| {
        anyhow::anyhow!(
            "{error}
use --stdout to print the value instead"
        )
    })
}

/// Prints a table of items: id, title, kind and tags.
pub fn table(items: &[ItemSummary]) {
    if items.is_empty() {
        println!("no items");
        return;
    }

    let id_width = items
        .iter()
        .map(|item| item.id.to_string().len())
        .max()
        .unwrap_or(2);
    let title_width = items
        .iter()
        .map(|item| item.title.chars().count())
        .max()
        .unwrap_or(5)
        .min(40);

    // A kind this build does not know is named as it was stored and marked, so
    // a listing shows the item plainly instead of hiding it — and says why
    // `sefy get` will not open it.
    let kind_width = items
        .iter()
        .map(|item| item.kind.as_str().chars().count())
        .max()
        .unwrap_or(10)
        .max(10);

    for item in items {
        let tags = if item.tags.is_empty() {
            String::new()
        } else {
            format!("  [{}]", item.tags.join(", "))
        };
        let unknown = if item.kind.is_known() {
            ""
        } else {
            "  (needs a newer sefy)"
        };
        println!(
            "{:>id_width$}  {:<title_width$}  {:<kind_width$}{}{}",
            item.id,
            truncate(&item.title, title_width),
            item.kind.as_str(),
            tags,
            unknown,
        );
    }
}

/// Shortens a title to fit the column, marking that it was cut.
pub(crate) fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// `n` followed by `noun`, pluralised the lazy English way.
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        return format!("{n} {noun}");
    }
    // "copy" becomes "copies", while "key" stays "keys": a y after a
    // consonant is the one English plural the plain -s gets wrong here.
    let mut letters = noun.chars().rev();
    if letters.next() == Some('y') && letters.next().is_some_and(|c| !"aeiou".contains(c)) {
        return format!("{n} {}ies", &noun[..noun.len() - 1]);
    }
    format!("{n} {noun}s")
}

/// Turns a core error into the message the user should see.
///
/// An ambiguous reference is the interesting case: rather than a bare
/// complaint, it lists what the input could have meant so the next command can
/// be exact.
pub fn explain(error: Error) -> anyhow::Error {
    match error {
        Error::Ambiguous {
            reference,
            candidates,
        } => {
            let mut message = format!("{} items match {reference:?}:\n", candidates.len());
            for item in &candidates {
                message.push_str(&format!(
                    "  {:>4}  {:<30}  {}\n",
                    item.id,
                    truncate(&item.title, 30),
                    item.kind.as_str()
                ));
            }
            message.push_str("narrow the text, or use an id");
            anyhow::anyhow!(message)
        }
        Error::NotFound(reference) => {
            anyhow::anyhow!("nothing matches {reference:?}")
        }
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_text_and_marks_cuts() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactly-ten", 11), "exactly-ten");
        assert_eq!(truncate("much too long to fit", 8), "much to…");
    }

    #[test]
    fn truncate_counts_characters_not_bytes() {
        // Multi-byte test data on purpose: each of these characters is two
        // bytes in UTF-8, so a byte-based truncation would both cut in the
        // wrong place and be able to split a character in half.
        assert_eq!(truncate("паспорт", 7), "паспорт");
        assert_eq!(truncate("паспорт", 4), "пас…");
    }
}
