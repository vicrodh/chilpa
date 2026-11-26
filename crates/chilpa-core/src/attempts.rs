//! The attempts at a track a play has announced and still owes an ending.
//!
//! A track that started is closed exactly once, whatever becomes of the play:
//! with how it ended and how much of it left the device. The two are known
//! at different moments. How it ended is known when its source runs out,
//! breaks or is stopped; how much was heard only when the audio it left in
//! the device has played or been thrown away, which may be well into the
//! next track. So the ending is kept from the moment it is known, and the
//! track is closed when its audio is accounted for, or when the play ends.

use std::collections::VecDeque;

use chilpa_audio::render::Outcome;
use chilpa_audio::sink::StopSignal;

use crate::playback_events::{Lost, Report};

/// How a track that started ended. In order: what is known of an ending is
/// only ever raised, so a stop that throws away the last of a track that
/// broke does not make it a track that was merely stopped.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum How {
    /// It played to its end.
    Completed,
    /// The listener went on to the next one.
    Advanced,
    /// It was stopped, or the play was replaced, before it ended.
    Stopped,
    /// Its own data could not be read on: it broke partway.
    Broken,
    /// The output failed with its audio.
    Failed,
}

impl How {
    /// What a render's outcome says of the track, and whether the listener
    /// skipped it.
    pub(crate) fn of(outcome: &Outcome, skipped: bool) -> Self {
        match outcome {
            Outcome::Finished { .. } if skipped => How::Advanced,
            Outcome::Finished { .. } => How::Completed,
            Outcome::Stopped { .. } => How::Stopped,
            Outcome::Unreadable { .. } => How::Broken,
            Outcome::Failed { .. } => How::Failed,
        }
    }
}

/// A track that started, closed: its turn in the play, its place in the
/// queue, how many frames of it left the device, how it ended, and whether
/// volume, normalization or DSP changed any of its samples on the way out,
/// which its start could not know when they changed later.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Closed {
    pub attempt: u64,
    pub index: usize,
    pub heard: u64,
    pub how: How,
    pub processed: bool,
}

struct Open {
    attempt: u64,
    index: usize,
    how: Option<How>,
    heard: u64,
    processed: bool,
}

pub(crate) struct Attempts<'a> {
    tell: &'a mut dyn FnMut(Closed),
    report: Report,
    /// The play's own stop: what it throws away of the last track is that
    /// track stopped.
    stop: StopSignal,
    open: VecDeque<Open>,
}

impl<'a> Attempts<'a> {
    pub fn new(tell: &'a mut dyn FnMut(Closed), stop: StopSignal) -> Self {
        Self { tell, report: Report::default(), stop, open: VecDeque::new() }
    }

    /// Where the meters of this play's device say what is accounted for.
    pub fn report(&self) -> Report {
        self.report.clone()
    }

    /// A track was announced: it is owed an ending from here on, device or
    /// no device.
    pub fn open(&mut self, attempt: u64, index: usize) {
        self.open.push_back(Open { attempt, index, how: None, heard: 0, processed: false });
    }

    /// Something on the way out changed samples of `attempt`.
    pub fn processed(&mut self, attempt: u64) {
        if let Some(open) = self.open.iter_mut().find(|open| open.attempt == attempt) {
            open.processed = true;
        }
    }

    /// How `attempt` ended, as far as is known now. Never lowered.
    pub fn ended(&mut self, attempt: u64, how: How) {
        if let Some(open) = self.open.iter_mut().find(|open| open.attempt == attempt) {
            open.how = Some(open.how.map_or(how, |known| known.max(how)));
        }
    }

    /// What of `attempt` has left the device so far, when it is the newest.
    pub fn heard(&mut self, attempt: u64) -> Option<u64> {
        let heard = self.report.played(attempt)?;
        if let Some(open) = self.open.iter_mut().find(|open| open.attempt == attempt) {
            open.heard = heard;
        }
        Some(heard)
    }

    /// Closes the tracks whose audio is accounted for.
    pub fn settle(&mut self) {
        for retired in self.report.take() {
            let Some(at) = self.open.iter().position(|open| open.attempt == retired.attempt) else { continue };
            let last = at + 1 == self.open.len();
            let Some(mut open) = self.open.remove(at) else { continue };
            open.heard = retired.played;
            match retired.lost {
                // The output failed with what it still held of it.
                Lost::Error => open.how = Some(open.how.map_or(How::Failed, |known| known.max(How::Failed))),
                // The play was stopped with the last of its last track still
                // in the device: that track was stopped.
                Lost::Cut if last && self.stop.is_stopped() => {
                    open.how = Some(open.how.map_or(How::Stopped, |known| known.max(How::Stopped)));
                }
                _ => {}
            }
            self.close(open);
        }
    }

    fn close(&mut self, open: Open) {
        // An ending nobody could say is the play breaking under it.
        (self.tell)(Closed { attempt: open.attempt, index: open.index, heard: open.heard, how: open.how.unwrap_or(How::Failed), processed: open.processed });
    }
}

impl Drop for Attempts<'_> {
    /// The play is over, or unwinding: whatever is accounted for is closed
    /// with its count, and whatever is not, with the last one known.
    fn drop(&mut self) {
        self.settle();
        while let Some(open) = self.open.pop_front() {
            self.close(open);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_known_of_an_ending_is_only_raised() {
        let mut said = Vec::new();
        let mut tell = |closed: Closed| said.push(closed);
        let stop = StopSignal::new();
        {
            let mut attempts = Attempts::new(&mut tell, stop.clone());
            attempts.open(1, 0);
            attempts.ended(1, How::Broken);
            attempts.ended(1, How::Stopped);
            attempts.open(2, 1);
            attempts.ended(2, How::Completed);
            attempts.ended(2, How::Failed);
            attempts.open(3, 2);
        }
        assert_eq!(said.iter().map(|closed| (closed.attempt, closed.how)).collect::<Vec<_>>(),
            [(1, How::Broken), (2, How::Failed), (3, How::Failed)],
            "a stop does not lower a break, a failure is kept, and an ending nobody said is a failure");
    }

    #[test]
    fn a_track_is_closed_as_processed_only_when_its_own_samples_were() {
        let mut said = Vec::new();
        let mut tell = |closed: Closed| said.push(closed);
        {
            let mut attempts = Attempts::new(&mut tell, StopSignal::new());
            attempts.open(1, 0);
            attempts.open(2, 1);
            attempts.processed(2);
            attempts.processed(7);
        }
        assert_eq!(said.iter().map(|closed| (closed.attempt, closed.processed)).collect::<Vec<_>>(), [(1, false), (2, true)]);
    }

    #[test]
    fn the_order_of_endings_is_failed_over_broken_over_stopped_over_played() {
        assert!(How::Failed > How::Broken && How::Broken > How::Stopped && How::Stopped > How::Advanced && How::Advanced > How::Completed);
    }
}
