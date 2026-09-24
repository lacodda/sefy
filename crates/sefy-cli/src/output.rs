//! Printing things, and the one path by which a secret leaves the program.

use anyhow::{Context, Result};
use sefy_core::{Error, ItemSummary};

/// Puts a secret on the clipboard, and takes it back off after `seconds`.
///
/// Zero seconds means leaving it there until something else overwrites it.
///
/// The two platform paths differ in more than syntax. On Windows and macOS the
/// clipboard holds the value itself, so this process can set it, wait, and
/// clear it. On X11 and Wayland the clipboard is *served* by the process that
/// owns the selection: once sefy exits, the value is gone whether or not a
/// timer fired. There the wait is the mechanism — sefy keeps serving the
/// selection for the timeout and then lets go.
pub fn to_clipboard(value: &str, seconds: u64) -> Result<ClipboardHold> {
    Clipboard::open()?.hold(value, seconds)
}

/// The clipboard, held open across several values.
///
/// `sefy fill` puts one field after another on it; on X11 and Wayland the
/// value is served by this process, so the handle has to outlive every one of
/// them rather than be opened and dropped per field.
pub struct Clipboard {
    inner: arboard::Clipboard,
}

impl Clipboard {
    /// Reaches the clipboard, or says how to do without it.
    pub fn open() -> Result<Self> {
        let inner = arboard::Clipboard::new().context(
            "cannot reach the clipboard\n\
             use --stdout to print the value instead",
        )?;
        Ok(Self { inner })
    }

    /// Puts a value on the clipboard and leaves it there while this handle
    /// lives.
    pub fn put(&mut self, value: &str) -> Result<()> {
        self.inner
            .set_text(value.to_owned())
            .context("cannot write to the clipboard")
    }

    /// Takes `value` back off the clipboard, if it is still what is there.
    pub fn take_back(&mut self, value: &str) {
        if matches!(self.inner.get_text(), Ok(current) if current == value) {
            let _ = self.inner.clear();
        }
    }

    /// Puts the last value on the clipboard and keeps it there for `seconds`.
    pub fn hold(self, value: &str, seconds: u64) -> Result<ClipboardHold> {
        hold(self.inner, value, seconds)
    }
}

fn hold(mut clipboard: arboard::Clipboard, value: &str, seconds: u64) -> Result<ClipboardHold> {
    #[cfg(target_os = "linux")]
    {
        use arboard::SetExtLinux;
        // `wait_until` hands the selection to the desktop and serves it until
        // the deadline; without a wait of some kind the value would vanish the
        // moment this process exits.
        let deadline = std::time::Instant::now() + clipboard_lifetime(seconds);
        clipboard
            .set()
            .wait_until(deadline)
            .text(value.to_owned())
            .context("cannot write to the clipboard")?;
        Ok(ClipboardHold { cleared: true })
    }

    #[cfg(not(target_os = "linux"))]
    {
        clipboard
            .set_text(value.to_owned())
            .context("cannot write to the clipboard")?;

        if seconds == 0 {
            return Ok(ClipboardHold { cleared: false });
        }

        std::thread::sleep(std::time::Duration::from_secs(seconds));
        // Only clear what is still ours: overwriting whatever the user copied
        // in the meantime would be its own small betrayal.
        match clipboard.get_text() {
            Ok(current) if current == value => {
                let _ = clipboard.clear();
                Ok(ClipboardHold { cleared: true })
            }
            _ => Ok(ClipboardHold { cleared: false }),
        }
    }
}

/// How long a secret stays on the clipboard, with a floor under it.
///
/// A zero timeout means "leave it there" everywhere else, but on Linux letting
/// go immediately would mean the value was never pastable at all. There it
/// becomes a long hold instead.
#[cfg(target_os = "linux")]
fn clipboard_lifetime(seconds: u64) -> std::time::Duration {
    const FOREVER_IN_PRACTICE: u64 = 8 * 60 * 60;
    std::time::Duration::from_secs(if seconds == 0 {
        FOREVER_IN_PRACTICE
    } else {
        seconds
    })
}

/// What became of a secret that was put on the clipboard.
pub struct ClipboardHold {
    /// Whether sefy took the value back off the clipboard before returning.
    pub cleared: bool,
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
fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// `n` followed by `noun`, pluralised the lazy English way.
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("{n} {noun}")
    } else {
        format!("{n} {noun}s")
    }
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
