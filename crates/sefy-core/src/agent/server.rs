//! The agent's side of the socket.

use super::Seat;
use super::keeper::Keeper;
use super::socket::{self, Address};
use super::wire::{self, Answer, Request};
use crate::keys::KeySource;
use interprocess::local_socket::traits::Listener as _;
use interprocess::local_socket::{Listener, Stream};
use std::io;
use std::sync::Arc;
use std::time::Duration;

/// How often the clock looks.
///
/// A short tick rather than one long sleep until the deadline: a sleep measured
/// on a clock that stops while the machine does would run on past it. A second
/// is as late as the password can outlive its time, and costs nothing.
const TICK: Duration = Duration::from_secs(1);

/// Runs the clock of a keeper for as long as the process lives.
pub fn start_ticking(keeper: Arc<Keeper>) {
    std::thread::Builder::new()
        .name("agent clock".to_owned())
        .spawn(move || {
            loop {
                std::thread::sleep(TICK);
                keeper.tick();
            }
        })
        .expect("the clock thread starts");
}

/// Why a seat could not be taken.
#[derive(Debug, thiserror::Error)]
pub enum Taken {
    /// Someone already answers there.
    #[error("an agent is already running")]
    Occupied,
    /// The system would not have it.
    #[error("cannot listen for the command line: {0}")]
    Refused(#[from] io::Error),
    /// There is nowhere to listen: no directory to put a socket in.
    #[error("cannot listen for the command line: there is no directory to listen in")]
    Nowhere,
}

/// Starts answering in `seat` for `keeper`, for as long as the process lives.
///
/// Returns once the socket is there, so that whoever waits for an agent can be
/// told it is ready.
pub fn serve(seat: Seat, keeper: Arc<Keeper>) -> Result<(), Taken> {
    let address = Address::of(seat).ok_or(Taken::Nowhere)?;
    let listener = socket::listen(&address).map_err(|error| {
        // Refused for being taken, or for a reason of its own: asking is the
        // only way to tell on both systems.
        if super::client::answers_at(&address) {
            Taken::Occupied
        } else {
            Taken::Refused(error)
        }
    })?;
    std::thread::Builder::new()
        .name("agent".to_owned())
        .spawn(move || accept(listener, keeper))
        .map_err(Taken::Refused)?;
    Ok(())
}

/// Removes what a seat left on disk, on the way out of a process that does not
/// run its destructors.
pub fn leave(seat: Seat) {
    if let Some(address) = Address::of(seat) {
        socket::leave(&address);
    }
}

fn accept(listener: Listener, keeper: Arc<Keeper>) {
    loop {
        match listener.accept() {
            Ok(stream) => {
                let keeper = keeper.clone();
                // One thread each: a client that connects and then says
                // nothing must not keep the next one waiting. Named pipes have
                // no read timeout, so its thread waits with it until it goes.
                let _ = std::thread::Builder::new()
                    .name("agent client".to_owned())
                    .spawn(move || {
                        let _ = answer(stream, &keeper);
                    });
            }
            // Nothing this process can do about a failed accept except not
            // spin on it.
            Err(_) => std::thread::sleep(TICK),
        }
    }
}

fn answer(stream: Stream, keeper: &Keeper) -> io::Result<()> {
    let message = wire::receive(&stream)?;
    let request = Request::decode(&message);
    let locking = request == Some(Request::Lock);
    let answer = respond(request, keeper);
    let sent = wire::send(&stream, &answer.encode());

    // After the answer, which should reach whoever asked: being told may well
    // end this process. The password is gone already.
    if locking {
        keeper.tell();
    }
    sent
}

fn respond(request: Option<Request>, keeper: &Keeper) -> Answer {
    match request {
        None => Answer::Unknown,
        Some(Request::State) => Answer::of_state(keeper.state()),
        Some(Request::Lock) => Answer::LockedNow(keeper.wipe()),
        Some(Request::Key(salt)) => match keeper.key_for(&salt) {
            Ok(key) => Answer::Key(key),
            Err(_) => Answer::Locked,
        },
        Some(Request::Peek(salt)) => match keeper.peek(&salt) {
            Ok(key) => Answer::Key(key),
            Err(_) => Answer::Locked,
        },
        Some(Request::Fresh(opened_with)) => match keeper.fresh(&opened_with) {
            Ok(fresh) => Answer::Fresh(fresh),
            Err(_) => Answer::Locked,
        },
    }
}
