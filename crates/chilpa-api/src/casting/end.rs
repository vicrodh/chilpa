use chilpa_cast::{State, Status};
use std::time::{Duration, Instant};

pub(super) struct End {
    started: Instant, seen_playing: bool, maximum: f64, previous: f64, stopped: u8, frozen: u8, duration: f64,
}
impl End {
    pub fn new(duration: f64) -> Self { Self { started: Instant::now(), seen_playing: false, maximum: 0.0, previous: 0.0, stopped: 0, frozen: 0, duration } }
    pub fn tick(&mut self, status: &Status) -> bool {
        if status.state == State::Finished { return true; }
        if let Some(duration) = status.duration.filter(|d| *d > 0.0) { self.duration = duration; }
        if status.state == State::Playing { self.seen_playing = true; }
        let position = status.position.unwrap_or(0.0);
        self.maximum = self.maximum.max(position);
        let near = self.duration > 0.0 && self.maximum >= (self.duration - 5.0).max(1.0);
        if status.state == State::Paused || status.state == State::Buffering { self.frozen = 0; self.stopped = 0; return false; }
        self.stopped = if status.state == State::Stopped { self.stopped.saturating_add(1) } else { 0 };
        self.frozen = if near && (position - self.previous).abs() < 0.5 { self.frozen.saturating_add(1) } else { 0 };
        self.previous = position;
        self.seen_playing && self.started.elapsed() >= Duration::from_secs(3) && ((near && self.stopped >= 2) || self.stopped >= 4 || self.frozen >= 5)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transient_stop_pause_and_frozen_end_are_distinct() {
        let mut end = End::new(100.0); end.started = Instant::now() - Duration::from_secs(10);
        assert!(!end.tick(&Status { state: State::Stopped, ..Default::default() }));
        assert!(!end.tick(&Status { state: State::Playing, position: Some(96.0), ..Default::default() }));
        assert!(!end.tick(&Status { state: State::Stopped, ..Default::default() }));
        for _ in 0..10 { assert!(!end.tick(&Status { state: State::Paused, position: Some(96.0), ..Default::default() })); }
        let mut ended = false;
        for _ in 0..6 { ended |= end.tick(&Status { state: State::Playing, position: Some(96.0), ..Default::default() }); }
        assert!(ended);
    }
}
