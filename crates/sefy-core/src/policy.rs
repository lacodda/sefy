//! How long what a vault lets out stays out: how long it stays unlocked with
//! nobody using it, and how long a copied secret stays on the clipboard.
//!
//! One answer per vault, kept inside it. Every front end reads it from there, so
//! the window and the command line cannot disagree about a vault, and a setting
//! needs no file of its own: a settings file in a predictable place under the
//! profile is one more thing saying sefy was here, and sefy keeps nothing on disk
//! but the vault, its copies and its plugins.
//!
//! It stays with the file it was set on. A merge folds items, not facts about the
//! vault, so a laptop that locks in a minute and a desktop that locks in an hour
//! can be two copies of one vault.

/// What a vault says when nothing was ever set: how long it stays unlocked with
/// nobody using it, in seconds.
pub const LOCK_AFTER_SECONDS: u64 = 15 * 60;

/// What a vault says when nothing was ever set: how long a copied secret stays
/// on the clipboard, in seconds.
///
/// Long enough to switch windows, find the field and paste; short enough that a
/// secret copied and forgotten is gone before the next thing is pasted.
pub const CLIPBOARD_SECONDS: u64 = 45;

/// What a vault may be set to lock after, in seconds. Twelve hours is as long as
/// anything holds a password, whatever it is told ([`crate::agent`]).
pub const LOCK_AFTER_CHOICES: [u64; 6] =
    [60, 5 * 60, LOCK_AFTER_SECONDS, 30 * 60, 60 * 60, 4 * 3600];

/// What a vault may be set to hold a copied secret for, in seconds. Never for
/// good, which only `--clear-after 0` can ask for, one copy at a time.
pub const CLIPBOARD_CHOICES: [u64; 5] = [15, 30, CLIPBOARD_SECONDS, 90, 180];

/// `meta` key for how long the vault stays unlocked.
pub(crate) const LOCK_AFTER_KEY: &str = "lock_after_seconds";

/// `meta` key for how long a copied secret stays on the clipboard.
pub(crate) const CLIPBOARD_KEY: &str = "clipboard_seconds";

/// How long what a vault lets out stays out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// Seconds the vault stays unlocked with nobody using it.
    pub lock_after_seconds: u64,
    /// Seconds a copied secret stays on the clipboard.
    pub clipboard_seconds: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            lock_after_seconds: LOCK_AFTER_SECONDS,
            clipboard_seconds: CLIPBOARD_SECONDS,
        }
    }
}

impl Policy {
    /// Whether both values are among those a vault can be set to.
    pub fn is_offered(&self) -> bool {
        LOCK_AFTER_CHOICES.contains(&self.lock_after_seconds)
            && CLIPBOARD_CHOICES.contains(&self.clipboard_seconds)
    }

    /// A stored value read back: a number above zero, or the default.
    ///
    /// Not held to the choices above, which a newer sefy may have more of: what
    /// it set is what this one obeys. Zero is the one thing that is not kept,
    /// since a vault that locks at once or a secret gone before the paste is no
    /// one's setting.
    pub(crate) fn read(stored: Option<String>, default: u64) -> u64 {
        stored
            .and_then(|value| value.parse().ok())
            .filter(|seconds| *seconds > 0)
            .unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_among_the_choices() {
        assert!(Policy::default().is_offered());
    }

    #[test]
    fn what_is_not_offered_is_not_offered() {
        let mut policy = Policy::default();
        policy.lock_after_seconds = 1;
        assert!(!policy.is_offered());

        let mut policy = Policy::default();
        policy.clipboard_seconds = 0;
        assert!(!policy.is_offered());
    }

    #[test]
    fn a_stored_value_comes_back_unless_it_means_nothing() {
        assert_eq!(Policy::read(Some("90".into()), 45), 90);
        // Not one of today's choices: a newer sefy set it, and it is obeyed.
        assert_eq!(Policy::read(Some("7".into()), 45), 7);
        assert_eq!(Policy::read(Some("0".into()), 45), 45);
        assert_eq!(Policy::read(Some("soon".into()), 45), 45);
        assert_eq!(Policy::read(None, 45), 45);
    }
}
