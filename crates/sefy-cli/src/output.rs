//! Printing things, and the one path by which a secret leaves the program.

use anyhow::{Context, Result};
use sefy_core::{Error, ItemSummary};

/// Puts a secret on the clipboard.
///
/// On X11 and Wayland the clipboard is served by the process that owns it, so
/// the value has to be handed to the desktop before this program exits;
/// `arboard` does that on Linux via its wait-for-paste path.
pub fn to_clipboard(value: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context(
        "cannot reach the clipboard\n\
         use --stdout to print the value instead",
    )?;

    #[cfg(target_os = "linux")]
    {
        use arboard::SetExtLinux;
        clipboard
            .set()
            .wait()
            .text(value.to_owned())
            .context("cannot write to the clipboard")?;
    }
    #[cfg(not(target_os = "linux"))]
    {
        clipboard
            .set_text(value.to_owned())
            .context("cannot write to the clipboard")?;
    }
    Ok(())
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

    for item in items {
        let tags = if item.tags.is_empty() {
            String::new()
        } else {
            format!("  [{}]", item.tags.join(", "))
        };
        println!(
            "{:>id_width$}  {:<title_width$}  {:<10}{}",
            item.id,
            truncate(&item.title, title_width),
            item.kind.as_str(),
            tags,
        );
    }
}

/// Shortens a title to fit the column, marking that it was cut.
fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
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
        assert_eq!(truncate("паспорт", 7), "паспорт");
        assert_eq!(truncate("паспорт", 4), "пас…");
    }
}
