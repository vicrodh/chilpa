//! The plays the facade has asked of the player and not yet seen end.
//!
//! Starting another play does not wait for the old one: it is told to stop
//! and the new one is queued behind it, so what the old play said last -- a
//! track it passed over, a track that started, how it ended -- is often read
//! after the new launch went out. Every event carries the number of the play
//! that said it, and every play ends with one event, the ones that never
//! started too. So each play is kept here from its launch to that event,
//! with what became of it and the listens it opened.
//!
//! Two things follow from what became of a play. What it says reaches the
//! window only while it is the one in front of it: a failure of an old play
//! is not the new one's, its end is not the new queue running out, and a
//! track it started is not what plays now. And the listens it opened are
//! settled by it, whatever plays now: written, or handed to the play that
//! took the output over (`ledger.rs`).

pub(crate) mod ledger;

use std::collections::VecDeque;

use ledger::{Ledger, Written};

use crate::{Chilpa, InProgress, Relaunch};

/// What became of a play. It starts as the one in front of the window and
/// leaves that once, for one of the others.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Fate {
    /// The play in front of the window.
    Current,
    /// Another play took its place.
    Replaced(Replaced),
    /// The listener stopped it, or nothing was left to play.
    Stopped,
    /// Its player was handed away with it still playing: the music goes on
    /// somewhere the facade does not hear from.
    Abandoned,
}

/// Why a play was replaced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Replaced {
    /// Somebody chose another track: what was left was left by choice.
    Track,
    /// The same track goes on at another output, as the play `by`.
    Output { by: u64 },
}

/// One launch the player was asked for, until its last event is read.
pub(crate) struct Play {
    pub id: u64,
    /// The count of the queue's rows it was launched on.
    pub epoch: u64,
    /// Where in the queue it was launched from.
    pub from: usize,
    /// Whether a track of it has started in front of the window.
    pub started: bool,
    pub fate: Fate,
    pub ledger: Ledger<InProgress>,
}

/// The plays not yet seen to end, oldest first. At most one is in front of
/// the window, and it is the newest.
#[derive(Default)]
pub(crate) struct Plays(VecDeque<Play>);

impl Plays {
    pub fn get(&self, id: u64) -> Option<&Play> {
        self.0.iter().find(|play| play.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Play> {
        self.0.iter_mut().find(|play| play.id == id)
    }

    /// The play in front of the window.
    pub fn current(&self) -> Option<&Play> {
        self.0.back().filter(|play| play.fate == Fate::Current)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether a listen is open, or waits to be picked up.
    pub fn listening(&self) -> bool {
        self.0.iter().any(|play| !play.ledger.is_empty())
    }

    /// The play in front of the window leaves it, as `fate`. What became of
    /// a play is said once: one that already left keeps what it was.
    pub fn retire(&mut self, fate: Fate) -> Option<u64> {
        let play = self.0.back_mut().filter(|play| play.fate == Fate::Current)?;
        play.fate = fate;
        Some(play.id)
    }

    /// A launch went out: it is the play in front of the window.
    pub fn push(&mut self, id: u64, from: usize, epoch: u64) {
        self.0.push_back(Play { id, epoch, from, started: false, fate: Fate::Current, ledger: Ledger::default() });
    }

    fn take(&mut self, id: u64) -> Option<Play> {
        let at = self.0.iter().position(|play| play.id == id)?;
        self.0.remove(at)
    }

    fn oldest(&self) -> Option<u64> {
        self.0.front().map(|play| play.id)
    }
}

impl Chilpa {
    /// A launch went out as the play `id`, from `from` in the queue. The
    /// play that was in front of the window leaves it for this one.
    pub(crate) fn launched(&mut self, id: u64, from: usize, why: Relaunch) {
        let replaced = match why {
            Relaunch::Output => Replaced::Output { by: id },
            Relaunch::Track => Replaced::Track,
        };
        self.plays.retire(Fate::Replaced(replaced));
        self.plays.push(id, from, self.rows_epoch);
        self.launch_counted();
    }

    /// Writes a listen the ledger closed. `shown` is whether its play is in
    /// front of the window: only then is a failure to save it said there.
    pub(crate) fn record(&mut self, written: Written<InProgress>, shown: bool) {
        let Written { mut listen, skipped } = written;
        // The count it was closed with may be the one that makes it count.
        crate::scrobble::if_heard(self.scrobbler.as_ref(), &mut listen);
        self.record_listen(listen, skipped, shown);
    }

    /// The play said its last: what it still holds is written, or handed to
    /// the play that took its output over, and it is forgotten.
    pub(crate) fn settle_play(&mut self, id: u64, shown: bool) {
        let Some(mut play) = self.plays.take(id) else { return };
        let heir = match play.fate {
            Fate::Replaced(Replaced::Output { by }) => self.plays.get(by).map(|_| by),
            _ => None,
        };
        let settled = play.ledger.terminal(play.fate, heir.is_some());
        for written in settled.written {
            self.record(written, shown);
        }
        if let (Some(listen), Some(by)) = (settled.handed, heir) {
            let left = self.plays.get_mut(by).and_then(|heir| heir.ledger.carry(listen));
            if let Some(left) = left {
                self.record(left, false);
            }
        }
    }

    /// Settles every play that is left, oldest first, as if each had said
    /// its last: nothing more will be read from the player.
    pub(crate) fn settle_plays(&mut self) {
        while let Some(id) = self.plays.oldest() {
            self.settle_play(id, false);
        }
    }

    /// Takes the player out of the facade, which nothing else does, with
    /// every play settled as it returns: nothing is read from it afterwards.
    ///
    /// Drained, the player is closed first, so each of its plays has said
    /// how it ended and all of it is read. Abandoned, the player goes on to
    /// whoever takes it: the play in front of the window leaves as abandoned,
    /// the player is told to stop, and what it has said so far is read once
    /// more. That read is the cut. What a play still holds after it is
    /// written with what is known: a track nobody closed as a transfer, with
    /// the count of its last progress, unless it was already known to have
    /// broken. Whatever the player says after the cut is lost: a track that
    /// breaks or a device that fails in that last moment is written as a
    /// transfer too.
    pub(crate) fn retire_player(&mut self, how: Leaving) -> Option<chilpa_core::player::Player> {
        let mut player = self.player.take()?;
        match how {
            Leaving::Drained => {
                self.listen.pending = None;
                player.shutdown();
            }
            Leaving::Abandoned => {
                self.plays.retire(Fate::Abandoned);
                player.stop();
            }
        }
        self.leave_plays(player.poll_tagged());
        (how == Leaving::Abandoned).then_some(player)
    }

    /// The last a player says of its plays, and then whatever they still
    /// hold, oldest first, as if each had said how it ended.
    pub(crate) fn leave_plays(&mut self, last: Vec<(u64, chilpa_core::player::PlayerEvent)>) {
        for (play, event) in last {
            self.apply_player_event(play, event, &mut false);
        }
        self.settle_plays();
    }
}

/// How a player leaves the facade.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Leaving {
    /// It is closed and everything it said is read.
    Drained,
    /// It goes on somewhere else, with what plays still playing.
    Abandoned,
}
