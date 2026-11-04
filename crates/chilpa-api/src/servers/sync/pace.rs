//! How a media server's walk is paced, and how long it waits for a server
//! that stopped answering.
//!
//! A media server answers one request at a time on hardware that is often
//! small, and a walk sends thousands. Asked as fast as it answers, some fall
//! over in the middle of a library. So a walk asks at most `FLOOR_MS` apart,
//! leaves a gap after each answer that grows while the server is slow or
//! fails and comes back down while it is quick, and, when the server stops
//! answering, waits for it to come back before going on from the same page.

use std::time::{Duration, Instant};

/// At most four requests a second, whatever the gap.
const FLOOR_MS: u64 = 250;
/// The gap after an answer: where it starts and where it comes back to,
/// and the most it grows to.
const START_MS: u64 = 200;
const MOST_MS: u64 = 5_000;
/// An answer this slow, or none, doubles the gap.
const SLOW_MS: u64 = 2_000;

/// The waits for a server that stopped answering: these first, then every
/// `DOWN_EVERY_MS`, until they add up to `DOWN_MOST_MS`.
const DOWN_MS: [u64; 5] = [2_000, 5_000, 15_000, 45_000, 120_000];
const DOWN_EVERY_MS: u64 = 120_000;
const DOWN_MOST_MS: u64 = 30 * 60 * 1000;

/// What a wait for a server says while it lasts, so its sync is not taken
/// for stuck (`super::STUCK`).
const SAY_EVERY: Duration = super::BEAT;

/// The constants' milliseconds as time; in a test, microseconds, so the
/// same schedule runs a thousand times faster.
fn ms(n: u64) -> Duration {
    if cfg!(test) { Duration::from_micros(n) } else { Duration::from_millis(n) }
}

/// The gap a walk leaves after each answer.
pub(crate) struct Pace {
    gap: u64,
}

impl Pace {
    pub(crate) fn new() -> Self {
        Self { gap: START_MS }
    }

    /// How long to wait before the next request, after one that took `took`
    /// and was or was not answered.
    pub(crate) fn after(&mut self, took: Duration, answered: bool) -> Duration {
        let before = self.gap;
        self.gap = if !answered || took >= ms(SLOW_MS) {
            (self.gap * 2).min(MOST_MS)
        } else {
            (self.gap - self.gap / 4).max(START_MS)
        };
        if self.gap > before {
            eprintln!("chilpa-servers: media server walk slows down: {} ms between requests", self.gap);
        }
        ms(self.gap).max(ms(FLOOR_MS).saturating_sub(took))
    }
}

/// The waits for a server that stopped answering, in order.
pub(crate) fn down_waits() -> impl Iterator<Item = Duration> {
    let mut total = 0;
    DOWN_MS
        .into_iter()
        .chain(std::iter::repeat(DOWN_EVERY_MS))
        .take_while(move |wait| {
            total += wait;
            total <= DOWN_MOST_MS
        })
        .map(ms)
}

/// Sleeps `wait`, calling `say` every `SAY_EVERY` meanwhile. False if the
/// sync was asked to stop.
pub(crate) fn rest(wait: Duration, stopped: &dyn Fn() -> bool, say: &dyn Fn()) -> bool {
    let until = Instant::now() + wait;
    loop {
        if stopped() {
            return false;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        std::thread::sleep(left.min(SAY_EVERY));
        say();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walk_never_asks_more_than_four_times_a_second() {
        let mut pace = Pace::new();
        for _ in 0..20 {
            let wait = pace.after(Duration::ZERO, true);
            assert!(wait >= ms(FLOOR_MS), "{wait:?}");
        }
        // After an answer that took most of the floor, the gap alone.
        assert_eq!(pace.after(ms(240), true), ms(START_MS));
    }

    #[test]
    fn the_gap_grows_while_the_server_struggles_and_comes_back_after() {
        let mut pace = Pace::new();
        assert_eq!(pace.after(ms(10), false), ms(400));
        assert_eq!(pace.after(ms(SLOW_MS), true), ms(800));
        for _ in 0..10 {
            pace.after(ms(10), false);
        }
        assert_eq!(pace.gap, MOST_MS, "it grows to a ceiling");
        for _ in 0..40 {
            pace.after(ms(10), true);
        }
        assert_eq!(pace.gap, START_MS, "and comes back down to where it began");
    }

    #[test]
    fn a_server_is_waited_for_longer_each_time_and_half_an_hour_in_all() {
        let waits: Vec<_> = down_waits().collect();
        assert_eq!(&waits[..6], &[ms(2_000), ms(5_000), ms(15_000), ms(45_000), ms(120_000), ms(120_000)]);
        let total: Duration = waits.iter().sum();
        assert!(total <= ms(DOWN_MOST_MS) && total > ms(DOWN_MOST_MS - DOWN_EVERY_MS), "{total:?}");
    }

    #[test]
    fn a_rest_ends_when_the_sync_is_stopped() {
        let said = std::cell::Cell::new(0);
        assert!(rest(Duration::from_millis(5), &|| false, &|| said.set(said.get() + 1)));
        assert!(said.get() >= 1);
        assert!(!rest(Duration::from_secs(60), &|| true, &|| {}));
    }
}
