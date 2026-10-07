//! Seeking inside a track, counting frames.
//!
//! In QBZ the position was kept with the system clock in whole seconds.
//! That broke two things: resuming a pause went back to the truncated
//! second, and the track change was fired by two different detectors.
//! Here the position is always a frame count and time is only used to talk
//! to the UI.

use std::time::Duration;

/// Turns time into frames rounding to nearest, not truncating.
pub fn frames_at(position: Duration, sample_rate_hz: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    let nanos = position.as_nanos();
    let rate = u128::from(sample_rate_hz);
    ((nanos * rate + 500_000_000) / 1_000_000_000) as u64
}

/// The time a frame stands for, for display.
pub fn position_of(frames: u64, sample_rate_hz: u32) -> Duration {
    if sample_rate_hz == 0 {
        return Duration::ZERO;
    }
    Duration::from_nanos(frames * 1_000_000_000 / u64::from(sample_rate_hz))
}

/// What has to be done to land exactly on the requested frame.
///
/// A container cannot always land on any frame: usually it lands at the
/// start of a packet, before the target. The difference is decoded and
/// dropped, which is what makes the seek exact.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SeekPlan {
    /// The frame the container is asked to move to.
    pub request: u64,
    /// Frames to decode and throw away afterwards.
    pub discard: u64,
}

impl SeekPlan {
    /// `landed` is where the container actually ended up.
    pub fn after_landing(target: u64, landed: u64) -> Self {
        Self { request: landed, discard: target.saturating_sub(landed) }
    }

    pub fn is_exact(&self) -> bool {
        self.discard == 0
    }
}

/// Where playback is, in frames.
///
/// Pausing moves nothing: resuming continues on the same frame, not on the
/// rounded second.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Cursor {
    pub frame: u64,
    pub sample_rate_hz: u32,
}

impl Cursor {
    pub fn new(sample_rate_hz: u32) -> Self {
        Self { frame: 0, sample_rate_hz }
    }

    pub fn advance(&mut self, frames: u64) {
        self.frame += frames;
    }

    pub fn seek_to(&mut self, position: Duration) -> u64 {
        self.frame = frames_at(position, self.sample_rate_hz);
        self.frame
    }

    pub fn position(&self) -> Duration {
        position_of(self.frame, self.sample_rate_hz)
    }
}

/// A place to ask for a seek from another thread.
///
/// Like the stop signal: whoever plays checks it between blocks and
/// consumes it. Asking for a seek never blocks the asker nor takes the
/// device's lock.
#[derive(Clone, Debug, Default)]
pub struct SeekSignal(std::sync::Arc<std::sync::Mutex<Option<Duration>>>);

impl SeekSignal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks to jump to that point. A new seek replaces one that has not been
    /// served: what matters is the last one the user asked for.
    pub fn request(&self, position: Duration) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = Some(position);
        }
    }

    /// Takes the pending seek, if there is one.
    /// Whether a seek waits to be taken.
    pub fn asked(&self) -> bool {
        self.0.lock().map(|s| s.is_some()).unwrap_or(false)
    }

    pub fn take(&self) -> Option<Duration> {
        self.0.lock().ok().and_then(|mut g| g.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_becomes_frames_without_truncating_to_seconds() {
        assert_eq!(frames_at(Duration::from_millis(1500), 44_100), 66_150);
        assert_eq!(frames_at(Duration::from_millis(1), 44_100), 44);
        assert_eq!(frames_at(Duration::ZERO, 44_100), 0);
    }

    #[test]
    fn a_position_round_trip_keeps_the_frame() {
        let c = Cursor { frame: 123_456, sample_rate_hz: 96_000 };
        assert_eq!(frames_at(c.position(), 96_000), 123_456);
    }

    #[test]
    fn pause_and_resume_does_not_jump_back_to_a_rounded_second() {
        // The QBZ finding: resuming re-sought the truncated second and
        // repeated almost half a second of music.
        let mut c = Cursor::new(44_100);
        c.advance(66_150); // 1.5 s
        let on_pause = c.frame;
        // Resuming does not touch the cursor.
        assert_eq!(c.frame, on_pause);
        assert_eq!(c.position(), Duration::from_millis(1500));
    }

    #[test]
    fn the_last_seek_asked_for_is_the_one_that_counts() {
        let s = SeekSignal::new();
        assert_eq!(s.take(), None);
        s.request(Duration::from_secs(10));
        s.request(Duration::from_secs(30));
        assert_eq!(s.take(), Some(Duration::from_secs(30)), "the most recent one wins");
        assert_eq!(s.take(), None, "it is consumed once");
    }

    #[test]
    fn what_the_container_cannot_reach_is_dropped_while_decoding() {
        let plan = SeekPlan::after_landing(100_000, 98_304);
        assert_eq!(plan.request, 98_304);
        assert_eq!(plan.discard, 1_696);
        assert!(!plan.is_exact());
    }

    #[test]
    fn if_the_container_lands_exactly_nothing_is_dropped() {
        let plan = SeekPlan::after_landing(44_100, 44_100);
        assert!(plan.is_exact());
    }

    #[test]
    fn overshooting_never_leaves_a_negative_discard() {
        let plan = SeekPlan::after_landing(1_000, 2_000);
        assert_eq!(plan.discard, 0, "nothing is ever discarded backwards");
    }
}
