//! Receiver positions measure remote listening; serving bytes is only buffering.
use chilpa_cast::{State, Status};
use chilpa_core::playback_events::Observer;
use std::{sync::Arc, time::Instant};

pub(super) struct Listening {
    observer: Arc<dyn Observer>,
    previous: Option<(Instant, f64, bool)>,
    seconds: f64,
    ended: bool,
}
impl Listening {
    pub fn new(observer: Arc<dyn Observer>) -> Self { Self { observer, previous: None, seconds: 0.0, ended: false } }
    pub fn seek(&mut self) { self.previous = None; }
    pub fn tick(&mut self, status: &Status, now: Instant) {
        let Some(position) = status.position.filter(|p| p.is_finite() && *p >= 0.0) else { self.previous = None; return; };
        if let Some((at, before, playing)) = self.previous {
            let delta = position - before;
            let elapsed = now.duration_since(at).as_secs_f64();
            if playing && delta > 0.0 && delta <= elapsed + 2.0 {
                self.seconds += delta.min(elapsed);
            }
        }
        self.previous = Some((now, position, status.state == State::Playing));
        if self.seconds > 0.0 {
            self.observer.update_at((self.seconds * 1000.0) as u64, 1000,
                if status.state == State::Paused { "paused" } else { "playing" }, None, Some((position * 1000.0) as u64));
        }
    }
    pub fn finish(&mut self, reason: &'static str) {
        if !self.ended && self.seconds > 0.0 {
            self.observer.update_at((self.seconds * 1000.0) as u64, 1000, "playing", Some(reason), self.previous.map(|(_, p, _)| (p * 1000.0) as u64));
        }
        self.ended = true;
    }
}
impl Drop for Listening { fn drop(&mut self) { self.finish("stopped"); } }

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Mutex, time::Duration};
    #[derive(Default)]
    struct Recorded(Mutex<Vec<(u64, Option<&'static str>)>>);
    impl Observer for Recorded {
        fn update(&self, frames: u64, rate: u32, _: &'static str, reason: Option<&'static str>) {
            assert_eq!(rate, 1000);
            self.0.lock().unwrap().push((frames, reason));
        }
    }
    #[test]
    fn receiver_reports_exclude_loads_seeks_pauses_and_buffering() {
        let recorded = Arc::new(Recorded::default());
        let mut listening = Listening::new(recorded.clone());
        let start = Instant::now();
        let status = |state, position| Status { state, position: Some(position), ..Default::default() };
        listening.tick(&status(State::Playing, 0.0), start);
        assert!(recorded.0.lock().unwrap().is_empty());
        listening.tick(&status(State::Playing, 1.0), start + Duration::from_secs(1));
        listening.seek();
        listening.tick(&status(State::Playing, 50.0), start + Duration::from_secs(2));
        listening.tick(&status(State::Paused, 51.0), start + Duration::from_secs(3));
        listening.tick(&status(State::Paused, 51.0), start + Duration::from_secs(10));
        listening.tick(&status(State::Buffering, 51.0), start + Duration::from_secs(11));
        listening.tick(&status(State::Playing, 51.0), start + Duration::from_secs(12));
        listening.tick(&status(State::Finished, 52.0), start + Duration::from_secs(13));
        listening.finish("finished");
        drop(listening);
        let events = recorded.0.lock().unwrap();
        assert_eq!(events.last(), Some(&(3000, Some("finished"))));
        assert_eq!(events.iter().filter(|(_, end)| end.is_some()).count(), 1);
    }
}
