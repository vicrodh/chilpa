//! Listening facts measured at the output, independently of the UI cursor.
use chilpa_audio::{format::StreamFormat, sink::{AudioSink, SinkError}};
use std::{cell::RefCell, collections::VecDeque, sync::Arc};

/// Implementations enqueue in memory only: no disk, network or plugin calls.
pub trait Observer: Send + Sync {
    fn update(&self, frames: u64, rate: u32, state: &'static str, ended: Option<&'static str>);
}

struct Segment {
    observer: Option<Arc<dyn Observer>>,
    pending: u64,
    played: u64,
    ended: Option<&'static str>,
}

#[derive(Default)]
struct Meter {
    segments: VecDeque<Segment>,
}
impl Meter {
    fn sample(&mut self, held: u64, rate: u32, state: &'static str) {
        let pending: u64 = self.segments.iter().map(|s| s.pending).sum();
        let mut consumed = pending.saturating_sub(held);
        for segment in &mut self.segments {
            let taken = consumed.min(segment.pending);
            segment.pending -= taken;
            segment.played += taken;
            consumed -= taken;
            if segment.played > 0 {
                if let Some(observer) = &segment.observer {
                    observer.update(segment.played, rate, state,
                        segment.ended.filter(|_| segment.pending == 0));
                }
            }
            // Do not announce a later occurrence ahead of an unconsumed tail.
            if segment.pending > 0 { break; }
        }
        while self.segments.front().is_some_and(|s| s.ended.is_some() && s.pending == 0) {
            self.segments.pop_front();
        }
    }

    fn discard(&mut self, rate: u32, reason: &'static str, close: bool) {
        for segment in &mut self.segments {
            if close || segment.ended.is_some() {
                if segment.ended.is_none() || (segment.pending > 0 && segment.ended == Some("finished")) {
                    segment.ended = Some(reason);
                }
            }
            segment.pending = 0;
        }
        self.sample(0, rate, if close { "playing" } else { "seeking" });
    }
}

/// I keep one FIFO across gapless tracks so the previous tail cannot be
/// counted as the next track. Discarded buffers never become listening time.
pub(crate) struct Measured<K: AudioSink> {
    inner: K,
    meter: RefCell<Meter>,
}
impl<K: AudioSink> Measured<K> {
    pub(crate) fn new(inner: K) -> Self { Self { inner, meter: RefCell::new(Meter::default()) } }
    fn sample(&self, state: &'static str) {
        if self.meter.borrow().segments.is_empty() { return; }
        self.meter.borrow_mut().sample(self.inner.held(), self.inner.format().sample_rate_hz, state);
    }
    pub(crate) fn begin(&mut self, observer: Option<Arc<dyn Observer>>) {
        self.sample("playing");
        if self.meter.get_mut().segments.is_empty() {
            if observer.is_none() { return; }
            // An unobserved local track may still have a gapless tail.
            let held = self.inner.held();
            if held > 0 {
                self.meter.get_mut().segments.push_back(Segment { observer: None, pending: held, played: 0, ended: Some("finished") });
            }
        }
        self.meter.get_mut().segments.push_back(Segment { observer, pending: 0, played: 0, ended: None });
    }
    pub(crate) fn end(&mut self, reason: &'static str) {
        self.sample("playing");
        if let Some(segment) = self.meter.get_mut().segments.back_mut() {
            segment.ended = Some(reason);
        }
        self.sample("playing");
    }
    fn discard(&mut self, reason: &'static str, close: bool) {
        self.meter.get_mut().discard(self.inner.format().sample_rate_hz, reason, close);
    }
}
impl<K: AudioSink> AudioSink for Measured<K> {
    fn format(&self) -> StreamFormat { self.inner.format() }
    fn held(&self) -> u64 {
        let held = self.inner.held();
        if !self.meter.borrow().segments.is_empty() {
            self.meter.borrow_mut().sample(held, self.inner.format().sample_rate_hz, "playing");
        }
        held
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        self.sample("playing");
        match self.inner.write(frames) {
            Ok(written) => {
                if let Some(segment) = self.meter.get_mut().segments.back_mut() {
                    segment.pending += written as u64;
                }
                self.sample("playing");
                Ok(written)
            }
            Err(error) => {
                // Recovery may reset a device buffer. Its old contents are
                // unconfirmed, even if held() becomes zero after the error.
                self.discard("error", !error.is_recoverable());
                Err(error)
            }
        }
    }
    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        self.sample("playing");
        let result = self.inner.drain(give_up);
        if result.is_ok() { self.sample("playing"); }
        else { self.discard("error", true); }
        result
    }
    fn flush(&mut self) -> Result<(), SinkError> {
        self.sample("seeking");
        self.discard("skipped", false);
        self.inner.flush()
    }
    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        let result = self.inner.set_paused(paused);
        self.sample(if paused { "paused" } else { "playing" });
        result
    }
    fn suspend(&mut self) -> Result<(), SinkError> {
        self.sample("paused");
        self.discard("stopped", false);
        self.inner.suspend()
    }
    fn wake(&mut self) -> Result<(), SinkError> { self.inner.wake() }
    fn close(&mut self) -> Result<(), SinkError> {
        self.sample("playing");
        self.discard("stopped", true);
        self.inner.close()
    }
    fn handoff(&mut self) -> Result<(), SinkError> {
        self.sample("playing");
        self.discard("replaced", true);
        self.inner.handoff()
    }
}
impl<K: AudioSink> Drop for Measured<K> {
    fn drop(&mut self) {
        self.sample("playing");
        self.discard("error", true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use chilpa_audio::format::SampleFormat;
    #[derive(Default)]
    struct Recorder(Mutex<Vec<(u64, String, Option<String>)>>);
    impl Observer for Recorder {
        fn update(&self, frames: u64, _: u32, state: &'static str, ended: Option<&'static str>) {
            self.0.lock().unwrap().push((frames, state.into(), ended.map(str::to_string)));
        }
    }
    struct Sink { held: u64, fail: bool }
    impl AudioSink for Sink {
        fn format(&self) -> StreamFormat { StreamFormat::new(SampleFormat::S16, 1000, 1) }
        fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
            if self.fail { return Err(SinkError::DeviceLost); }
            let frames = bytes.len() / 2; self.held += frames as u64; Ok(frames)
        }
        fn held(&self) -> u64 { self.held }
        fn close(&mut self) -> Result<(), SinkError> { self.held = 0; Ok(()) }
        fn flush(&mut self) -> Result<(), SinkError> { self.held = 0; Ok(()) }
        fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
            if !give_up() { self.held = 0; } Ok(())
        }
    }
    #[test]
    fn preload_and_buffer_fill_are_not_audible() {
        let log = Arc::new(Recorder::default());
        let mut sink = Measured::new(Sink { held: 0, fail: false });
        sink.begin(Some(log.clone()));
        sink.write(&[0; 200]).unwrap();
        sink.end("stopped");
        sink.close().unwrap();
        assert!(log.0.lock().unwrap().is_empty());
    }
    #[test]
    fn gapless_tail_belongs_to_the_old_occurrence() {
        let first = Arc::new(Recorder::default());
        let second = Arc::new(Recorder::default());
        let mut sink = Measured::new(Sink { held: 0, fail: false });
        sink.begin(Some(first.clone())); sink.write(&[0; 200]).unwrap(); sink.end("finished");
        sink.begin(Some(second.clone())); sink.write(&[0; 100]).unwrap();
        sink.inner.held = 60; sink.held();
        assert_eq!(first.0.lock().unwrap().last().unwrap().0, 90);
        assert!(second.0.lock().unwrap().is_empty());
        sink.inner.held = 40; sink.held();
        assert_eq!(first.0.lock().unwrap().last().unwrap(), &(100, "playing".into(), Some("finished".into())));
        assert_eq!(second.0.lock().unwrap().last().unwrap().0, 10);
        sink.end("finished"); sink.drain(&|| false).unwrap();
        assert_eq!(second.0.lock().unwrap().last().unwrap().0, 50);
    }
    #[test]
    fn seek_pause_and_stop_exclude_discarded_frames() {
        let log = Arc::new(Recorder::default());
        let mut sink = Measured::new(Sink { held: 0, fail: false });
        sink.begin(Some(log.clone())); sink.write(&[0; 200]).unwrap();
        sink.inner.held = 40; sink.flush().unwrap();
        sink.write(&[0; 100]).unwrap(); sink.inner.held = 20;
        sink.set_paused(true).unwrap(); sink.end("stopped"); sink.close().unwrap();
        assert_eq!(log.0.lock().unwrap().last().unwrap().0, 90);
        assert_eq!(log.0.lock().unwrap().last().unwrap().2.as_deref(), Some("stopped"));
    }
    #[test]
    fn output_failure_reports_only_confirmed_audio_and_no_unheard_start() {
        for consumed in [0, 60] {
            let log = Arc::new(Recorder::default());
            let mut sink = Measured::new(Sink { held: 0, fail: false });
            sink.begin(Some(log.clone())); sink.write(&[0; 200]).unwrap();
            sink.inner.held = 100 - consumed;
            sink.inner.fail = true;
            assert!(sink.write(&[0; 200]).is_err());
            drop(sink);
            let events = log.0.lock().unwrap();
            if consumed == 0 { assert!(events.is_empty()); }
            else { assert_eq!(events.last().unwrap(), &(60, "playing".into(), Some("error".into()))); }
        }
    }
    #[test]
    fn an_interrupted_drain_does_not_count_the_remaining_tail() {
        let log = Arc::new(Recorder::default());
        let mut sink = Measured::new(Sink { held: 0, fail: false });
        sink.begin(Some(log.clone())); sink.write(&[0; 200]).unwrap();
        sink.inner.held = 70; sink.end("finished");
        sink.drain(&|| true).unwrap(); sink.handoff().unwrap();
        assert_eq!(log.0.lock().unwrap().last().unwrap(), &(30, "playing".into(), Some("replaced".into())));
    }

    #[test]
    fn a_local_tail_cannot_start_the_following_remote_resource() {
        let log = Arc::new(Recorder::default());
        let mut sink = Measured::new(Sink { held: 0, fail: false });
        sink.begin(None); sink.write(&[0; 200]).unwrap(); sink.end("finished");
        sink.begin(Some(log.clone())); sink.write(&[0; 100]).unwrap();
        sink.inner.held = 60; sink.held();
        assert!(log.0.lock().unwrap().is_empty());
        sink.inner.held = 40; sink.held();
        assert_eq!(log.0.lock().unwrap().last().unwrap().0, 10);
        sink.end("finished"); sink.drain(&|| false).unwrap();
        assert_eq!(log.0.lock().unwrap().last().unwrap().0, 50);
    }

}
