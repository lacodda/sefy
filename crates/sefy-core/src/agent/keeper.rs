//! What an agent holds, and for how long.
//!
//! The rule the rest of this file serves: **when the time is up the password
//! is gone, whether or not anyone asked.** A check made only when a request
//! arrives would refuse the request and leave the password in memory for as
//! long as nobody came, which is how gpg-agent's cache behaves. So the clock
//! runs on a thread of its own ([`super::start_ticking`]) and [`Keeper::tick`]
//! wipes.

use super::LockState as State;
use crate::crypto::{DerivedKey, SALT_LEN, random_bytes};
use crate::error::{Error, Result};
use crate::keys::{FreshKey, KeySource};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};
use zeroize::Zeroizing;

/// How long an agent stays unlocked with nobody using it, unless told otherwise.
pub const IDLE: Duration = Duration::from_secs(crate::policy::LOCK_AFTER_SECONDS);

/// How long an agent stays unlocked from the moment it got the password,
/// however busy. Without it a script asking every few minutes would keep the
/// password in memory for good.
pub const CAP: Duration = Duration::from_secs(12 * 60 * 60);

/// How many keys are kept, so that reading a file does not stretch the
/// password again: the file as it is, the version a running command opened,
/// a copy or two.
const KEPT_KEYS: usize = 4;

/// Whether an agent is holding the password, and for how much longer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockState {
    /// It is not.
    Locked,
    /// It is.
    Unlocked {
        /// Until it locks if nobody uses it, or at its hard limit if sooner.
        left: Duration,
        /// How long it stays unlocked with nobody using it.
        idle: Duration,
    },
}

/// A moment on both clocks.
///
/// `Instant` does not advance while a machine sleeps on Linux and macOS, so on
/// its own it would wake a laptop after a night with fifteen idle minutes
/// still to run. The wall clock does advance, and can be set back by anyone.
/// Time passed is whichever says more: setting the clock back buys nothing,
/// and a night asleep counts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Moment {
    steady: Instant,
    wall: SystemTime,
}

impl Moment {
    pub(crate) fn now() -> Self {
        Self {
            steady: Instant::now(),
            wall: SystemTime::now(),
        }
    }

    fn since(self, earlier: Self) -> Duration {
        let steady = self.steady.saturating_duration_since(earlier.steady);
        let wall = self
            .wall
            .duration_since(earlier.wall)
            .unwrap_or(Duration::ZERO);
        steady.max(wall)
    }
}

/// The password and what was derived from it.
///
/// Every secret here wipes itself when dropped.
struct Held {
    password: Zeroizing<Vec<u8>>,
    /// Oldest first.
    keys: Vec<([u8; SALT_LEN], DerivedKey)>,
    given_at: Moment,
    last_used: Moment,
    idle: Duration,
}

impl Held {
    fn left(&self, at: Moment) -> Duration {
        let idle = self.idle.saturating_sub(at.since(self.last_used));
        let cap = CAP.saturating_sub(at.since(self.given_at));
        idle.min(cap)
    }

    fn key_for(&mut self, salt: &[u8; SALT_LEN]) -> Result<DerivedKey> {
        if let Some(found) = self.keys.iter().position(|(known, _)| known == salt) {
            // Moved to the newest end: the version in use is the last to go.
            let entry = self.keys.remove(found);
            let key = entry.1.duplicate();
            self.keys.push(entry);
            return Ok(key);
        }
        let key = DerivedKey::derive(&self.password, salt)?;
        self.keep(*salt, key.duplicate());
        Ok(key)
    }

    fn keep(&mut self, salt: [u8; SALT_LEN], key: DerivedKey) {
        if self.keys.len() == KEPT_KEYS {
            self.keys.remove(0);
        }
        self.keys.push((salt, key));
    }
}

/// What to do when the password goes without the owner of this keeper having
/// said so: the time ran out, or `sefy lock` came over the socket.
type Told = Arc<dyn Fn() + Send + Sync>;

/// Holds the password for as long as it should be held.
#[derive(Default)]
pub struct Keeper {
    held: Mutex<Option<Held>>,
    told: Mutex<Option<Told>>,
}

impl Keeper {
    /// A keeper holding nothing yet.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Says what to do once the password has gone by itself or from outside.
    /// A process that exists to hold it exits; the app locks its window.
    pub fn when_locked(&self, told: impl Fn() + Send + Sync + 'static) {
        *self.told.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(told));
    }

    /// Takes the password, in place of any it held, for `idle` of nobody using
    /// it. Nothing can ask for longer than [`CAP`].
    pub fn give(&self, password: Zeroizing<Vec<u8>>, idle: Duration) {
        self.give_at(password, idle, Moment::now());
    }

    fn give_at(&self, password: Zeroizing<Vec<u8>>, idle: Duration, at: Moment) {
        *self.slot() = Some(Held {
            password,
            keys: Vec::new(),
            given_at: at,
            last_used: at,
            idle: idle.min(CAP),
        });
    }

    /// Wipes the password and every key, and says whether there were any.
    ///
    /// Tells nobody: the caller is who wanted it gone.
    pub fn wipe(&self) -> bool {
        self.slot().take().is_some()
    }

    /// From now on, how long the vault stays unlocked with nobody using it. No
    /// more than [`CAP`], as ever; the time since it was last used still counts.
    pub fn set_idle(&self, idle: Duration) {
        if let Some(held) = self.slot().as_mut() {
            held.idle = idle.min(CAP);
        }
    }

    /// Someone is using the vault: the idle time starts over.
    pub fn touch(&self) {
        self.touch_at(Moment::now());
    }

    fn touch_at(&self, at: Moment) {
        if let Some(held) = self.slot().as_mut() {
            held.last_used = at;
        }
    }

    /// Whether anything is held, and for how much longer. Looking does not
    /// count as using it.
    pub fn state(&self) -> State {
        self.state_at(Moment::now())
    }

    fn state_at(&self, at: Moment) -> State {
        self.tick_at(at);
        match self.slot().as_ref() {
            Some(held) => State::Unlocked {
                left: held.left(at),
                idle: held.idle,
            },
            None => State::Locked,
        }
    }

    /// If the time is up, wipes everything and tells whoever asked to know.
    /// Returns whether it did.
    pub fn tick(&self) -> bool {
        self.tick_at(Moment::now())
    }

    fn tick_at(&self, at: Moment) -> bool {
        let expired = {
            let mut slot = self.slot();
            let expired = slot
                .as_ref()
                .is_some_and(|held| held.left(at) == Duration::ZERO);
            if expired {
                *slot = None;
            }
            expired
        };
        if expired {
            self.tell();
        }
        expired
    }

    /// Runs what [`Keeper::when_locked`] set, with no lock of ours held: it
    /// may well come back to [`Keeper::wipe`].
    pub(crate) fn tell(&self) {
        let told = self
            .told
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(told) = told {
            told();
        }
    }

    fn slot(&self) -> MutexGuard<'_, Option<Held>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The key for this salt, without that counting as using the vault: for
    /// looking at it, which something can do every minute in an empty room.
    pub fn peek(&self, salt: &[u8; SALT_LEN]) -> Result<DerivedKey> {
        self.peek_at(salt, Moment::now())
    }

    fn peek_at(&self, salt: &[u8; SALT_LEN], at: Moment) -> Result<DerivedKey> {
        self.tick_at(at);
        self.slot().as_mut().ok_or(Error::KeysGone)?.key_for(salt)
    }

    fn key_for_at(&self, salt: &[u8; SALT_LEN], at: Moment) -> Result<DerivedKey> {
        self.tick_at(at);
        let mut slot = self.slot();
        let held = slot.as_mut().ok_or(Error::KeysGone)?;
        held.last_used = at;
        held.key_for(salt)
    }

    fn fresh_at(&self, opened_with: &[u8; SALT_LEN], at: Moment) -> Result<FreshKey> {
        self.tick_at(at);
        let mut slot = self.slot();
        let held = slot.as_mut().ok_or(Error::KeysGone)?;
        held.last_used = at;
        let proof = held.key_for(opened_with)?;
        let salt = random_bytes::<SALT_LEN>()?;
        let key = DerivedKey::derive(&held.password, &salt)?;
        // The file will have this salt in a moment, and whoever reads it next
        // should not have to wait for the password to be stretched again.
        held.keep(salt, key.duplicate());
        Ok(FreshKey { salt, key, proof })
    }
}

impl KeySource for Keeper {
    fn key_for(&self, salt: &[u8; SALT_LEN]) -> Result<DerivedKey> {
        self.key_for_at(salt, Moment::now())
    }

    fn fresh(&self, opened_with: &[u8; SALT_LEN]) -> Result<FreshKey> {
        self.fresh_at(opened_with, Moment::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const MINUTE: Duration = Duration::from_secs(60);
    const HOUR: Duration = Duration::from_secs(60 * 60);

    fn password(text: &str) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(text.as_bytes().to_vec())
    }

    fn later(moment: Moment, by: Duration) -> Moment {
        Moment {
            steady: moment.steady + by,
            wall: moment.wall + by,
        }
    }

    /// A keeper holding a password since `start`.
    fn holding(start: Moment, idle: Duration) -> Keeper {
        let keeper = Keeper::default();
        keeper.give_at(password("pw"), idle, start);
        keeper
    }

    #[test]
    fn the_password_goes_when_the_time_is_up_with_nobody_asking() {
        // The point of the stage. No request here: only the clock's tick. A
        // keeper that merely refused requests after the time would still hold
        // the password when this looks.
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);

        assert!(!keeper.tick_at(later(start, 14 * MINUTE)));
        assert!(keeper.slot().is_some());

        assert!(keeper.tick_at(later(start, 15 * MINUTE)));
        assert!(keeper.slot().is_none(), "the time is up and it still holds");
    }

    #[test]
    fn a_request_after_the_time_finds_nothing_even_before_any_tick() {
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);
        let after = later(start, 16 * MINUTE);

        assert!(matches!(
            keeper.key_for_at(&[1; SALT_LEN], after),
            Err(Error::KeysGone)
        ));
        assert!(keeper.slot().is_none());
    }

    #[test]
    fn using_it_starts_the_idle_time_over() {
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);

        keeper
            .key_for_at(&[1; SALT_LEN], later(start, 10 * MINUTE))
            .unwrap();
        assert!(!keeper.tick_at(later(start, 20 * MINUTE)));
        assert!(keeper.tick_at(later(start, 25 * MINUTE)));
    }

    #[test]
    fn saving_counts_as_using_it() {
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);

        keeper
            .fresh_at(&[1; SALT_LEN], later(start, 10 * MINUTE))
            .unwrap();
        assert!(!keeper.tick_at(later(start, 20 * MINUTE)));
    }

    #[test]
    fn looking_does_not_count_as_using_it() {
        // A status prompt asking every minute must not keep a vault unlocked.
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);

        for minute in 1..15 {
            assert!(matches!(
                keeper.state_at(later(start, minute * MINUTE)),
                State::Unlocked { .. }
            ));
        }
        assert_eq!(keeper.state_at(later(start, 15 * MINUTE)), State::Locked);
    }

    #[test]
    fn a_key_only_looked_at_does_not_start_the_time_over() {
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);
        let expected = DerivedKey::derive(b"pw", &[1; SALT_LEN]).unwrap();

        for minute in [5, 10, 14] {
            let key = keeper
                .peek_at(&[1; SALT_LEN], later(start, minute * MINUTE))
                .unwrap();
            assert!(key.same_as(&expected));
        }
        assert!(keeper.tick_at(later(start, 15 * MINUTE)));
        assert!(matches!(
            keeper.peek_at(&[1; SALT_LEN], later(start, 16 * MINUTE)),
            Err(Error::KeysGone)
        ));
    }

    #[test]
    fn no_amount_of_use_outlasts_the_hard_limit() {
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);

        let mut at = start;
        for _ in 0..(12 * 6 - 1) {
            at = later(at, 10 * MINUTE);
            keeper.touch_at(at);
            assert!(!keeper.tick_at(at));
        }
        assert!(keeper.tick_at(later(start, 12 * HOUR)));
    }

    #[test]
    fn no_idle_time_asked_for_outlasts_the_hard_limit() {
        let start = Moment::now();
        let keeper = holding(start, 100 * HOUR);
        let State::Unlocked { left, idle } = keeper.state_at(start) else {
            panic!("it holds the password");
        };
        assert_eq!((left, idle), (CAP, CAP));
    }

    #[test]
    fn a_night_asleep_counts_though_the_steady_clock_stood_still() {
        // What a suspended Linux or macOS machine looks like on waking.
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);
        let woken = Moment {
            steady: start.steady + MINUTE,
            wall: start.wall + 8 * HOUR,
        };
        assert!(keeper.tick_at(woken));
    }

    #[test]
    fn setting_the_clock_back_buys_nothing() {
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);
        let set_back = Moment {
            steady: start.steady + 16 * MINUTE,
            wall: start.wall - 8 * HOUR,
        };
        assert!(keeper.tick_at(set_back));
    }

    #[test]
    fn the_left_time_is_the_sooner_of_the_two_limits() {
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);

        let State::Unlocked { left, idle } = keeper.state_at(later(start, 5 * MINUTE)) else {
            panic!("it holds the password");
        };
        assert_eq!((left, idle), (10 * MINUTE, 15 * MINUTE));

        // Near the hard limit, a vault just used still has only what is left
        // of it.
        let near_cap = later(start, 12 * HOUR - 5 * MINUTE);
        keeper.touch_at(near_cap);
        let State::Unlocked { left, .. } = keeper.state_at(near_cap) else {
            panic!("it holds the password");
        };
        assert_eq!(left, 5 * MINUTE);
    }

    #[test]
    fn whoever_asked_is_told_once_when_the_time_runs_out() {
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);
        let told = Arc::new(AtomicUsize::new(0));
        let counter = told.clone();
        keeper.when_locked(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        keeper.tick_at(later(start, 14 * MINUTE));
        assert_eq!(told.load(Ordering::SeqCst), 0);
        keeper.tick_at(later(start, 15 * MINUTE));
        keeper.tick_at(later(start, 16 * MINUTE));
        assert_eq!(told.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn being_told_can_come_back_for_a_wipe() {
        // The app's handler locks everything, which wipes this keeper again.
        // Told with a lock of ours still held, this would hang here.
        let start = Moment::now();
        let keeper = Arc::new(holding(start, MINUTE));
        let same = keeper.clone();
        keeper.when_locked(move || {
            same.wipe();
        });
        assert!(keeper.tick_at(later(start, 2 * MINUTE)));
    }

    #[test]
    fn a_wipe_of_its_own_tells_nobody_and_says_what_it_found() {
        let start = Moment::now();
        let keeper = holding(start, 15 * MINUTE);
        let told = Arc::new(AtomicUsize::new(0));
        let counter = told.clone();
        keeper.when_locked(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        assert!(keeper.wipe());
        assert!(!keeper.wipe());
        assert_eq!(told.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_key_is_the_one_the_password_makes() {
        let keeper = holding(Moment::now(), 15 * MINUTE);
        let salt = [7; SALT_LEN];
        let expected = DerivedKey::derive(b"pw", &salt).unwrap();

        // Twice: derived, then from what was kept.
        assert!(keeper.key_for(&salt).unwrap().same_as(&expected));
        assert!(keeper.key_for(&salt).unwrap().same_as(&expected));
    }

    #[test]
    fn a_fresh_key_has_a_salt_of_its_own_and_proves_the_password() {
        let keeper = holding(Moment::now(), 15 * MINUTE);
        let opened = [7; SALT_LEN];

        let first = keeper.fresh(&opened).unwrap();
        let second = keeper.fresh(&opened).unwrap();

        assert_ne!(first.salt, second.salt, "a salt was used twice");
        assert_ne!(first.salt, opened);
        assert!(
            first
                .key
                .same_as(&DerivedKey::derive(b"pw", &first.salt).unwrap())
        );
        assert!(
            first
                .proof
                .same_as(&DerivedKey::derive(b"pw", &opened).unwrap())
        );
    }

    #[test]
    fn the_key_of_a_fresh_salt_is_kept_for_whoever_reads_next() {
        let keeper = holding(Moment::now(), 15 * MINUTE);
        let fresh = keeper.fresh(&[7; SALT_LEN]).unwrap();
        assert!(
            keeper
                .slot()
                .as_ref()
                .unwrap()
                .keys
                .iter()
                .any(|(salt, _)| *salt == fresh.salt)
        );
    }

    #[test]
    fn only_so_many_keys_are_kept_and_the_one_in_use_stays() {
        let keeper = holding(Moment::now(), 15 * MINUTE);
        let in_use = [0; SALT_LEN];
        keeper.key_for(&in_use).unwrap();
        for byte in 1..=(KEPT_KEYS as u8 + 2) {
            keeper.key_for(&in_use).unwrap();
            keeper.key_for(&[byte; SALT_LEN]).unwrap();
        }
        let slot = keeper.slot();
        let keys = &slot.as_ref().unwrap().keys;
        assert_eq!(keys.len(), KEPT_KEYS);
        assert!(keys.iter().any(|(salt, _)| *salt == in_use));
    }

    #[test]
    fn with_nothing_held_there_is_nothing_to_give() {
        let keeper = Keeper::default();
        assert_eq!(keeper.state(), State::Locked);
        assert!(matches!(
            keeper.key_for(&[1; SALT_LEN]),
            Err(Error::KeysGone)
        ));
        assert!(matches!(keeper.fresh(&[1; SALT_LEN]), Err(Error::KeysGone)));
    }

    #[test]
    fn a_new_password_takes_the_keys_of_the_old_one_with_it() {
        let keeper = holding(Moment::now(), 15 * MINUTE);
        let salt = [7; SALT_LEN];
        let old = keeper.key_for(&salt).unwrap();

        keeper.give(password("another"), 15 * MINUTE);
        assert!(!keeper.key_for(&salt).unwrap().same_as(&old));
    }
}
