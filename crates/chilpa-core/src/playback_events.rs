//! Listening facts measured at the output, independently of the UI cursor.
use chilpa_audio::{format::StreamFormat, sink::{AudioSink, SinkError}};
use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::Arc};

/// Implementations enqueue in memory only: no disk, network or plugin calls.
pub trait Observer: Send + Sync {
    fn update(&self, frames: u64, rate: u32, state: &'static str, ended: Option<&'static str>);
    fn update_at(&self, frames: u64, rate: u32, state: &'static str, ended: Option<&'static str>, _position: Option<u64>) {
        self.update(frames, rate, state, ended);
    }
}

struct Segment {
    /// The attempt at a track this audio is of: its turn in the play.
    attempt: u64,
    observer: Option<Arc<dyn Observer>>,
    pending: u64,
    played: u64,
    position: Option<u64>,
    ended: Option<&'static str>,
    /// What became of audio it still had in the device when it was let go.
    lost: Lost,
}

/// What became of the audio a track still had in the device when it was let
/// go of: nothing was left, the rest was thrown away, or the device failed
/// with it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub(crate) enum Lost {
    #[default]
    Nothing,
    Cut,
    Error,
}

/// A track whose audio is all accounted for: how much of it left the device,
/// and what became of the rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Retired {
    pub attempt: u64,
    pub played: u64,
    pub lost: Lost,
}

/// Where the meters of a play say which tracks are accounted for, and how
/// much of the one in front has been heard. One for the whole play: the
/// device is opened again between tracks, and each opening has its meter.
#[derive(Clone, Default)]
pub(crate) struct Report(Rc<RefCell<Reported>>);

#[derive(Default)]
struct Reported {
    retired: VecDeque<Retired>,
    /// The newest track and what of it has left the device.
    current: Option<(u64, u64)>,
}

impl Report {
    /// The tracks accounted for since last asked, oldest first.
    pub(crate) fn take(&self) -> Vec<Retired> {
        self.0.borrow_mut().retired.drain(..).collect()
    }

    /// What of `attempt` has left the device, while it is the newest.
    pub(crate) fn played(&self, attempt: u64) -> Option<u64> {
        self.0.borrow().current.filter(|(of, _)| *of == attempt).map(|(_, played)| played)
    }

    fn len(&self) -> usize {
        self.0.borrow().retired.len()
    }

    /// The device failed doing what let those tracks go: whatever they had
    /// left in it went with the failure.
    fn failed_from(&self, mark: usize) {
        for retired in self.0.borrow_mut().retired.iter_mut().skip(mark) {
            if retired.lost == Lost::Cut {
                retired.lost = Lost::Error;
            }
        }
    }
}

#[derive(Default)]
struct Meter {
    segments: VecDeque<Segment>,
    report: Option<Report>,
    /// A drain was given up: what is still pending is not counted as played
    /// by whatever the device says next, until it is thrown away.
    frozen: bool,
}
impl Meter {
    fn sample(&mut self, held: u64, rate: u32, state: &'static str) {
        if self.frozen { return; }
        let pending: u64 = self.segments.iter().map(|s| s.pending).sum();
        let mut consumed = pending.saturating_sub(held);
        for segment in &mut self.segments {
            let taken = consumed.min(segment.pending);
            segment.pending -= taken;
            segment.played += taken;
            segment.position = segment.position.map(|p| p.saturating_add(taken));
            consumed -= taken;
            if segment.played > 0 {
                if let Some(observer) = &segment.observer {
                    observer.update_at(segment.played, rate, state,
                        segment.ended.filter(|_| segment.pending == 0), segment.position);
                }
            }
            // Do not announce a later occurrence ahead of an unconsumed tail.
            if segment.pending > 0 { break; }
        }
        while self.segments.front().is_some_and(|s| s.ended.is_some() && s.pending == 0) {
            if let (Some(segment), Some(report)) = (self.segments.pop_front(), &self.report) {
                report.0.borrow_mut().retired.push_back(Retired { attempt: segment.attempt, played: segment.played, lost: segment.lost });
            }
        }
        if let Some(report) = &self.report {
            report.0.borrow_mut().current = self.segments.back().map(|s| (s.attempt, s.played));
        }
    }

    fn discard(&mut self, rate: u32, reason: &'static str, close: bool, lost: Lost) {
        self.frozen = false;
        for segment in &mut self.segments {
            if close || segment.ended.is_some() {
                if segment.pending > 0 {
                    segment.lost = segment.lost.max(lost);
                }
                if segment.ended.is_none() || (segment.pending > 0 && segment.ended == Some("finished")) {
                    segment.ended = Some(reason);
                }
            }
            segment.pending = 0;
        }
        self.sample(0, rate, if close { "playing" } else { "seeking" });
    }

    /// Whether anybody is told as the audio goes: then every write is looked
    /// at. Otherwise the look the renderer already takes after each write is
    /// enough, and no other is added.
    fn observed(&self) -> bool {
        self.segments.iter().any(|segment| segment.observer.is_some())
    }
}

/// I keep one FIFO across gapless tracks so the previous tail cannot be
/// counted as the next track. Discarded buffers never become listening time.
pub(crate) struct Measured<K: AudioSink> {
    inner: K,
    // A drop during unwinding must not call the failed device again.
    rate: u32,
    meter: RefCell<Meter>,
}
impl<K: AudioSink> Measured<K> {
    pub(crate) fn new(inner: K) -> Self { Self { rate: inner.format().sample_rate_hz, inner, meter: RefCell::new(Meter::default()) } }
    /// The same, saying in `report` which tracks are accounted for.
    pub(crate) fn reporting(inner: K, report: Report) -> Self {
        Self { rate: inner.format().sample_rate_hz, inner, meter: RefCell::new(Meter { report: Some(report), ..Meter::default() }) }
    }
    fn sample(&self, state: &'static str) {
        if self.meter.borrow().segments.is_empty() { return; }
        self.meter.borrow_mut().sample(self.inner.held(), self.rate, state);
    }
    pub(crate) fn begin(&mut self, observer: Option<Arc<dyn Observer>>) {
        self.sample("playing");
        if self.meter.get_mut().segments.is_empty() {
            if observer.is_none() { return; }
            // An unobserved local track may still have a gapless tail.
            let held = self.inner.held();
            if held > 0 {
                self.meter.get_mut().segments.push_back(Segment { attempt: 0, observer: None, pending: held, played: 0, position: None, ended: Some("finished"), lost: Lost::Nothing });
            }
        }
        self.meter.get_mut().segments.push_back(Segment { attempt: 0, observer, pending: 0, played: 0, position: Some(0), ended: None, lost: Lost::Nothing });
    }
    /// A track starts, observed or not: its audio is counted from here, as
    /// `attempt`'s.
    pub(crate) fn begin_attempt(&mut self, attempt: u64, observer: Option<Arc<dyn Observer>>) {
        self.sample("playing");
        self.meter.get_mut().segments.push_back(Segment { attempt, observer, pending: 0, played: 0, position: Some(0), ended: None, lost: Lost::Nothing });
    }
    pub(crate) fn end(&mut self, reason: &'static str) {
        self.sample("playing");
        if let Some(segment) = self.meter.get_mut().segments.back_mut() {
            segment.ended = Some(reason);
        }
        self.sample("playing");
    }
    fn discard(&mut self, reason: &'static str, close: bool, lost: Lost) {
        self.meter.get_mut().discard(self.rate, reason, close, lost);
    }
    /// What `fallible` let go of went with the device's failure, if it failed.
    fn letting_go(&mut self, reason: &'static str, close: bool, fallible: impl FnOnce(&mut K) -> Result<(), SinkError>) -> Result<(), SinkError> {
        struct Release(Option<(Report, usize)>);
        impl Drop for Release {
            fn drop(&mut self) {
                if let Some((report, mark)) = &self.0 { report.failed_from(*mark); }
            }
        }
        let mut release = Release(self.meter.get_mut().report.as_ref().map(|report| (report.clone(), report.len())));
        self.discard(reason, close, Lost::Cut);
        let result = fallible(&mut self.inner);
        if result.is_ok() { release.0 = None; }
        result
    }
}
impl<K: AudioSink> Measured<K> {
    fn write_measured(&mut self, frames: &[u8], source: Option<Option<chilpa_audio::sink::SourcePcm<'_>>>) -> Result<usize, SinkError> {
        let observed = self.meter.get_mut().observed();
        if observed { self.sample("playing"); }
        let result = match source {
            None => self.inner.write(frames),
            Some(source) => self.inner.write_with_source(frames, source),
        };
        match result {
            Ok(written) => {
                if let Some(segment) = self.meter.get_mut().segments.back_mut() {
                    segment.pending += written as u64;
                }
                if observed { self.sample("playing"); }
                Ok(written)
            }
            Err(error) => {
                // Recovery may reset a device buffer. Its old contents are
                // unconfirmed, even if held() becomes zero after the error.
                let lost = if error.is_recoverable() { Lost::Cut } else { Lost::Error };
                self.discard("error", !error.is_recoverable(), lost);
                Err(error)
            }
        }
    }
}
impl<K: AudioSink> AudioSink for Measured<K> {
    fn set_dop(&mut self, dop: bool) { self.inner.set_dop(dop); }
    fn position_changed(&mut self, position: Option<u64>) {
        if let Some(segment) = self.meter.get_mut().segments.back_mut() { segment.position = position; }
        self.inner.position_changed(position);
    }
    fn format(&self) -> StreamFormat { self.inner.format() }
    fn held(&self) -> u64 {
        let held = self.inner.held();
        if !self.meter.borrow().segments.is_empty() {
            self.meter.borrow_mut().sample(held, self.rate, "playing");
        }
        held
    }
    fn wants_source_pcm(&self) -> bool { self.inner.wants_source_pcm() }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        self.write_measured(frames, None)
    }
    fn write_with_source(&mut self, frames: &[u8], source: Option<chilpa_audio::sink::SourcePcm<'_>>) -> Result<usize, SinkError> {
        self.write_measured(frames, Some(source))
    }
    /// The device is watched while it drains, where it can say what it
    /// holds. A drain that is given up counts what played until then, by
    /// what the device said or, where it says nothing, by the time that
    /// went by; what was left is not counted by anything the device says
    /// afterwards: some throw it away as they give up and read as empty.
    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        self.sample("playing");
        let rate = self.rate;
        let (gave_up, told) = (std::cell::Cell::new(false), std::cell::Cell::new(false));
        let started = std::time::Instant::now();
        let meter = &self.meter;
        let result = self.inner.drain_observed(
            &|| {
                let yes = give_up();
                if yes { gave_up.set(true); }
                yes
            },
            &|held| {
                told.set(true);
                if !meter.borrow().segments.is_empty() {
                    meter.borrow_mut().sample(held, rate, "playing");
                }
            },
        );
        match result {
            Ok(()) if gave_up.get() => {
                let meter = self.meter.get_mut();
                if !told.get() {
                    let pending: u64 = meter.segments.iter().map(|s| s.pending).sum();
                    let played = (started.elapsed().as_secs_f64() * f64::from(rate)) as u64;
                    meter.sample(pending.saturating_sub(played), rate, "playing");
                }
                meter.frozen = true;
            }
            Ok(()) => self.sample("playing"),
            Err(_) => self.discard("error", true, Lost::Error),
        }
        result
    }
    fn flush(&mut self) -> Result<(), SinkError> {
        self.sample("seeking");
        self.letting_go("skipped", false, |inner| inner.flush())
    }
    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        let result = self.inner.set_paused(paused);
        self.sample(if paused { "paused" } else { "playing" });
        result
    }
    fn suspend(&mut self) -> Result<(), SinkError> {
        self.sample("paused");
        self.letting_go("stopped", false, |inner| inner.suspend())
    }
    fn wake(&mut self) -> Result<(), SinkError> { self.inner.wake() }
    fn close(&mut self) -> Result<(), SinkError> {
        self.sample("playing");
        self.letting_go("stopped", true, |inner| inner.close())
    }
    fn handoff(&mut self) -> Result<(), SinkError> {
        self.sample("playing");
        self.letting_go("replaced", true, |inner| inner.handoff())
    }
}
impl<K: AudioSink> Drop for Measured<K> {
    fn drop(&mut self) {
        // A failed operation may already have thrown away its buffers.
        // During unwind only the last confirmed sample is evidence.
        if !std::thread::panicking() { self.sample("playing"); }
        self.discard("error", true, Lost::Error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use chilpa_audio::format::SampleFormat;
    #[test]
    fn measured_writes_forward_source_pcm_without_changing_accounting() {
        struct SourceSink { source: Vec<u8>, output: Vec<u8> }
        impl AudioSink for SourceSink {
            fn format(&self) -> StreamFormat { StreamFormat::new(SampleFormat::S16, 8_000, 1) }
            fn wants_source_pcm(&self) -> bool { true }
            fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
                self.output.extend_from_slice(bytes);
                Ok(bytes.len() / 2)
            }
            fn write_with_source(&mut self, bytes: &[u8], source: Option<chilpa_audio::sink::SourcePcm<'_>>) -> Result<usize, SinkError> {
                self.source.extend_from_slice(source.unwrap().bytes);
                self.write(bytes)
            }
            fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
        }
        let mut sink = Measured::new(SourceSink { source: Vec::new(), output: Vec::new() });
        struct SilentObserver;
        impl Observer for SilentObserver {
            fn update(&self, _: u64, _: u32, _: &'static str, _: Option<&'static str>) {}
        }
        sink.begin(Some(Arc::new(SilentObserver)));
        assert!(sink.wants_source_pcm());
        let source = [0.25f32.to_le_bytes(), 0.5f32.to_le_bytes()].concat();
        assert_eq!(sink.write_with_source(&[0; 4], Some(chilpa_audio::sink::SourcePcm {
            bytes: &source, format: StreamFormat::new(SampleFormat::F32, 8_000, 1),
        })).unwrap(), 2);
        assert_eq!(sink.inner.source, source);
        assert_eq!(sink.inner.output, [0; 4]);
        let meter = sink.meter.borrow();
        assert_eq!(meter.segments.back().unwrap().played, 2);
        assert_eq!(meter.segments.back().unwrap().pending, 0);
    }

    #[test]
    fn seeking_moves_the_media_cursor_without_creating_listening_time() {
        #[derive(Default)]
        struct Positions(Mutex<Vec<(u64, Option<u64>)>>);
        impl Observer for Positions {
            fn update(&self, _: u64, _: u32, _: &'static str, _: Option<&'static str>) {}
            fn update_at(&self, frames: u64, _: u32, _: &'static str, _: Option<&'static str>, position: Option<u64>) {
                self.0.lock().unwrap().push((frames, position));
            }
        }
        let observer = Arc::new(Positions::default());
        let mut sink = Measured::new(Sink { held: 0, fail: false });
        sink.begin(Some(observer.clone()));
        sink.write(&[0; 400]).unwrap();
        sink.inner.held = 100;
        sink.sample("playing");
        sink.flush().unwrap();
        sink.inner.held = 0;
        sink.position_changed(Some(80_000));
        sink.write(&[0; 200]).unwrap();
        sink.inner.held = 0;
        sink.sample("playing");
        assert_eq!(observer.0.lock().unwrap().last(), Some(&(200, Some(80_100))));
    }
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

    /// A device at 1,000 frames a second that plays what it holds as time
    /// goes by while it drains. `tells`: it says what it holds at each look.
    /// `flushes`: giving up throws the rest away, and it then reads as empty.
    struct Timed { held: u64, tells: bool, flushes: bool, drain_fails: bool, close_fails: bool, asked: std::cell::Cell<u32> }
    impl Timed {
        fn new(tells: bool, flushes: bool) -> Self {
            Self { held: 0, tells, flushes, drain_fails: false, close_fails: false, asked: Default::default() }
        }
        fn wait(&mut self, give_up: &dyn Fn() -> bool, tell: Option<&dyn Fn(u64)>) -> Result<(), SinkError> {
            if self.drain_fails { return Err(SinkError::DeviceLost); }
            let (started, before) = (std::time::Instant::now(), self.held);
            loop {
                self.held = before.saturating_sub(started.elapsed().as_millis() as u64);
                if let Some(tell) = tell { tell(self.held); }
                if give_up() {
                    if self.flushes { self.held = 0; }
                    return Ok(());
                }
                if self.held == 0 { return Ok(()); }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
    impl AudioSink for Timed {
        fn format(&self) -> StreamFormat { StreamFormat::new(SampleFormat::S16, 1000, 1) }
        fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> { let frames = bytes.len() / 2; self.held += frames as u64; Ok(frames) }
        fn held(&self) -> u64 { self.asked.set(self.asked.get() + 1); self.held }
        fn close(&mut self) -> Result<(), SinkError> { self.held = 0; if self.close_fails { Err(SinkError::DeviceLost) } else { Ok(()) } }
        fn flush(&mut self) -> Result<(), SinkError> { self.held = 0; Ok(()) }
        fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> { self.wait(give_up, None) }
        fn drain_observed(&mut self, give_up: &dyn Fn() -> bool, held: &dyn Fn(u64)) -> Result<(), SinkError> {
            if self.tells { self.wait(give_up, Some(held)) } else { self.wait(give_up, None) }
        }
    }

    /// A thousand frames held, the drain given up once six hundred have
    /// played, then the device closed and let go: what the meter says of it.
    fn given_up_at_six_hundred(tells: bool, flushes: bool) -> Retired {
        let report = Report::default();
        let mut sink = Measured::reporting(Timed::new(tells, flushes), report.clone());
        sink.begin_attempt(1, None);
        sink.write(&[0; 2000]).unwrap();
        sink.end("finished");
        let started = std::time::Instant::now();
        sink.drain(&|| started.elapsed().as_millis() >= 600).unwrap();
        sink.close().unwrap();
        drop(sink);
        let retired = report.take();
        assert_eq!(retired.len(), 1, "{retired:?}");
        retired[0]
    }

    #[test]
    fn a_drain_given_up_counts_what_played_until_then_whatever_the_device_does_after() {
        // The device says what it holds and throws the rest away: by its word.
        let told = given_up_at_six_hundred(true, true);
        assert!((600..=700).contains(&told.played), "{told:?}");
        // It says nothing and throws the rest away, so it reads as empty
        // afterwards: by the time that went by, not by what it reads.
        let silent = given_up_at_six_hundred(false, true);
        assert!((600..=700).contains(&silent.played), "{silent:?}");
        // It says nothing and keeps the rest until it is closed.
        let keeps = given_up_at_six_hundred(false, false);
        assert!((600..=700).contains(&keeps.played), "{keeps:?}");
        for retired in [told, silent, keeps] {
            assert_eq!(retired.lost, Lost::Cut, "the rest was thrown away, and is said so: {retired:?}");
        }
    }

    #[test]
    fn a_tail_that_played_out_before_the_device_failed_is_heard_whole() {
        let report = Report::default();
        let mut sink = Measured::reporting(Sink { held: 0, fail: false }, report.clone());
        sink.begin_attempt(1, None); sink.write(&[0; 400]).unwrap(); sink.end("finished");
        sink.begin_attempt(2, None);
        // The first track's last audio plays while nothing is written.
        sink.inner.held = 0; sink.held();
        sink.inner.fail = true;
        assert!(sink.write(&[0; 200]).is_err());
        drop(sink);
        assert_eq!(report.take(), [
            Retired { attempt: 1, played: 200, lost: Lost::Nothing },
            Retired { attempt: 2, played: 0, lost: Lost::Nothing },
        ]);
    }

    #[test]
    fn a_seek_in_the_next_track_takes_from_the_one_before_only_the_tail_it_threw_away() {
        let report = Report::default();
        let mut sink = Measured::reporting(Sink { held: 0, fail: false }, report.clone());
        sink.begin_attempt(1, None); sink.write(&[0; 400]).unwrap(); sink.end("finished");
        sink.begin_attempt(2, None); sink.write(&[0; 200]).unwrap();
        // Fifty frames of the first have played; a seek throws the rest away.
        sink.inner.held = 250; sink.held();
        sink.flush().unwrap();
        sink.write(&[0; 200]).unwrap();
        sink.inner.held = 0; sink.held();
        sink.end("finished"); sink.close().unwrap();
        assert_eq!(report.take(), [
            Retired { attempt: 1, played: 50, lost: Lost::Cut },
            Retired { attempt: 2, played: 100, lost: Lost::Nothing },
        ]);
    }

    #[test]
    fn short_tracks_sharing_what_the_device_holds_are_accounted_for_in_order() {
        let report = Report::default();
        let mut sink = Measured::reporting(Sink { held: 0, fail: false }, report.clone());
        for attempt in 1..=3 {
            sink.begin_attempt(attempt, None); sink.write(&[0; 20]).unwrap();
            if attempt < 3 { sink.end("finished"); }
        }
        assert!(report.take().is_empty(), "all of it is still in the device");
        sink.inner.held = 0; sink.held();
        assert_eq!(report.take(), [
            Retired { attempt: 1, played: 10, lost: Lost::Nothing },
            Retired { attempt: 2, played: 10, lost: Lost::Nothing },
        ]);
        assert_eq!(report.played(3), Some(10), "and the one in front is heard so far");
    }

    #[test]
    fn a_track_nobody_observes_adds_no_look_at_the_device_per_write() {
        let mut sink = Measured::reporting(Timed::new(false, false), Report::default());
        sink.begin_attempt(1, None);
        for _ in 0..5 { sink.write(&[0; 20]).unwrap(); }
        assert_eq!(sink.inner.asked.get(), 0, "the renderer's own look after each write is the only one");
        struct Silent;
        impl Observer for Silent { fn update(&self, _: u64, _: u32, _: &'static str, _: Option<&'static str>) {} }
        let mut observed = Measured::reporting(Timed::new(false, false), Report::default());
        observed.begin_attempt(1, Some(Arc::new(Silent)));
        observed.write(&[0; 20]).unwrap();
        assert_eq!(observed.inner.asked.get(), 2, "an observed track is looked at around every write, as before");
    }

    #[test]
    fn a_device_that_fails_letting_go_takes_what_it_held_with_the_failure() {
        for fails_closing in [false, true] {
            let report = Report::default();
            let mut device = Timed::new(false, false);
            (device.drain_fails, device.close_fails) = (!fails_closing, fails_closing);
            let mut sink = Measured::reporting(device, report.clone());
            sink.begin_attempt(1, None); sink.write(&[0; 200]).unwrap(); sink.end("finished");
            if fails_closing { assert!(sink.close().is_err()); } else { assert!(sink.drain(&|| false).is_err()); }
            drop(sink);
            assert_eq!(report.take(), [Retired { attempt: 1, played: 0, lost: Lost::Error }], "closing: {fails_closing}");
        }
    }

    /// A track written to `device` and ended as `how`, the play then driven
    /// by `then`; what it was closed as.
    fn closed_after(device: Timed, how: crate::attempts::How, then: impl FnOnce(&mut Measured<Timed>, &chilpa_audio::sink::StopSignal)) -> crate::attempts::Closed {
        let mut said = Vec::new();
        let mut tell = |closed: crate::attempts::Closed| said.push(closed);
        let stop = chilpa_audio::sink::StopSignal::new();
        {
            let mut attempts = crate::attempts::Attempts::new(&mut tell, stop.clone());
            let mut sink = Measured::reporting(device, attempts.report());
            sink.begin_attempt(1, None);
            attempts.open(1, 0);
            sink.write(&[0; 2000]).unwrap();
            attempts.ended(1, how);
            sink.end("finished");
            then(&mut sink, &stop);
            drop(sink);
            attempts.settle();
        }
        assert_eq!(said.len(), 1, "closed once: {said:?}");
        said[0]
    }

    #[test]
    fn what_the_device_does_with_a_track_s_last_audio_raises_how_it_ended_and_never_lowers_it() {
        use crate::attempts::How;
        // It played to its end and the drain failed: the output failed with it.
        let mut failing = Timed::new(false, false);
        failing.drain_fails = true;
        let closed = closed_after(failing, How::Completed, |sink, _| assert!(sink.drain(&|| false).is_err()));
        assert_eq!((closed.how, closed.heard), (How::Failed, 0));
        // It played to its end and the play was stopped during its last drain.
        let closed = closed_after(Timed::new(true, true), How::Completed, |sink, stop| {
            stop.stop();
            sink.drain(&|| true).unwrap();
            sink.close().unwrap();
        });
        assert_eq!(closed.how, How::Stopped);
        // It broke, and the play was stopped during its last drain: it broke.
        let closed = closed_after(Timed::new(true, true), How::Broken, |sink, stop| {
            stop.stop();
            sink.drain(&|| true).unwrap();
            sink.close().unwrap();
        });
        assert_eq!(closed.how, How::Broken);
        // Its last audio was cut by a Next, not by a stop: it played.
        let closed = closed_after(Timed::new(true, true), How::Completed, |sink, _| {
            sink.drain(&|| true).unwrap();
            sink.close().unwrap();
        });
        assert_eq!(closed.how, How::Completed);
        // It played out whole.
        let closed = closed_after(Timed::new(true, true), How::Completed, |sink, _| {
            sink.drain(&|| false).unwrap();
            sink.close().unwrap();
        });
        assert_eq!((closed.how, closed.heard), (How::Completed, 1000));
    }
}
