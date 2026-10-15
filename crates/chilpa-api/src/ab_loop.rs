//! The A-B loop: a stretch of the playing track repeated until it is let go.
//!
//! One mark sets where it starts, a second one where it ends, and from
//! then on playback goes back to the start every time it reaches the end.
//! A third mark, another track, a stop or a jump outside the stretch lets
//! it go. Nothing of it is kept between runs.
//!
//! It is watched from `poll`, not from the audio thread: the jump back
//! comes within a poll of the end, which is as close as a seek gets.

pub(crate) const COMMANDS: &[crate::commands::Command] = &[
    crate::commands::command("loop", "playback.loop", "a | b | off"),
];

use crate::Chilpa;

/// The end has to come at least this long after the start.
const SHORTEST: f64 = 2.0;
/// How close to the end the jump back is made: half of what goes by
/// between two polls, so it lands as often before the end as after it.
const LEAD: f64 = 0.125;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AbLoop {
    /// The track it belongs to.
    path: String,
    /// Where it starts, in whole seconds.
    start: f64,
    /// Where it ends, once marked.
    end: Option<f64>,
}

/// What a mark at `at` seconds of `path` leaves.
fn after_mark(current: Option<AbLoop>, path: &str, at: f64) -> Option<AbLoop> {
    match current {
        Some(marked) if marked.path == path => match marked.end {
            // The third mark lets it go.
            Some(_) => None,
            None if at >= marked.start + SHORTEST => Some(AbLoop { end: Some(at), ..marked }),
            // Too soon after the start: it stays waiting for its end.
            None => Some(marked),
        },
        _ => Some(AbLoop { path: path.to_owned(), start: at.floor(), end: None }),
    }
}

/// Whether a jump to `to` leaves the loop standing: inside it, or anywhere
/// after its start while it has no end.
fn survives(marked: &AbLoop, to: f64) -> bool {
    to >= marked.start && marked.end.is_none_or(|end| to <= end)
}

impl Chilpa {
    /// Marks the start of a loop, then its end, then lets it go. Nothing
    /// happens without a track of known length, or while casting.
    pub fn ab_mark(&mut self) {
        if self.casting.active() || !self.now.playing || self.now.total_seconds <= 0.0 {
            return;
        }
        self.ab_loop = after_mark(self.ab_loop.take(), &self.now.path, self.now.seconds.max(0.0));
    }

    pub fn ab_clear(&mut self) {
        self.ab_loop = None;
    }

    /// 0 with no loop, 1 with a start waiting for its end, 2 while looping;
    /// then where it starts and where it ends, in seconds.
    pub fn ab_state(&self) -> (i32, f64, f64) {
        match &self.ab_loop {
            None => (0, 0.0, 0.0),
            Some(AbLoop { start, end: None, .. }) => (1, *start, 0.0),
            Some(AbLoop { start, end: Some(end), .. }) => (2, *start, *end),
        }
    }

    /// A jump the listener made: outside the loop, it lets it go.
    pub(crate) fn ab_jumped(&mut self, to: f64) {
        if self.ab_loop.as_ref().is_some_and(|marked| !survives(marked, to)) {
            self.ab_loop = None;
        }
    }

    /// Goes back to the start when the end is reached, and lets the loop
    /// go when its track is no longer the one playing.
    pub(crate) fn advance_ab(&mut self) {
        let Some(marked) = &self.ab_loop else { return };
        if self.casting.active() || !self.now.playing || self.now.path != marked.path {
            self.ab_loop = None;
            return;
        }
        let (start, Some(end)) = (marked.start, marked.end) else { return };
        if !self.now.paused && self.now.seconds + LEAD >= end {
            self.seek(start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(start: f64, end: Option<f64>) -> AbLoop {
        AbLoop { path: "a.flac".into(), start, end }
    }

    #[test]
    fn three_marks_set_the_start_then_the_end_then_let_go() {
        let first = after_mark(None, "a.flac", 12.7);
        assert_eq!(first, Some(marked(12.0, None)), "the start is a whole second");
        let second = after_mark(first, "a.flac", 20.25);
        assert_eq!(second, Some(marked(12.0, Some(20.25))), "the end is exact");
        assert_eq!(after_mark(second, "a.flac", 15.0), None);
    }

    #[test]
    fn an_end_too_close_to_the_start_is_not_taken() {
        let waiting = Some(marked(12.0, None));
        assert_eq!(after_mark(waiting.clone(), "a.flac", 13.9), waiting);
        assert_eq!(after_mark(waiting, "a.flac", 14.0), Some(marked(12.0, Some(14.0))));
    }

    #[test]
    fn a_mark_on_another_track_starts_over() {
        let looping = Some(marked(12.0, Some(20.0)));
        assert_eq!(after_mark(looping, "b.flac", 3.2), Some(AbLoop { path: "b.flac".into(), start: 3.0, end: None }));
    }

    #[test]
    fn a_jump_outside_lets_the_loop_go() {
        let looping = marked(12.0, Some(20.0));
        assert!(survives(&looping, 12.0), "its own jump back");
        assert!(survives(&looping, 20.0));
        assert!(!survives(&looping, 11.0));
        assert!(!survives(&looping, 20.5));
        let waiting = marked(12.0, None);
        assert!(survives(&waiting, 300.0), "with no end, anywhere after the start");
        assert!(!survives(&waiting, 5.0));
    }
}
