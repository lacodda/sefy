//! What goes over the socket, byte for byte.
//!
//! Hand-written rather than JSON because what it carries are keys, and a key
//! that passed through a text encoding leaves strings behind that nothing
//! wipes. A message is one length byte and that many bytes, in buffers that
//! wipe themselves. Everything fits: the longest is 81 bytes.
//!
//! The first byte of a message says what it is. Whoever does not recognize one
//! answers [`Answer::Unknown`], and a client that cannot get what it asked for
//! asks for the password instead, so a newer build and an older agent get
//! along.

use super::keeper::LockState;
use crate::crypto::{DerivedKey, KEY_LEN, SALT_LEN};
use crate::keys::FreshKey;
use std::io::{self, Read, Write};
use std::time::Duration;
use zeroize::Zeroizing;

const REQUEST_KEY: u8 = 0x01;
const REQUEST_FRESH: u8 = 0x02;
const REQUEST_STATE: u8 = 0x03;
const REQUEST_LOCK: u8 = 0x04;
const REQUEST_PEEK: u8 = 0x05;

const ANSWER_KEY: u8 = 0x81;
const ANSWER_FRESH: u8 = 0x82;
const ANSWER_LOCKED: u8 = 0x83;
const ANSWER_UNLOCKED: u8 = 0x84;
const ANSWER_LOCKED_NOW: u8 = 0x85;
const ANSWER_UNKNOWN: u8 = 0xff;

/// What a client asks.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Request {
    /// The key for this salt.
    Key([u8; SALT_LEN]),
    /// The key for this salt, without that counting as using the vault.
    Peek([u8; SALT_LEN]),
    /// A key for a new salt, and the key for this one as proof.
    Fresh([u8; SALT_LEN]),
    /// Whether anything is held, and for how much longer.
    State,
    /// Wipe it all.
    Lock,
}

/// What an agent answers.
pub(super) enum Answer {
    /// The key asked for.
    Key(DerivedKey),
    /// A key for a new salt.
    Fresh(FreshKey),
    /// There is nothing to give: the same answer to every question an agent
    /// without a password can be asked.
    Locked,
    /// Something is held.
    Unlocked {
        /// Until it locks.
        left: Duration,
        /// How long it stays unlocked with nobody using it.
        idle: Duration,
    },
    /// Done; and whether there was anything to wipe.
    LockedNow(bool),
    /// A message this build does not know.
    Unknown,
}

impl Answer {
    pub(super) fn of_state(state: LockState) -> Self {
        match state {
            LockState::Locked => Self::Locked,
            LockState::Unlocked { left, idle } => Self::Unlocked { left, idle },
        }
    }
}

impl Request {
    pub(super) fn encode(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(1 + SALT_LEN));
        match self {
            Self::Key(salt) => {
                bytes.push(REQUEST_KEY);
                bytes.extend_from_slice(salt);
            }
            Self::Peek(salt) => {
                bytes.push(REQUEST_PEEK);
                bytes.extend_from_slice(salt);
            }
            Self::Fresh(salt) => {
                bytes.push(REQUEST_FRESH);
                bytes.extend_from_slice(salt);
            }
            Self::State => bytes.push(REQUEST_STATE),
            Self::Lock => bytes.push(REQUEST_LOCK),
        }
        bytes
    }

    pub(super) fn decode(bytes: &[u8]) -> Option<Self> {
        let (&tag, rest) = bytes.split_first()?;
        match tag {
            REQUEST_KEY => Some(Self::Key(rest.try_into().ok()?)),
            REQUEST_PEEK => Some(Self::Peek(rest.try_into().ok()?)),
            REQUEST_FRESH => Some(Self::Fresh(rest.try_into().ok()?)),
            REQUEST_STATE if rest.is_empty() => Some(Self::State),
            REQUEST_LOCK if rest.is_empty() => Some(Self::Lock),
            _ => None,
        }
    }
}

impl Answer {
    pub(super) fn encode(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(1 + 2 * KEY_LEN + SALT_LEN));
        match self {
            Self::Key(key) => {
                bytes.push(ANSWER_KEY);
                bytes.extend_from_slice(key.as_bytes());
            }
            Self::Fresh(fresh) => {
                bytes.push(ANSWER_FRESH);
                bytes.extend_from_slice(&fresh.salt);
                bytes.extend_from_slice(fresh.key.as_bytes());
                bytes.extend_from_slice(fresh.proof.as_bytes());
            }
            Self::Locked => bytes.push(ANSWER_LOCKED),
            Self::Unlocked { left, idle } => {
                bytes.push(ANSWER_UNLOCKED);
                bytes.extend_from_slice(&left.as_secs().to_le_bytes());
                bytes.extend_from_slice(&idle.as_secs().to_le_bytes());
            }
            Self::LockedNow(had) => {
                bytes.push(ANSWER_LOCKED_NOW);
                bytes.push(u8::from(*had));
            }
            Self::Unknown => bytes.push(ANSWER_UNKNOWN),
        }
        bytes
    }

    pub(super) fn decode(bytes: &[u8]) -> Option<Self> {
        let (&tag, rest) = bytes.split_first()?;
        match tag {
            ANSWER_KEY => Some(Self::Key(key_from(rest)?)),
            ANSWER_FRESH if rest.len() == SALT_LEN + 2 * KEY_LEN => {
                let (salt, keys) = rest.split_at(SALT_LEN);
                let (key, proof) = keys.split_at(KEY_LEN);
                Some(Self::Fresh(FreshKey {
                    salt: salt.try_into().ok()?,
                    key: key_from(key)?,
                    proof: key_from(proof)?,
                }))
            }
            ANSWER_LOCKED if rest.is_empty() => Some(Self::Locked),
            ANSWER_UNLOCKED if rest.len() == 16 => {
                let (left, idle) = rest.split_at(8);
                Some(Self::Unlocked {
                    left: Duration::from_secs(u64::from_le_bytes(left.try_into().ok()?)),
                    idle: Duration::from_secs(u64::from_le_bytes(idle.try_into().ok()?)),
                })
            }
            ANSWER_LOCKED_NOW => match rest {
                [0] => Some(Self::LockedNow(false)),
                [1] => Some(Self::LockedNow(true)),
                _ => None,
            },
            ANSWER_UNKNOWN => Some(Self::Unknown),
            _ => None,
        }
    }
}

fn key_from(bytes: &[u8]) -> Option<DerivedKey> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    if bytes.len() != KEY_LEN {
        return None;
    }
    key.copy_from_slice(bytes);
    Some(DerivedKey::from_bytes(key))
}

/// Writes one message.
pub(super) fn send(mut stream: impl Write, message: &[u8]) -> io::Result<()> {
    let length = u8::try_from(message.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "message too long"))?;
    let mut framed = Zeroizing::new(Vec::with_capacity(message.len() + 1));
    framed.push(length);
    framed.extend_from_slice(message);
    stream.write_all(&framed)?;
    stream.flush()
}

/// Reads one message.
pub(super) fn receive(mut stream: impl Read) -> io::Result<Zeroizing<Vec<u8>>> {
    let mut length = [0u8; 1];
    stream.read_exact(&mut length)?;
    let mut message = Zeroizing::new(vec![0u8; usize::from(length[0])]);
    stream.read_exact(&mut message)?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> DerivedKey {
        DerivedKey::from_bytes(Zeroizing::new([byte; KEY_LEN]))
    }

    #[test]
    fn requests_come_out_as_they_went_in() {
        for request in [
            Request::Key([3; SALT_LEN]),
            Request::Peek([5; SALT_LEN]),
            Request::Fresh([4; SALT_LEN]),
            Request::State,
            Request::Lock,
        ] {
            assert_eq!(Request::decode(&request.encode()), Some(request));
        }
    }

    #[test]
    fn a_key_comes_out_as_it_went_in() {
        let Some(Answer::Key(back)) = Answer::decode(&Answer::Key(key(9)).encode()) else {
            panic!("not a key");
        };
        assert!(back.same_as(&key(9)));
    }

    #[test]
    fn a_fresh_key_comes_out_as_it_went_in_with_each_part_in_its_place() {
        let fresh = Answer::Fresh(FreshKey {
            salt: [1; SALT_LEN],
            key: key(2),
            proof: key(3),
        });
        let Some(Answer::Fresh(back)) = Answer::decode(&fresh.encode()) else {
            panic!("not a fresh key");
        };
        assert_eq!(back.salt, [1; SALT_LEN]);
        assert!(back.key.same_as(&key(2)));
        assert!(back.proof.same_as(&key(3)));
    }

    #[test]
    fn the_state_comes_out_as_it_went_in() {
        let answer = Answer::of_state(LockState::Unlocked {
            left: Duration::from_secs(70),
            idle: Duration::from_secs(900),
        });
        let Some(Answer::Unlocked { left, idle }) = Answer::decode(&answer.encode()) else {
            panic!("not a state");
        };
        assert_eq!(
            (left, idle),
            (Duration::from_secs(70), Duration::from_secs(900))
        );
        assert!(matches!(
            Answer::decode(&Answer::of_state(LockState::Locked).encode()),
            Some(Answer::Locked)
        ));
    }

    #[test]
    fn the_longest_message_fits_its_length_byte() {
        let fresh = Answer::Fresh(FreshKey {
            salt: [1; SALT_LEN],
            key: key(2),
            proof: key(3),
        });
        let mut wire = Vec::new();
        send(&mut wire, &fresh.encode()).unwrap();
        assert_eq!(usize::from(wire[0]) + 1, wire.len());
        assert_eq!(receive(wire.as_slice()).unwrap().as_slice(), &wire[1..]);
    }

    #[test]
    fn what_is_cut_short_or_unknown_is_not_read_as_anything() {
        assert_eq!(Request::decode(&[]), None);
        assert_eq!(Request::decode(&[0x7f]), None);
        assert_eq!(Request::decode(&[REQUEST_KEY, 1, 2]), None);
        assert!(Answer::decode(&[]).is_none());
        assert!(Answer::decode(&[ANSWER_KEY, 1, 2]).is_none());
        assert!(Answer::decode(&[ANSWER_FRESH, 1, 2]).is_none());
        assert!(Answer::decode(&[0x70]).is_none());
        assert!(receive(&[5u8, 1, 2][..]).is_err());
    }
}
