//! Holding the master password in one process, so that others need not ask.
//!
//! An agent is a process that was given the password once and answers for it
//! over a local socket, until it locks: after a stretch of idleness, at a hard
//! limit from the moment it was given the password, or when told to. Two kinds
//! of process do it, each in a [`Seat`] of its own: `sefy agent`, and the
//! desktop app while a vault is unlocked in it.
//!
//! # What crosses the socket
//!
//! Keys, one version of one file at a time. Never the password: it reaches an
//! agent from a terminal or from the app's Unlock screen. Never the database:
//! whoever asks opens the file itself. A client sends nothing but a salt, the
//! first sixteen bytes of a file it already has, so there is nothing to learn
//! by posing as an agent; and the wrong key fails the file's authentication
//! tag, so there is nothing to gain by it either. The file authenticates the
//! agent.
//!
//! # What it does not protect from
//!
//! Anything running as the same user can connect, and can read this process's
//! memory unless the system stops it. The socket keeps other users out; it
//! cannot tell one of your programs from another.

mod client;
mod keeper;
mod server;
mod socket;
mod wire;

pub use client::Client;
pub use keeper::{CAP, IDLE, Keeper, LockState};
pub use server::{Taken, leave, serve, start_ticking};
pub use socket::SOCKET_ENV;

/// Where on the socket an agent listens.
///
/// Two, so that one kind cannot hide the other: the app unlocked next to a
/// running `sefy agent` would otherwise find the name taken, go unheard, and
/// stay unlocked through a `sefy lock` that reported success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seat {
    /// `sefy agent`.
    Agent,
    /// The desktop app.
    Window,
}

impl Seat {
    /// Every seat, in the order a client asks them.
    pub const ALL: [Self; 2] = [Self::Agent, Self::Window];

    /// Its name, in the socket's name and in what a person reads.
    pub fn name(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Window => "window",
        }
    }
}
