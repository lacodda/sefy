//! How a secret goes onto the clipboard, and how long it stays there.
//!
//! The clipboard is the one place a secret leaves the vault for something else
//! to read, and every front end puts secrets there: the command line's `get`,
//! `otp`, `fill` and `gen`, the desktop app's Copy buttons and its quick
//! search. Both the number and the mechanism live here, so that all of them
//! take a secret back off at the same moment and mark it the same way - a
//! window that cleared after 20 seconds beside a terminal that cleared after
//! 45 would be two policies for one secret, and a window that kept secrets out
//! of the clipboard history beside a terminal that put them in would be no
//! policy at all.
//!
//! The mechanism is behind the `clipboard` feature: it is what a front end
//! needs, and a program that only reads vaults has no reason to link a
//! desktop's clipboard.

/// Seconds a copied secret stays on the clipboard before it is taken back.
///
/// Long enough to switch windows, find the field and paste; short enough that
/// a secret copied and forgotten is gone before the next thing is pasted.
pub const HOLD_SECONDS: u64 = 45;

#[cfg(feature = "clipboard")]
pub use system::{Clipboard, Held};

#[cfg(feature = "clipboard")]
mod system {
    use crate::{Error, Result};
    use zeroize::Zeroizing;

    /// The system clipboard, as a place for secrets.
    ///
    /// Everything put on it is marked as a secret, on every platform that has
    /// a way to say so (see [`Clipboard::put`]). The handle is meant to stay
    /// on the thread that opened it: on Windows the clipboard is one object
    /// for the whole desktop, and two threads working it at once can refuse
    /// each other or deadlock.
    pub struct Clipboard {
        inner: arboard::Clipboard,
    }

    /// What became of a secret held on the clipboard by [`Clipboard::hold`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Held {
        /// Whether the secret was taken back off before `hold` returned.
        pub cleared: bool,
    }

    impl Clipboard {
        /// Reaches the clipboard.
        pub fn open() -> Result<Self> {
            let inner = arboard::Clipboard::new().map_err(unreachable)?;
            Ok(Self { inner })
        }

        /// Puts a secret on the clipboard and leaves it there while this
        /// handle lives.
        ///
        /// Marked so that it goes no further than the paste it is for. On
        /// Windows a plain copy is also written into the clipboard history
        /// (Win+V), which keeps it after the clipboard itself is cleared, and
        /// into the cloud clipboard, which carries it to every machine signed
        /// in to the same account - so the copy says it may be neither, and
        /// that clipboard managers watching the clipboard should pass it by.
        /// On macOS it carries the marker clipboard history tools agree on,
        /// and on Linux the one desktop clipboard managers read.
        ///
        /// On X11 and Wayland the clipboard holds no copy of its own: the
        /// process that put the value there serves it to whoever pastes. The
        /// value is pastable for as long as this handle lives, and no longer.
        pub fn put(&mut self, value: &str) -> Result<()> {
            marked(&mut self.inner)
                .text(value.to_owned())
                .map_err(unwritable)
        }

        /// Whether the clipboard can be read, without putting anything on it.
        ///
        /// A test value written and taken back would land in whatever
        /// clipboard history the desktop keeps, so this only reads - which is
        /// what fails on a machine whose clipboard is there but unusable. An
        /// empty clipboard, or one holding something other than text, reads
        /// fine.
        pub fn check(&mut self) -> Result<()> {
            match self.inner.get_text().map(Zeroizing::new) {
                Ok(_) | Err(arboard::Error::ContentNotAvailable) => Ok(()),
                Err(error) => Err(Error::Clipboard(format!(
                    "the clipboard cannot be read: {error}"
                ))),
            }
        }

        /// Whether `value` is what the clipboard holds now.
        pub fn holds(&mut self, value: &str) -> bool {
            // What is read back is the secret again, and is wiped as such.
            matches!(self.inner.get_text().map(Zeroizing::new), Ok(current) if *current == value)
        }

        /// Takes `value` back off the clipboard, if it is still what is there,
        /// and says whether it did.
        ///
        /// Only a value that is still ours: clearing whatever the person
        /// copied in the meantime would be its own small betrayal.
        pub fn take_back(&mut self, value: &str) -> bool {
            self.holds(value) && self.inner.clear().is_ok()
        }

        /// Puts a secret on the clipboard, keeps it there for `seconds`, and
        /// takes it back - for a program that exits once the wait is over.
        ///
        /// Zero seconds leaves it there until something else replaces it.
        ///
        /// The platforms differ in more than syntax. On Windows and macOS the
        /// clipboard keeps the value itself, so this sets it, waits and
        /// clears it. On X11 and Wayland the wait *is* the mechanism: the
        /// value is served by this process, so it is served for `seconds`
        /// and then let go.
        pub fn hold(mut self, value: &str, seconds: u64) -> Result<Held> {
            #[cfg(all(
                unix,
                not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
            ))]
            {
                use arboard::SetExtLinux;
                // A zero wait means "leave it there" everywhere else, but here
                // letting go at once would mean it was never pastable at all,
                // so it becomes a long hold instead.
                const FOREVER_IN_PRACTICE: u64 = 8 * 60 * 60;
                let lifetime = if seconds == 0 {
                    FOREVER_IN_PRACTICE
                } else {
                    seconds
                };
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(lifetime);
                marked(&mut self.inner)
                    .wait_until(deadline)
                    .text(value.to_owned())
                    .map_err(unwritable)?;
                Ok(Held { cleared: true })
            }

            #[cfg(not(all(
                unix,
                not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
            )))]
            {
                self.put(value)?;
                if seconds == 0 {
                    return Ok(Held { cleared: false });
                }
                std::thread::sleep(std::time::Duration::from_secs(seconds));
                Ok(Held {
                    cleared: self.take_back(value),
                })
            }
        }
    }

    /// A write to the clipboard that marks what it writes as a secret.
    fn marked(clipboard: &mut arboard::Clipboard) -> arboard::Set<'_> {
        let set = clipboard.set();
        #[cfg(windows)]
        let set = {
            use arboard::SetExtWindows;
            // All three, as other password managers set them: the first is
            // the one Windows documents as covering the other two, the other
            // two are what the clipboard history and the cloud clipboard
            // themselves read, and a reader that knows only one of them still
            // finds it.
            set.exclude_from_monitoring()
                .exclude_from_history()
                .exclude_from_cloud()
        };
        #[cfg(target_os = "macos")]
        let set = {
            use arboard::SetExtApple;
            set.exclude_from_history()
        };
        #[cfg(all(
            unix,
            not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
        ))]
        let set = {
            use arboard::SetExtLinux;
            set.exclude_from_history()
        };
        set
    }

    fn unreachable(error: arboard::Error) -> Error {
        Error::Clipboard(format!("cannot reach the clipboard: {error}"))
    }

    fn unwritable(error: arboard::Error) -> Error {
        Error::Clipboard(format!("cannot write to the clipboard: {error}"))
    }
}
