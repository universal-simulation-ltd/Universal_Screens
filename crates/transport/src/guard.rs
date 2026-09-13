//! Slowing down PIN guessing: one [`PinGuard`] per host, shared by its whole
//! accept loop.
//!
//! The PIN is four digits, so there are only 10,000 of them, and with "Use my
//! own PIN" it never changes. Without this, each guess cost an attacker one TCP
//! connect plus one Noise handshake — the whole space in minutes.
//!
//! ## What counts as a guess
//!
//! A peer only learns something about the PIN when the host *evaluates* one:
//!
//! - an encrypted client's first Noise message failing to authenticate
//!   ([`is_pin_rejection`](crate::is_pin_rejection) — the AEAD failure in
//!   `accept`), and
//! - a plaintext client's `ClientHello` carrying the wrong `pin` (the hosts' own
//!   hello check — the legacy/loopback path never runs Noise, so without this it
//!   would be a free guessing channel next to the rate-limited one).
//!
//! A peer that connects and hangs up, or times out, has tested nothing and is
//! not counted.
//!
//! ## The policy
//!
//! - The first [`FREE_FAILURES`] wrong PINs in a row cost nothing, so a person
//!   who mistypes once or twice is never made to wait.
//! - Each further one locks the host for [`FIRST_LOCKOUT`], doubling every time,
//!   up to [`MAX_LOCKOUT`]. While locked, the host closes new connections
//!   *before* the handshake, so no PIN is tested and the lockout is real rather
//!   than a delay an attacker can pipeline around.
//! - A right PIN resets the count. So does [`FORGET_AFTER`] with no wrong PIN at
//!   all, so an attack that stopped does not leave the owner's next typo locked
//!   out for five minutes.
//!
//! At the cap an attacker gets one guess per five minutes: half the space takes
//! about 17 days, against a host whose PIN changes on every start unless the
//! user chose one.
//!
//! ## Per host, not per address
//!
//! Deliberately. The remote-access relay (and the loopback browser bridge) make
//! every peer arrive from the same address, so a per-IP count would either lock
//! everyone out together anyway or — keyed wrongly — not at all. The cost is that
//! while someone is guessing, the owner's own devices wait too; the host windows
//! say so.
//!
//! The clock is a parameter (`now: Instant`) on every method, so the policy is
//! tested without sleeping.

use std::time::{Duration, Instant};

/// Wrong PINs in a row that cost nothing — room for an honest typo or two.
pub const FREE_FAILURES: u32 = 3;

/// The lockout after the first wrong PIN beyond [`FREE_FAILURES`]. Each further
/// one doubles it.
pub const FIRST_LOCKOUT: Duration = Duration::from_secs(1);

/// The longest single lockout: one guess per five minutes at worst.
pub const MAX_LOCKOUT: Duration = Duration::from_secs(5 * 60);

/// A quiet spell this long (no wrong PIN at all) forgets the count.
///
/// Long enough that waiting it out gains an attacker nothing: after a reset the
/// free guesses plus the doubling run up to the cap yield about a dozen guesses,
/// which is what an hour at the cap yields anyway.
pub const FORGET_AFTER: Duration = Duration::from_secs(60 * 60);

/// How long the `failures`-th wrong PIN in a row locks the host for.
/// Zero for the first [`FREE_FAILURES`].
#[must_use]
pub fn lockout_for(failures: u32) -> Duration {
    let Some(over) = failures.checked_sub(FREE_FAILURES + 1) else {
        return Duration::ZERO;
    };
    // 2^over seconds, saturating long before it could overflow: the cap is
    // reached at 2^9 = 512 s, so any exponent past 16 is simply the cap.
    let factor = 1u32 << over.min(16);
    FIRST_LOCKOUT.saturating_mul(factor).min(MAX_LOCKOUT)
}

/// The wrong-PIN count and lockout for one host's accept loop. See the module
/// docs for the policy. Not thread-safe by design: the hosts' `serve_loop`s are
/// single-threaded, and the one guard lives on that loop's stack.
#[derive(Debug, Default)]
pub struct PinGuard {
    /// Wrong PINs in a row since the last right one (or the last forgetting).
    failures: u32,
    /// When the most recent wrong PIN arrived, for [`FORGET_AFTER`].
    last_failure: Option<Instant>,
    /// New connections are refused until this instant.
    locked_until: Option<Instant>,
    /// Whether [`PinGuard::refuse`] has already reported this lockout.
    refusal_reported: bool,
}

/// A connection turned away by a lockout, from [`PinGuard::refuse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// How long the lockout has left.
    pub left: Duration,
    /// True for the first refusal of this lockout only, so a host logs one line
    /// per lockout rather than one per connection a guesser opens.
    pub first: bool,
}

impl Refusal {
    /// The line a host shows for it.
    #[must_use]
    pub fn message(&self, peer: &str) -> String {
        format!(
            "Refused {peer}: too many wrong PINs — try again in {}",
            human_wait(self.left)
        )
    }
}

impl PinGuard {
    /// A fresh guard: no wrong PINs, not locked.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How much longer the host is refusing connections, or `None` if a new one
    /// may be handshaken now.
    #[must_use]
    pub fn locked_for(&self, now: Instant) -> Option<Duration> {
        self.locked_until
            .and_then(|until| until.checked_duration_since(now))
            .filter(|left| !left.is_zero())
    }

    /// Call as a connection arrives, before any handshake. `Some` means the host
    /// is locked: close the connection unread, so no PIN is tested.
    pub fn refuse(&mut self, now: Instant) -> Option<Refusal> {
        let left = self.locked_for(now)?;
        let first = !self.refusal_reported;
        self.refusal_reported = true;
        Some(Refusal { left, first })
    }

    /// Count one wrong PIN arriving at `now`. Returns the lockout it started, if
    /// any (`None` while still within [`FREE_FAILURES`]).
    pub fn record_failure(&mut self, now: Instant) -> Option<Duration> {
        if self
            .last_failure
            .is_some_and(|last| now.saturating_duration_since(last) >= FORGET_AFTER)
        {
            self.failures = 0;
        }
        self.failures = self.failures.saturating_add(1);
        self.last_failure = Some(now);
        let lock = lockout_for(self.failures);
        if lock.is_zero() {
            return None;
        }
        // Never shorten a lockout already in force (a caller passing an older
        // `now` must not unlock the host early).
        let until = now + lock;
        self.locked_until = Some(self.locked_until.map_or(until, |u| u.max(until)));
        self.refusal_reported = false;
        Some(lock)
    }

    /// A right PIN: forget every wrong one before it.
    pub fn record_success(&mut self) {
        *self = Self::default();
    }

    /// Wrong PINs in a row so far.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// The line a host shows while locked, or `None` when it is not.
    #[must_use]
    pub fn notice(&self, now: Instant) -> Option<String> {
        self.locked_for(now).map(|left| {
            format!(
                "{} wrong PINs in a row — not accepting connections for {}",
                self.failures,
                human_wait(left)
            )
        })
    }
}

/// A wait rounded *up* to whole seconds ("1 s", "4 min 16 s"), so a host never
/// tells someone to retry before it will actually let them.
#[must_use]
pub fn human_wait(d: Duration) -> String {
    let secs = d.as_secs() + u64::from(d.subsec_nanos() > 0);
    match (secs / 60, secs % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn the_schedule_is_free_then_doubling_then_capped() {
        let got: Vec<u64> = (1..=14).map(|n| lockout_for(n).as_secs()).collect();
        assert_eq!(got, [0, 0, 0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 300, 300]);
        assert_eq!(lockout_for(0), Duration::ZERO);
        // No overflow however long it goes on.
        assert_eq!(lockout_for(u32::MAX), MAX_LOCKOUT);
        assert_eq!(lockout_for(64), MAX_LOCKOUT);
    }

    #[test]
    fn a_typo_or_two_never_locks_the_host() {
        let t0 = Instant::now();
        let mut g = PinGuard::new();
        for i in 0..FREE_FAILURES {
            assert_eq!(g.record_failure(t0 + secs(u64::from(i))), None);
            assert_eq!(g.locked_for(t0 + secs(u64::from(i))), None);
        }
        assert_eq!(g.failures(), FREE_FAILURES);
    }

    #[test]
    fn the_fourth_wrong_pin_locks_and_the_lock_expires() {
        let t0 = Instant::now();
        let mut g = PinGuard::new();
        for _ in 0..FREE_FAILURES {
            g.record_failure(t0);
        }
        assert_eq!(g.record_failure(t0), Some(secs(1)));
        assert_eq!(g.locked_for(t0), Some(secs(1)));
        assert_eq!(g.locked_for(t0 + Duration::from_millis(400)), Some(Duration::from_millis(600)));
        assert_eq!(g.locked_for(t0 + secs(1)), None, "open again exactly at expiry");
        assert_eq!(g.locked_for(t0 + secs(2)), None);
    }

    #[test]
    fn each_further_wrong_pin_doubles_up_to_the_cap() {
        let mut now = Instant::now();
        let mut g = PinGuard::new();
        let mut locks = Vec::new();
        for _ in 0..15 {
            // An attacker retrying the moment each lockout ends.
            if let Some(left) = g.locked_for(now) {
                now += left;
            }
            locks.push(g.record_failure(now).map_or(0, |d| d.as_secs()));
        }
        assert_eq!(locks, [0, 0, 0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 300, 300, 300]);
    }

    #[test]
    fn a_right_pin_resets_everything() {
        let t0 = Instant::now();
        let mut g = PinGuard::new();
        for _ in 0..6 {
            g.record_failure(t0);
        }
        assert!(g.locked_for(t0).is_some());
        g.record_success();
        assert_eq!(g.failures(), 0);
        assert_eq!(g.locked_for(t0), None);
        // …and the free allowance is back.
        assert_eq!(g.record_failure(t0), None);
    }

    #[test]
    fn a_long_quiet_spell_forgets_the_count() {
        let t0 = Instant::now();
        let mut g = PinGuard::new();
        for _ in 0..10 {
            g.record_failure(t0);
        }
        // Just short of the forgetting window: still counting, still escalating.
        let almost = t0 + FORGET_AFTER - secs(1);
        assert_eq!(g.record_failure(almost), Some(lockout_for(11)));
        // A full window after that last one: a single typo is free again.
        let later = almost + FORGET_AFTER;
        assert_eq!(g.record_failure(later), None);
        assert_eq!(g.failures(), 1);
    }

    #[test]
    fn a_clock_that_steps_backwards_does_not_panic_or_unlock_early() {
        let t0 = Instant::now() + secs(10);
        let mut g = PinGuard::new();
        for _ in 0..5 {
            g.record_failure(t0);
        }
        // `Instant` is monotonic, but the arithmetic must not assume the caller
        // passes increasing values.
        assert_eq!(g.record_failure(t0 - secs(5)), Some(lockout_for(6)));
        assert!(g.locked_for(t0 - secs(5)).is_some());
    }

    #[test]
    fn the_notice_names_the_count_and_rounds_the_wait_up() {
        let t0 = Instant::now();
        let mut g = PinGuard::new();
        assert_eq!(g.notice(t0), None);
        for _ in 0..4 {
            g.record_failure(t0);
        }
        assert_eq!(
            g.notice(t0 + Duration::from_millis(100)).as_deref(),
            Some("4 wrong PINs in a row — not accepting connections for 1 s")
        );
        assert_eq!(g.notice(t0 + secs(1)), None);
    }

    #[test]
    fn refuse_turns_connections_away_but_reports_each_lockout_once() {
        let t0 = Instant::now();
        let mut g = PinGuard::new();
        assert_eq!(g.refuse(t0), None, "an unlocked host admits");
        for _ in 0..4 {
            g.record_failure(t0);
        }
        assert_eq!(g.refuse(t0), Some(Refusal { left: secs(1), first: true }));
        let again = g.refuse(t0 + Duration::from_millis(500)).unwrap();
        assert!(!again.first, "a flood of refused connections logs one line");
        // Refusals test no PIN, so they neither count nor extend the lock.
        assert_eq!(g.failures(), 4);
        assert_eq!(g.refuse(t0 + secs(1)), None);
        // The next lockout is reported afresh.
        assert_eq!(g.record_failure(t0 + secs(1)), Some(secs(2)));
        assert!(g.refuse(t0 + secs(1)).unwrap().first);
        assert_eq!(
            Refusal { left: secs(2), first: true }.message("10.0.0.9:5000"),
            "Refused 10.0.0.9:5000: too many wrong PINs — try again in 2 s"
        );
    }

    #[test]
    fn human_wait_rounds_up_and_uses_minutes() {
        assert_eq!(human_wait(Duration::from_millis(1)), "1 s");
        assert_eq!(human_wait(secs(59)), "59 s");
        assert_eq!(human_wait(secs(60)), "1 min");
        assert_eq!(human_wait(Duration::from_millis(255_001)), "4 min 16 s");
        assert_eq!(human_wait(MAX_LOCKOUT), "5 min");
    }
}
