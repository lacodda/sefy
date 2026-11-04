//! The side of the socket that asks.
//!
//! One connection per question: a command runs for a moment or for the time it
//! takes someone to type a note, and an agent it had kept a connection to may
//! have locked, or been replaced, long before the next one.

use super::Seat;
use super::keeper::LockState;
use super::socket::{self, Address, PATIENCE};
use super::wire::{self, Answer, Request};
use crate::crypto::{DerivedKey, SALT_LEN};
use crate::error::{Error, Result};
use crate::keys::{FreshKey, KeySource};
use std::sync::mpsc;
use std::time::Duration;
use zeroize::Zeroizing;

/// Someone to ask, in one seat. Nobody need be there.
#[derive(Debug, Clone)]
pub struct Client {
    seat: Seat,
    address: Address,
    patience: Duration,
    looking: bool,
}

impl Client {
    /// Whoever sits in `seat`; `None` when there is nowhere it could listen.
    pub fn of(seat: Seat) -> Option<Self> {
        Some(Self {
            seat,
            address: Address::of(seat)?,
            patience: PATIENCE,
            looking: false,
        })
    }

    /// This, only looking: the keys it asks for do not count as anyone using
    /// the vault. For a status, which a prompt can ask for every minute.
    pub fn only_looking(mut self) -> Self {
        self.looking = true;
        self
    }

    /// This, waiting no longer than `patience` for any answer.
    pub fn with_patience(mut self, patience: Duration) -> Self {
        self.patience = patience;
        self
    }

    /// The seat this asks.
    pub fn seat(&self) -> Seat {
        self.seat
    }

    /// What the agent there says about itself; `None` when nobody answers.
    pub fn state(&self) -> Option<LockState> {
        match self.ask(&Request::State)? {
            Answer::Locked => Some(LockState::Locked),
            Answer::Unlocked { left, idle } => Some(LockState::Unlocked { left, idle }),
            _ => None,
        }
    }

    /// Tells the agent there to wipe everything. Says whether it held
    /// anything; `None` when nobody answers.
    pub fn lock(&self) -> Option<bool> {
        match self.ask(&Request::Lock)? {
            Answer::LockedNow(had) => Some(had),
            _ => None,
        }
    }

    fn ask(&self, request: &Request) -> Option<Answer> {
        ask(&self.address, request, self.patience)
    }
}

impl KeySource for Client {
    fn key_for(&self, salt: &[u8; SALT_LEN]) -> Result<DerivedKey> {
        let request = if self.looking {
            Request::Peek(*salt)
        } else {
            Request::Key(*salt)
        };
        match self.ask(&request) {
            Some(Answer::Key(key)) => Ok(key),
            _ => Err(Error::KeysGone),
        }
    }

    fn fresh(&self, opened_with: &[u8; SALT_LEN]) -> Result<FreshKey> {
        match self.ask(&Request::Fresh(*opened_with)) {
            Some(Answer::Fresh(fresh)) => Ok(fresh),
            _ => Err(Error::KeysGone),
        }
    }
}

/// Whether anyone answers at `address`.
pub(super) fn answers_at(address: &Address) -> bool {
    ask(address, &Request::State, PATIENCE).is_some()
}

/// One question and its answer. `None` for anything else: nobody there, no
/// answer in time, an answer that is not one. All of those mean the same to
/// whoever asked, which is that there is no agent to rely on.
///
/// On a thread of its own, since named pipes have no read timeout: an agent
/// that stopped answering must cost a command a moment and a question for the
/// password, not the command. If the time runs out the thread is left behind,
/// and goes with the process.
fn ask(address: &Address, request: &Request, patience: Duration) -> Option<Answer> {
    let (answer, answered) = mpsc::channel();
    let address = address.clone();
    let request = request.encode();
    std::thread::Builder::new()
        .name("agent question".to_owned())
        .spawn(move || {
            let _ = answer.send(exchange(&address, &request));
        })
        .ok()?;
    answered.recv_timeout(patience).ok().flatten()
}

fn exchange(address: &Address, request: &Zeroizing<Vec<u8>>) -> Option<Answer> {
    let stream = socket::connect(address).ok()?;
    wire::send(&stream, request).ok()?;
    let answer = wire::receive(&stream).ok()?;
    Answer::decode(&answer)
}
