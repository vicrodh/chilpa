//! The watchdog: it counts failures and sends plugins to quarantine.
//!
//! ADR-003 says it plainly: *a broken plugin never breaks Chilpa; before
//! breaking it, it does not load*. This module is the half that remembers
//! the failures; the other half is the store's limits, over in `limits`.
//!
//! Three decisions worth stating:
//!
//! 1. **Three failures in a row in a short window, and it is quarantined.**
//!    Not on the first stumble: a network that drops is not a broken plugin.
//! 2. **Only the user takes it out of quarantine.** No retrying on its own
//!    later: if something broke three times, someone should look at it.
//! 3. **A call that goes well clears the count.** What is punished is
//!    failing repeatedly, not having failed once.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How many failures in a row send a plugin to quarantine.
pub const FAILURES_TO_QUARANTINE: usize = 3;

/// The span those failures have to fit in to count as consecutive.
pub const WINDOW: Duration = Duration::from_secs(60);

/// How a plugin is doing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum State {
    /// Found on disk, but the user has not yet approved what it asks for.
    /// It has been neither compiled nor instantiated: it exists as a folder
    /// (ADR-008).
    PendingApproval,
    /// Working.
    Healthy,
    /// It has failed, but not enough yet.
    Stumbling { failures: usize },
    /// Switched off until the user turns it back on.
    Quarantined { reason: String },
}

impl State {
    pub fn usable(&self) -> bool {
        matches!(self, State::Healthy | State::Stumbling { .. })
    }
}

#[derive(Default)]
struct Tally {
    failures: Vec<Instant>,
    quarantine: Option<String>,
}

/// Keeps score of how each plugin behaves.
#[derive(Default)]
pub struct Watchdog {
    tallies: HashMap<String, Tally>,
}

impl Watchdog {
    pub fn new() -> Self {
        Self::default()
    }

    /// How a plugin is doing. One that has never done anything is fine.
    pub fn state(&self, id: &str) -> State {
        let Some(c) = self.tallies.get(id) else { return State::Healthy };
        if let Some(reason) = &c.quarantine {
            return State::Quarantined { reason: reason.clone() };
        }
        let recent = c.failures.iter().filter(|t| t.elapsed() < WINDOW).count();
        if recent == 0 {
            State::Healthy
        } else {
            State::Stumbling { failures: recent }
        }
    }

    /// True if it can be asked for something.
    pub fn usable(&self, id: &str) -> bool {
        self.state(id).usable()
    }

    /// Notes that a call went well. It forgives what came before.
    pub fn success(&mut self, id: &str) {
        if let Some(c) = self.tallies.get_mut(id) {
            if c.quarantine.is_none() {
                c.failures.clear();
            }
        }
    }

    /// Notes a failure. Returns the state it ended up in.
    pub fn failure(&mut self, id: &str, reason: &str) -> State {
        self.failure_at(id, reason, Instant::now())
    }

    /// The same, with the moment supplied. It is what makes this testable
    /// without sleeping for a minute.
    pub fn failure_at(&mut self, id: &str, reason: &str, at: Instant) -> State {
        let c = self.tallies.entry(id.to_string()).or_default();
        if c.quarantine.is_some() {
            return self.state(id);
        }
        // The old is forgotten: what is punished is failing repeatedly.
        c.failures.retain(|t| at.duration_since(*t) < WINDOW);
        c.failures.push(at);
        if c.failures.len() >= FAILURES_TO_QUARANTINE {
            c.quarantine = Some(chilpa_i18n::t_args(
                "it failed {} times in a row; the last one: {}",
                &[&FAILURES_TO_QUARANTINE.to_string(), reason],
            ));
        }
        self.state(id)
    }

    /// Quarantines it immediately, without counting. It is for what has no way
    /// back: a manifest that is not understood, a module that does not load.
    pub fn quarantine(&mut self, id: &str, reason: &str) {
        let c = self.tallies.entry(id.to_string()).or_default();
        c.quarantine = Some(reason.to_string());
    }

    /// Takes it out of quarantine. Only the user calls it.
    pub fn reactivate(&mut self, id: &str) {
        if let Some(c) = self.tallies.get_mut(id) {
            c.quarantine = None;
            c.failures.clear();
        }
    }

    /// The ones in quarantine, with their reason.
    pub fn quarantined(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self
            .tallies
            .iter()
            .filter_map(|(id, c)| c.quarantine.clone().map(|r| (id.clone(), r)))
            .collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_that_has_never_done_anything_is_healthy() {
        let w = Watchdog::new();
        assert_eq!(w.state("com.a.b"), State::Healthy);
        assert!(w.usable("com.a.b"));
    }

    #[test]
    fn two_failures_are_not_quarantine_but_they_show() {
        let mut w = Watchdog::new();
        assert_eq!(w.failure("com.a.b", "it blows up"), State::Stumbling { failures: 1 });
        assert_eq!(w.failure("com.a.b", "it blows up"), State::Stumbling { failures: 2 });
        assert!(w.usable("com.a.b"), "it can still be asked for something");
    }

    #[test]
    fn on_the_third_it_goes_to_quarantine_and_says_why() {
        let mut w = Watchdog::new();
        w.failure("com.a.b", "one");
        w.failure("com.a.b", "another");
        let state = w.failure("com.a.b", "it ran out of fuel");
        match &state {
            State::Quarantined { reason } => {
                assert!(reason.contains("3 times"), "{reason}");
                assert!(reason.contains("out of fuel"), "the last reason: {reason}");
            }
            other => panic!("it should have ended up quarantined: {other:?}"),
        }
        assert!(!w.usable("com.a.b"));
    }

    #[test]
    fn one_good_call_forgives_what_came_before() {
        let mut w = Watchdog::new();
        w.failure("com.a.b", "one");
        w.failure("com.a.b", "another");
        w.success("com.a.b");
        assert_eq!(w.state("com.a.b"), State::Healthy);
        // And now it has three chances again.
        w.failure("com.a.b", "one");
        w.failure("com.a.b", "another");
        assert!(w.usable("com.a.b"));
    }

    #[test]
    fn old_failures_do_not_count() {
        let mut w = Watchdog::new();
        let now = Instant::now();
        let a_while_ago = now - WINDOW - Duration::from_secs(1);
        w.failure_at("com.a.b", "old", a_while_ago);
        w.failure_at("com.a.b", "old", a_while_ago);
        // This is the first of the current window, not the third.
        assert_eq!(w.failure_at("com.a.b", "new", now), State::Stumbling { failures: 1 });
        assert!(w.usable("com.a.b"));
    }

    #[test]
    fn only_the_user_takes_it_out_of_quarantine() {
        let mut w = Watchdog::new();
        for _ in 0..3 {
            w.failure("com.a.b", "it blows up");
        }
        assert!(!w.usable("com.a.b"));
        // Failing more changes nothing, and a success does not get it out either.
        w.failure("com.a.b", "again");
        w.success("com.a.b");
        assert!(!w.usable("com.a.b"), "a success does not take it out of quarantine");

        w.reactivate("com.a.b");
        assert_eq!(w.state("com.a.b"), State::Healthy);
    }

    #[test]
    fn some_things_go_to_quarantine_at_once() {
        let mut w = Watchdog::new();
        w.quarantine("com.a.b", "the manifest asks for an API version that does not exist");
        assert!(!w.usable("com.a.b"));
        assert_eq!(
            w.quarantined(),
            vec![(
                "com.a.b".to_string(),
                "the manifest asks for an API version that does not exist".to_string()
            )]
        );
    }

    #[test]
    fn a_broken_plugin_does_not_drag_the_others_down() {
        let mut w = Watchdog::new();
        for _ in 0..3 {
            w.failure("com.broken.plugin", "it blows up");
        }
        assert!(!w.usable("com.broken.plugin"));
        assert!(w.usable("com.healthy.plugin"), "the other one is still whole");
        assert_eq!(w.quarantined().len(), 1);
    }
}
