//! The listens of one play: opened as a track starts, written as it is
//! closed, whatever became of the play in the meantime.
//!
//! Nothing here reads a clock, the library or the window. What goes in is
//! what the player said and what became of the play; what comes out is the
//! listens to write, each with whether it was cut short. A listen that is
//! moving to another output is neither: it is parked, handed to the play
//! that takes over when this one ends, and picked up there by the same track
//! starting, or written as interrupted.

use std::collections::VecDeque;
use std::path::Path;

use chilpa_core::attempts::How;

use super::{Fate, Replaced};

/// What the ledger needs of a listen.
pub(crate) trait Heard {
    fn path(&self) -> &Path;
    /// How many frames of it left the device in the play that holds it.
    fn set_frames(&mut self, frames: u64);
}

/// What a close does with a listen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Verdict {
    Write { skipped: bool },
    /// Kept for the play that takes over at another output.
    Park,
}

/// How a listen ends, from how its track ended and what became of its play.
pub(crate) fn verdict(how: How, fate: Fate) -> Verdict {
    match (how, fate) {
        (How::Completed | How::Advanced, _) => Verdict::Write { skipped: false },
        (How::Broken | How::Failed, _) => Verdict::Write { skipped: true },
        (How::Stopped, Fate::Replaced(Replaced::Output { .. })) => Verdict::Park,
        // The music went on somewhere else: nobody left the track.
        (How::Stopped, Fate::Abandoned) => Verdict::Write { skipped: false },
        (How::Stopped, _) => Verdict::Write { skipped: true },
    }
}

/// A listen to write.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Written<L> {
    pub listen: L,
    pub skipped: bool,
}

/// What a track starting did to the listen carried in from another output.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Began<L> {
    /// The carried listen was of this track and goes on as this attempt:
    /// no new one is opened.
    pub resumed: bool,
    /// The carried listen was of another track.
    pub written: Option<Written<L>>,
}

/// What the end of a play leaves.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Settled<L> {
    pub written: Vec<Written<L>>,
    /// The listen that goes on in the play that took over.
    pub handed: Option<L>,
}

struct Open<L> {
    attempt: u64,
    listen: L,
    /// What is already known of how it ended, before it is closed.
    known: Option<How>,
}

pub(crate) struct Ledger<L> {
    /// Handed over by the play this one took the output from.
    carried: Option<L>,
    open: VecDeque<Open<L>>,
    parked: Option<L>,
}

impl<L> Default for Ledger<L> {
    fn default() -> Self {
        Self { carried: None, open: VecDeque::new(), parked: None }
    }
}

impl<L: Heard> Ledger<L> {
    /// Takes the listen the play before this one left to be picked up.
    pub fn carry(&mut self, listen: L) -> Option<Written<L>> {
        // Two cannot wait at once; one that somehow did was cut short.
        self.carried.replace(listen).map(|listen| Written { listen, skipped: true })
    }

    /// A track started. A carried listen of the same track goes on as this
    /// attempt; one of another track was left. When nothing is resumed the
    /// caller opens a listen with `open`.
    pub fn started(&mut self, attempt: u64, path: &Path) -> Began<L> {
        match self.carried.take() {
            Some(listen) if listen.path() == path => {
                self.open.push_back(Open { attempt, listen, known: None });
                Began { resumed: true, written: None }
            }
            Some(listen) => Began { resumed: false, written: Some(Written { listen, skipped: true }) },
            None => Began { resumed: false, written: None },
        }
    }

    pub fn open(&mut self, attempt: u64, listen: L) {
        self.open.push_back(Open { attempt, listen, known: None });
    }

    /// What the newest track has played so far.
    pub fn progress(&mut self, played: u64) -> Option<&mut L> {
        let newest = self.open.back_mut()?;
        newest.listen.set_frames(played);
        Some(&mut newest.listen)
    }

    /// How `attempt` ended is known before it is closed. Only ever raised.
    pub fn known(&mut self, attempt: u64, how: How) {
        if let Some(open) = self.open.iter_mut().find(|open| open.attempt == attempt) {
            open.known = Some(open.known.map_or(how, |known| known.max(how)));
        }
    }

    /// The listen of `attempt`, while it is open.
    pub fn listen_mut(&mut self, attempt: u64) -> Option<&mut L> {
        self.open.iter_mut().find(|open| open.attempt == attempt).map(|open| &mut open.listen)
    }

    /// The listen of the track that started last, while it is open.
    #[cfg(test)]
    pub fn newest(&self) -> Option<&L> {
        self.open.back().map(|open| &open.listen)
    }

    /// The listen kept for another output, or the one carried in and not
    /// picked up yet.
    #[cfg(test)]
    pub fn waiting_mut(&mut self) -> Option<&mut L> {
        self.parked.as_mut().or(self.carried.as_mut())
    }

    /// Whether nothing is open, carried or parked.
    pub fn is_empty(&self) -> bool {
        self.open.is_empty() && self.carried.is_none() && self.parked.is_none()
    }

    /// `attempt` is over: `heard` frames of it left the device, and it ended
    /// as `how`, or as something worse already known of it.
    pub fn closed(&mut self, attempt: u64, heard: u64, how: How, fate: Fate) -> Vec<Written<L>> {
        let Some(at) = self.open.iter().position(|open| open.attempt == attempt) else { return Vec::new() };
        let Some(mut open) = self.open.remove(at) else { return Vec::new() };
        open.listen.set_frames(heard);
        let how = open.known.map_or(how, |known| known.max(how));
        self.settle_one(open.listen, how, fate)
    }

    /// The play ended. A listen still open is closed with what is known of
    /// it, or as stopped, with its last count; what was carried in and never
    /// picked up, or parked, goes to the play that took the output over when
    /// `heir` says it is still there, and is written otherwise.
    pub fn terminal(&mut self, fate: Fate, heir: bool) -> Settled<L> {
        let mut written = Vec::new();
        while let Some(open) = self.open.pop_front() {
            let how = open.known.unwrap_or(How::Stopped);
            written.extend(self.settle_one(open.listen, how, fate));
        }
        let moving = matches!(fate, Fate::Replaced(Replaced::Output { .. })) && heir;
        let skipped = fate != Fate::Abandoned;
        let (carried, parked) = (self.carried.take(), self.parked.take());
        let handed = match (parked, carried) {
            (Some(parked), carried) if moving => {
                written.extend(carried.map(|listen| Written { listen, skipped: true }));
                Some(parked)
            }
            (None, Some(carried)) if moving => Some(carried),
            (parked, carried) => {
                written.extend(carried.into_iter().chain(parked).map(|listen| Written { listen, skipped }));
                None
            }
        };
        Settled { written, handed }
    }

    fn settle_one(&mut self, listen: L, how: How, fate: Fate) -> Vec<Written<L>> {
        match verdict(how, fate) {
            Verdict::Write { skipped } => vec![Written { listen, skipped }],
            // One already parked was left behind by the one that follows it.
            Verdict::Park => self.parked.replace(listen).map(|listen| Written { listen, skipped: true }).into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Listen {
        path: PathBuf,
        frames: u64,
    }

    impl Heard for Listen {
        fn path(&self) -> &Path {
            &self.path
        }
        fn set_frames(&mut self, frames: u64) {
            self.frames = frames;
        }
    }

    fn listen(path: &str) -> Listen {
        Listen { path: PathBuf::from(path), frames: 0 }
    }

    fn row(path: &str, frames: u64, skipped: bool) -> Written<Listen> {
        Written { listen: Listen { path: PathBuf::from(path), frames }, skipped }
    }

    const MOVED: Fate = Fate::Replaced(Replaced::Output { by: 2 });
    const LEFT: Fate = Fate::Replaced(Replaced::Track);
    const EVERY_FATE: [Fate; 5] = [Fate::Current, LEFT, MOVED, Fate::Stopped, Fate::Abandoned];

    /// A play that started `path` as its first track.
    fn playing(path: &str) -> Ledger<Listen> {
        let mut ledger = Ledger::default();
        assert_eq!(ledger.started(1, Path::new(path)), Began { resumed: false, written: None });
        ledger.open(1, listen(path));
        ledger
    }

    #[test]
    fn a_track_that_played_or_was_passed_is_written_whole_whatever_became_of_the_play() {
        for fate in EVERY_FATE {
            for how in [How::Completed, How::Advanced] {
                assert_eq!(verdict(how, fate), Verdict::Write { skipped: false }, "{how:?} {fate:?}");
            }
        }
    }

    #[test]
    fn a_track_that_broke_or_lost_its_output_is_cut_short_whatever_became_of_the_play() {
        for fate in EVERY_FATE {
            for how in [How::Broken, How::Failed] {
                assert_eq!(verdict(how, fate), Verdict::Write { skipped: true }, "{how:?} {fate:?}");
            }
        }
    }

    #[test]
    fn a_stopped_track_is_judged_by_what_became_of_its_play() {
        assert_eq!(verdict(How::Stopped, MOVED), Verdict::Park, "it goes on at another output");
        assert_eq!(verdict(How::Stopped, Fate::Abandoned), Verdict::Write { skipped: false }, "it goes on where the player went");
        for fate in [Fate::Current, LEFT, Fate::Stopped] {
            assert_eq!(verdict(How::Stopped, fate), Verdict::Write { skipped: true }, "{fate:?}");
        }
    }

    #[test]
    fn a_close_writes_the_count_the_device_gave_not_the_last_progress() {
        let mut ledger = playing("/a");
        ledger.progress(300);
        assert_eq!(ledger.closed(1, 500, How::Completed, Fate::Current), [row("/a", 500, false)]);
        assert!(ledger.is_empty());
        assert_eq!(ledger.closed(1, 500, How::Completed, Fate::Current), [], "closed once");
    }

    #[test]
    fn a_track_is_closed_after_the_next_one_started_and_progress_is_the_newest_tracks() {
        let mut ledger = playing("/a");
        ledger.progress(400);
        ledger.started(2, Path::new("/b"));
        ledger.open(2, listen("/b"));
        ledger.progress(10);
        assert_eq!(ledger.closed(1, 450, How::Completed, Fate::Current), [row("/a", 450, false)]);
        assert_eq!(ledger.closed(2, 90, How::Advanced, Fate::Current), [row("/b", 90, false)]);
    }

    #[test]
    fn a_break_known_early_is_not_lowered_by_the_stop_that_closes_it() {
        let mut ledger = playing("/a");
        ledger.known(1, How::Broken);
        ledger.known(1, How::Stopped);
        assert_eq!(ledger.closed(1, 70, How::Stopped, MOVED), [row("/a", 70, true)], "a broken track is not carried to another output");
        let mut ledger = playing("/a");
        ledger.known(1, How::Broken);
        assert_eq!(ledger.closed(1, 70, How::Failed, Fate::Current), [row("/a", 70, true)]);
    }

    #[test]
    fn a_play_that_ends_with_a_listen_open_closes_it_with_what_is_known_or_as_stopped() {
        let mut stopped = playing("/a");
        stopped.progress(200);
        assert_eq!(stopped.terminal(Fate::Stopped, false), Settled { written: vec![row("/a", 200, true)], handed: None });
        let mut broken = playing("/a");
        broken.progress(200);
        broken.known(1, How::Broken);
        assert_eq!(broken.terminal(Fate::Abandoned, false).written, [row("/a", 200, true)], "known before the player left");
        let mut transferred = playing("/a");
        transferred.progress(200);
        assert_eq!(transferred.terminal(Fate::Abandoned, false).written, [row("/a", 200, false)], "the music went on elsewhere");
    }

    #[test]
    fn only_the_last_track_of_a_play_that_ends_unclosed_goes_on_at_another_output() {
        let mut ledger = playing("/a");
        ledger.progress(100);
        ledger.started(2, Path::new("/b"));
        ledger.open(2, listen("/b"));
        ledger.progress(40);
        let settled = ledger.terminal(MOVED, true);
        assert_eq!(settled.written, [row("/a", 100, true)], "the earlier one is not carried");
        assert_eq!(settled.handed, Some(Listen { path: "/b".into(), frames: 40 }), "the last one goes on");
    }

    #[test]
    fn a_move_parks_the_listen_and_hands_it_to_the_play_that_took_over() {
        let mut first = playing("/a");
        assert_eq!(first.closed(1, 300, How::Stopped, MOVED), [], "kept, not written");
        assert!(!first.is_empty());
        let settled = first.terminal(MOVED, true);
        assert_eq!(settled, Settled { written: vec![], handed: Some(Listen { path: "/a".into(), frames: 300 }) });
        let mut second = Ledger::default();
        assert_eq!(second.carry(settled.handed.unwrap()), None);
        assert_eq!(second.started(1, Path::new("/a")), Began { resumed: true, written: None });
        second.progress(200);
        // The listen is one: what it carries from the first output is its own to add up.
        assert_eq!(second.closed(1, 250, How::Completed, Fate::Current), [row("/a", 250, false)]);
    }

    #[test]
    fn a_move_whose_successor_is_gone_writes_the_listen_as_cut_short() {
        let mut first = playing("/a");
        first.closed(1, 300, How::Stopped, MOVED);
        assert_eq!(first.terminal(MOVED, false), Settled { written: vec![row("/a", 300, true)], handed: None });
    }

    #[test]
    fn two_moves_and_then_another_track_write_the_listen_once_as_left() {
        // The first play is heard, and moved.
        let mut first = playing("/a");
        first.closed(1, 300, How::Stopped, MOVED);
        let parked = first.terminal(MOVED, true).handed.unwrap();
        // The second never starts and is moved again.
        let mut second = Ledger::default();
        second.carry(parked);
        let again = Fate::Replaced(Replaced::Output { by: 3 });
        let settled = second.terminal(again, true);
        assert_eq!(settled.written, []);
        // The third is replaced by another track before it starts.
        let mut third = Ledger::default();
        third.carry(settled.handed.unwrap());
        assert_eq!(third.terminal(LEFT, false), Settled { written: vec![row("/a", 300, true)], handed: None });
        // The fourth opens its own.
        let mut fourth = playing("/b");
        assert_eq!(fourth.closed(1, 10, How::Completed, Fate::Current), [row("/b", 10, false)]);
    }

    #[test]
    fn a_move_then_a_stop_writes_the_listen_as_cut_short_and_leaves_nothing_to_pick_up() {
        let mut first = playing("/a");
        first.closed(1, 300, How::Stopped, MOVED);
        let parked = first.terminal(MOVED, true).handed.unwrap();
        let mut second = Ledger::default();
        second.carry(parked);
        assert_eq!(second.terminal(Fate::Stopped, false), Settled { written: vec![row("/a", 300, true)], handed: None });
        // The same track played again later is a listen of its own.
        let mut third = Ledger::<Listen>::default();
        assert_eq!(third.started(1, Path::new("/a")), Began { resumed: false, written: None });
    }

    #[test]
    fn a_successor_cancelled_before_starting_hands_on_or_writes_what_it_carried() {
        let mut moved_on = Ledger::default();
        moved_on.carry(Listen { path: "/a".into(), frames: 300 });
        assert_eq!(moved_on.terminal(MOVED, true).handed, Some(Listen { path: "/a".into(), frames: 300 }));
        let mut abandoned = Ledger::default();
        abandoned.carry(Listen { path: "/a".into(), frames: 300 });
        assert_eq!(abandoned.terminal(Fate::Abandoned, false).written, [row("/a", 300, false)]);
    }

    #[test]
    fn a_carried_listen_is_not_picked_up_by_another_track() {
        let mut ledger = Ledger::default();
        ledger.carry(Listen { path: "/a".into(), frames: 300 });
        // The reopen failed and the queue went on to the next track.
        let began = ledger.started(1, Path::new("/b"));
        assert_eq!(began, Began { resumed: false, written: Some(row("/a", 300, true)) });
        ledger.open(1, listen("/b"));
        assert_eq!(ledger.closed(1, 20, How::Completed, Fate::Current), [row("/b", 20, false)]);
        assert!(ledger.is_empty());
    }

    #[test]
    fn what_is_said_of_a_track_that_is_not_open_changes_nothing() {
        let mut ledger = playing("/a");
        ledger.known(9, How::Broken);
        assert_eq!(ledger.closed(9, 5, How::Failed, Fate::Current), []);
        assert_eq!(ledger.closed(1, 5, How::Completed, Fate::Current), [row("/a", 5, false)]);
        assert!(ledger.progress(1).is_none(), "nothing is open");
        assert_eq!(ledger.terminal(Fate::Current, false), Settled { written: vec![], handed: None });
    }
}
