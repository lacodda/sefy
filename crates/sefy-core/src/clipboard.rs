//! How long a secret stays on the clipboard.
//!
//! The clipboard is the one place a secret leaves the vault for something else
//! to read, and every front end puts secrets there: the command line's `get`,
//! `otp`, `fill` and `gen`, the desktop app's Copy buttons. The number lives
//! here so that all of them take it back off at the same moment — a window
//! that cleared after 20 seconds beside a terminal that cleared after 45 would
//! be two policies for one secret, and the person would learn the shorter one.

/// Seconds a copied secret stays on the clipboard before it is taken back.
///
/// Long enough to switch windows, find the field and paste; short enough that
/// a secret copied and forgotten is gone before the next thing is pasted.
pub const HOLD_SECONDS: u64 = 45;
