//! The clipboard, as the window uses it: one secret at a time, taken back
//! when its time is up, when the vault is locked, and when the window closes.
//!
//! The clipboard is worked from one thread of its own. On Windows it is a
//! single object for the whole desktop that two threads must not work at
//! once; on X11 and Wayland the value is served by whoever put it there, so
//! the handle has to outlive the copy. A thread that owns the handle for as
//! long as the window runs is both.
//!
//! Marking a secret as one, and taking back only what is still ours, are the
//! core's (`sefy_core::clipboard`): the same as the command line's `get`.

use crate::wire::{Failure, FailureKind};
use sefy_core::clipboard::{Clipboard, HOLD_SECONDS};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// What the clipboard thread is told to do.
enum Order {
    /// Put this value on the clipboard, say whether it went, and take it back
    /// once the hold is over.
    Put(Zeroizing<String>, Sender<Result<(), Failure>>),
    /// Take back what is held now, if it is still there, and say when done.
    TakeBack(Sender<()>),
}

/// The window's way to the clipboard.
pub struct Clip {
    orders: Mutex<Sender<Order>>,
}

impl Clip {
    /// Starts the clipboard thread, holding each value for `hold`.
    pub fn start(hold: Duration) -> Self {
        let (orders, inbox) = mpsc::channel();
        std::thread::Builder::new()
            .name("clipboard".to_owned())
            .spawn(move || serve(inbox, hold))
            .expect("the clipboard thread starts");
        Self {
            orders: Mutex::new(orders),
        }
    }

    /// Starts the clipboard thread with the hold every front end uses.
    pub fn standard() -> Self {
        Self::start(Duration::from_secs(HOLD_SECONDS))
    }

    /// Puts `value` on the clipboard in place of whatever this window put
    /// there before. Waits until it is there, or says why it is not.
    pub fn put(&self, value: Zeroizing<String>) -> Result<(), Failure> {
        let (reply, answer) = mpsc::channel();
        self.send(Order::Put(value, reply))?;
        answer.recv().unwrap_or_else(|_| Err(gone()))
    }

    /// Takes back what this window put on the clipboard, if it is still
    /// there, and waits until that is done - so that a window closing right
    /// after does not leave it behind.
    pub fn take_back(&self) {
        let (reply, done) = mpsc::channel();
        if self.send(Order::TakeBack(reply)).is_ok() {
            let _ = done.recv();
        }
    }

    fn send(&self, order: Order) -> Result<(), Failure> {
        self.orders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send(order)
            .map_err(|_| gone())
    }
}

/// The clipboard thread: one value held at most, taken back when its time is
/// up or when told to.
fn serve(inbox: Receiver<Order>, hold: Duration) {
    // Opened at the first copy rather than at start: a window that is never
    // asked to copy anything never touches the clipboard.
    let mut clipboard: Option<Clipboard> = None;
    let mut held: Option<(Zeroizing<String>, Instant)> = None;

    loop {
        let order = match &held {
            Some((_, until)) => {
                match inbox.recv_timeout(until.saturating_duration_since(Instant::now())) {
                    Ok(order) => Some(order),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            None => match inbox.recv() {
                Ok(order) => Some(order),
                Err(_) => break,
            },
        };

        match order {
            // The hold is over.
            None => give_back(&mut clipboard, &mut held),
            Some(Order::TakeBack(done)) => {
                give_back(&mut clipboard, &mut held);
                let _ = done.send(());
            }
            Some(Order::Put(value, reply)) => {
                let result = opened(&mut clipboard)
                    .and_then(|clipboard| clipboard.put(&value).map_err(Failure::from));
                // The value replaced, if any, is ours no longer to take back:
                // the new one is what is on the clipboard now. A copy that
                // failed replaced nothing, and the old hold stands.
                if result.is_ok() {
                    held = Some((value, Instant::now() + hold));
                }
                let _ = reply.send(result);
            }
        }
    }
    give_back(&mut clipboard, &mut held);
}

/// The clipboard, opened on first use.
fn opened(clipboard: &mut Option<Clipboard>) -> Result<&mut Clipboard, Failure> {
    if clipboard.is_none() {
        *clipboard = Some(Clipboard::open().map_err(Failure::from)?);
    }
    Ok(clipboard.as_mut().expect("opened just above"))
}

/// Takes the held value back off the clipboard, if it is still what is there.
fn give_back(clipboard: &mut Option<Clipboard>, held: &mut Option<(Zeroizing<String>, Instant)>) {
    if let (Some(clipboard), Some((value, _))) = (clipboard.as_mut(), held.take()) {
        clipboard.take_back(&value);
    }
}

fn gone() -> Failure {
    Failure {
        kind: FailureKind::Other,
        message: "the clipboard stopped answering".to_owned(),
    }
}
