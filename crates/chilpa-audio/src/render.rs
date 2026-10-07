//! The loop that carries samples to the device and says how it ended.
//!
//! The point of this module is that **the end of a track has a type**. In
//! QBZ a failed write went down the same path as a natural end, so an
//! unplugged DAC looked like "it is over" and the whole queue skipped
//! through in silence. Here only [`Outcome::Finished`] lets the queue
//! advance; everything else is told to the user.
//!
//! The source has its own way to fail. A file that could not be read on
//! used to end its track as if it were over, and the queue went on to the
//! next with the rest of that one missing and nobody told.

use crate::sink::{AudioSink, PauseSignal, SinkError, StopSignal};

/// Where the bytes going to the device come from.
pub trait FrameSource {
    /// The next block, already in the device's format. `Ok(None)` = the
    /// track ended naturally; `Err` = it could not be read on, and why, for
    /// the user.
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String>;

    /// True, once, when the block just handed over comes from somewhere else
    /// in the track: after a seek, what the device still holds is from
    /// before, and it is thrown away rather than played first.
    fn moved(&mut self) -> bool {
        false
    }

    /// Goes back to what was heard -- the frames reported to `on_progress`
    /// last -- so the next block starts there. It is asked for when a long
    /// pause lets the device go and what it held is lost: without it, the
    /// music resumes a moment further on than where it stopped. By default
    /// it cannot, and that moment is skipped.
    fn rewind(&mut self) -> bool {
        false
    }
    /// True when the end just reported was asked for -- a skip -- and not
    /// reached: what was read ahead of it is not music to play. By default
    /// an end is the end.
    fn ended_early(&self) -> bool {
        false
    }
    /// What had been reported heard when the source last moved, as the
    /// source counted it when it worked out where it was: what went out
    /// after that, from the old place, is not where the music is. `None`
    /// from a source that does not count.
    fn heard_at_move(&self) -> Option<u64> {
        None
    }
}

/// How far the loop reads ahead of the device: two seconds of what the
/// device takes.
///
/// The device holds a third of a second at most, and what it holds is all
/// the time there is for the next block to arrive. A file on a network
/// share answers late now and then, and a thread the system holds back
/// behind a build reads nothing for as long as it is held; either one ran
/// the DAC dry at DSD256 on a direct ALSA route, where nothing else stands
/// between the file and the device. The source is read on a thread of its
/// own, two seconds ahead, and the loop takes its blocks from there.
pub const AHEAD: std::time::Duration = std::time::Duration::from_secs(2);

/// A read that takes this long is written down: it is what the cushion is
/// for, and the log should say when it was needed.
const SLOW_READ: std::time::Duration = std::time::Duration::from_millis(100);

/// What was read ahead and not yet handed to the device. A block from
/// somewhere else in the track carries how much had been heard when the
/// source moved, when the source counts that: what the loop wrote after
/// that, from the old place, is not where the music is.
struct Ahead {
    blocks: std::collections::VecDeque<(Vec<u8>, Option<Option<u64>>)>,
    bytes: usize,
    /// How the source ended, once it has: `Ok` when it was over, `Err` with
    /// the reason when it could not be read on.
    end: Option<Result<(), String>>,
    /// The loop asked the source to go back to what was heard, and is
    /// waiting to hear whether it could.
    rewind: Option<Option<bool>>,
    /// The loop is done with the source.
    done: bool,
}

struct Cushion {
    ahead: std::sync::Mutex<Ahead>,
    changed: std::sync::Condvar,
    /// Bytes to keep read ahead.
    fill: usize,
}

impl Cushion {
    fn new(fill: usize) -> Self {
        Self {
            ahead: std::sync::Mutex::new(Ahead { blocks: Default::default(), bytes: 0, end: None, rewind: None, done: false }),
            changed: std::sync::Condvar::new(),
            fill,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Ahead> {
        self.ahead.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Reads the source until the loop is done with it, keeping `fill`
    /// bytes ahead. A seek, a skip and a rewind each throw away what was
    /// read before them: it is from where the music no longer is. Once the
    /// source has ended it stays, asking it now and then, for a seek or a
    /// rewind: the source reaches its end two seconds before the device
    /// does, and a jump in those seconds goes back into them.
    fn read<S: FrameSource>(&self, source: &mut S, stop: &StopSignal) {
        loop {
            {
                let mut ahead = self.lock();
                loop {
                    if ahead.done || stop.is_stopped() {
                        return;
                    }
                    if let Some(None) = ahead.rewind {
                        drop(ahead);
                        let went_back = source.rewind();
                        ahead = self.lock();
                        if went_back {
                            ahead.blocks.clear();
                            ahead.bytes = 0;
                            ahead.end = None;
                            crate::diagnostics::trace("render", "ahead_dropped", format_args!("reason=rewind"));
                        }
                        ahead.rewind = Some(Some(went_back));
                        self.changed.notify_all();
                        continue;
                    }
                    if ahead.bytes < self.fill && ahead.end.is_none() {
                        break;
                    }
                    ahead = self.changed.wait_timeout(ahead, std::time::Duration::from_millis(20)).map(|(guard, _)| guard).unwrap_or_else(|poisoned| poisoned.into_inner().0);
                    // An ended source is asked again: a seek into its last
                    // seconds brings it back.
                    if ahead.end.is_some() && !ahead.done {
                        break;
                    }
                }
            }
            let reading = std::time::Instant::now();
            let read = source.next_block();
            if reading.elapsed() >= SLOW_READ {
                crate::diagnostics::trace("render", "slow_read", format_args!("took={:?}", reading.elapsed()));
            }
            let mut ahead = self.lock();
            match read {
                Ok(Some(block)) => {
                    let moved = source.moved().then(|| source.heard_at_move());
                    if moved.is_some() {
                        ahead.blocks.clear();
                        ahead.bytes = 0;
                        ahead.end = None;
                        crate::diagnostics::trace("render", "ahead_dropped", format_args!("reason=seek"));
                    }
                    ahead.bytes += block.len();
                    ahead.blocks.push_back((block, moved));
                }
                Ok(None) => {
                    if source.ended_early() {
                        ahead.blocks.clear();
                        ahead.bytes = 0;
                        crate::diagnostics::trace("render", "ahead_dropped", format_args!("reason=skip"));
                    }
                    if ahead.end.is_none() {
                        ahead.end = Some(Ok(()));
                    }
                }
                Err(reason) => ahead.end = Some(Err(reason)),
            }
            self.changed.notify_all();
        }
    }

    /// The next block read ahead and, when it comes from somewhere else in
    /// the track, how much had been heard when the source moved; `Ok(None)`
    /// once the source is over, `Err` when it could not be read on. Waits
    /// for one, minding the stop.
    fn take(&self, stop: &StopSignal) -> Result<Option<(Vec<u8>, Option<Option<u64>>)>, String> {
        let mut ahead = self.lock();
        let mut starved: Option<std::time::Instant> = None;
        loop {
            if let Some((block, moved)) = ahead.blocks.pop_front() {
                ahead.bytes -= block.len();
                self.changed.notify_all();
                // A wait worth writing down: a device that takes samples
                // faster than the file gives them waits a little at every
                // block, and that is not the cushion running dry.
                if let Some(since) = starved.filter(|since| since.elapsed() >= SLOW_READ) {
                    crate::diagnostics::trace("render", "ahead_empty", format_args!("waited={:?}", since.elapsed()));
                }
                return Ok(Some((block, moved)));
            }
            match &ahead.end {
                Some(Ok(())) => return Ok(None),
                Some(Err(reason)) => return Err(reason.clone()),
                None => {}
            }
            if stop.is_stopped() {
                return Ok(None);
            }
            starved.get_or_insert_with(std::time::Instant::now);
            ahead = self.changed.wait_timeout(ahead, std::time::Duration::from_millis(20)).map(|(guard, _)| guard).unwrap_or_else(|poisoned| poisoned.into_inner().0);
        }
    }

    /// Asks the source to go back to what was heard, and waits to hear
    /// whether it could, minding the stop.
    fn rewind(&self, stop: &StopSignal) -> bool {
        let mut ahead = self.lock();
        ahead.rewind = Some(None);
        self.changed.notify_all();
        loop {
            if let Some(Some(answer)) = ahead.rewind {
                ahead.rewind = None;
                return answer;
            }
            if stop.is_stopped() {
                ahead.rewind = None;
                return false;
            }
            ahead = self.changed.wait_timeout(ahead, std::time::Duration::from_millis(20)).map(|(guard, _)| guard).unwrap_or_else(|poisoned| poisoned.into_inner().0);
        }
    }

    fn finish(&self) {
        self.lock().done = true;
        self.changed.notify_all();
    }
}

/// How long a pause keeps the device before letting it go. A quick pause
/// and resume does not reopen it; a pause somebody walks away from does not
/// keep a DAC from everything else.
pub const RELEASE_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

/// How a track ended.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The source ran out and everything of its was delivered. It is the
    /// ONLY ending that allows moving to the next track.
    Finished { frames: u64 },
    /// Somebody asked to stop.
    Stopped { frames: u64 },
    /// The device failed. The queue does not advance and the user finds out.
    Failed { frames: u64, error: SinkError },
    /// The source could not be read on: a damaged file, a disc image with a
    /// bad sector. The same as a device failure for the queue.
    Unreadable { frames: u64, reason: String },
}

impl Outcome {
    pub fn may_advance_queue(&self) -> bool {
        matches!(self, Outcome::Finished { .. })
    }

    pub fn frames(&self) -> u64 {
        match self {
            Outcome::Finished { frames }
            | Outcome::Stopped { frames }
            | Outcome::Failed { frames, .. }
            | Outcome::Unreadable { frames, .. } => *frames,
        }
    }
}

/// How many xruns in a row are tolerated before giving the device up.
const MAX_CONSECUTIVE_XRUNS: u32 = 8;

/// How long a device that answers "full" is waited on between tries.
const FULL_WAIT: std::time::Duration = std::time::Duration::from_millis(5);

/// How long a device may take nothing before it is given up: twenty times
/// the quarter second JACK keeps queued.
const STALL_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Delivers everything the source gives and returns how it ended.
///
/// It checks the stop signal between blocks, so stopping does not depend on
/// the device answering. It retries partial writes, tolerates a few xruns
/// in a row, and when the device is lost or takes too long it does not fake
/// a natural end.
pub fn render<S, K>(source: &mut S, sink: &mut K, stop: &StopSignal) -> Outcome
where
    S: FrameSource + Send,
    K: AudioSink,
{
    render_with(source, sink, stop, &mut |_| {})
}

/// The same, reporting how many frames have gone out.
///
/// The report comes from the delivery loop, so the position the interface
/// sees is what really went over the wire and not a clock's count. It is
/// called once per accepted write, and never from the system's audio
/// thread.
pub fn render_with<S, K>(
    source: &mut S,
    sink: &mut K,
    stop: &StopSignal,
    on_progress: &mut dyn FnMut(u64),
) -> Outcome
where
    S: FrameSource + Send,
    K: AudioSink,
{
    render_paused(source, sink, stop, &PauseSignal::new(), on_progress)
}

/// The same, minding the pause as well.
///
/// While paused the loop stops writing and waits, and tells the device in
/// case it can really pause. It keeps watching the stop signal, so stopping
/// from a pause returns at once.
pub fn render_paused<S, K>(
    source: &mut S,
    sink: &mut K,
    stop: &StopSignal,
    pause: &PauseSignal,
    on_progress: &mut dyn FnMut(u64),
) -> Outcome
where
    S: FrameSource + Send,
    K: AudioSink,
{
    let format = sink.format();
    let frame_bytes = format.frame_bytes();
    let fill = frame_bytes * format.sample_rate_hz as usize * AHEAD.as_millis() as usize / 1000;
    let cushion = Cushion::new(fill.max(frame_bytes));
    let started = std::time::Instant::now();
    std::thread::scope(|threads| {
        let reader = std::thread::Builder::new().name("chilpa-read-ahead".into());
        let outcome = match reader.spawn_scoped(threads, || cushion.read(source, stop)) {
            Ok(_) => deliver(&cushion, sink, stop, pause, on_progress, frame_bytes, started),
            Err(_) => Outcome::Failed { frames: 0, error: SinkError::Backend(chilpa_i18n::t("the read-ahead thread could not start")) },
        };
        cushion.finish();
        outcome
    })
}

/// Carries what was read ahead to the device.
fn deliver<K: AudioSink>(
    cushion: &Cushion,
    sink: &mut K,
    stop: &StopSignal,
    pause: &PauseSignal,
    on_progress: &mut dyn FnMut(u64),
    frame_bytes: usize,
    started: std::time::Instant,
) -> Outcome {
    let format = sink.format();
    crate::diagnostics::trace("render", "started", format_args!("format={format:?}"));
    let mut frames: u64 = 0;
    let mut xruns = 0;
    // Since when the device has taken nothing, while it says it is full.
    let mut full_since: Option<std::time::Instant> = None;
    // Frames handed over and then thrown away at a seek: never heard.
    let mut thrown_away: u64 = 0;
    // Since when it is paused, and whether the device was let go. Across
    // blocks: letting it go goes back to what was heard, and the next block
    // is read while still paused.
    let mut paused_since: Option<std::time::Instant> = None;
    let mut released = false;

    while !stop.is_stopped() {
        let (block, moved) = match cushion.take(stop) {
            Ok(Some(block)) => block,
            Ok(None) if stop.is_stopped() => break,
            Ok(None) => {
                crate::diagnostics::trace(
                    "render",
                    "source_finished",
                    format_args!("after={:?} frames={frames}", started.elapsed()),
                );
                return Outcome::Finished { frames };
            }
            Err(reason) => {
                crate::diagnostics::trace(
                    "render",
                    "source_failed",
                    format_args!("after={:?} frames={frames} reason={reason:?}", started.elapsed()),
                );
                return Outcome::Unreadable { frames, reason };
            }
        };
        // From somewhere else in the track: what the device holds is from
        // before, and it goes; so does what went out from before since the
        // source moved, which was heard but is not where the music is.
        if let Some(heard_at_move) = moved {
            let held = sink.held();
            let stale = heard_at_move.map_or(0, |at| frames.saturating_sub(thrown_away + held).saturating_sub(at));
            crate::diagnostics::trace(
                "render",
                "position_moved",
                format_args!("frames={frames} held={held} stale={stale} thrown_away={thrown_away}"),
            );
            thrown_away += held + stale;
            if let Err(error) = sink.flush() {
                crate::diagnostics::trace("render", "flush_failed", format_args!("error={error}"));
                return Outcome::Failed { frames, error };
            }
        }
        let mut offset = 0;
        let mut warned = false;
        while offset < block.len() {
            if stop.is_stopped() {
                crate::diagnostics::trace(
                    "render",
                    "stopped",
                    format_args!("after={:?} frames={frames}", started.elapsed()),
                );
                return Outcome::Stopped { frames };
            }
            if pause.is_paused() {
                if !warned {
                    crate::diagnostics::trace("render", "pause_entered", format_args!("frames={frames}"));
                    let _ = sink.set_paused(true);
                    warned = true;
                }
                full_since = None;
                let since = *paused_since.get_or_insert_with(std::time::Instant::now);
                if !released && since.elapsed() >= RELEASE_AFTER {
                    // What was heard is where the music resumes: say it,
                    // go back to it, and let what the device held go with
                    // the device.
                    let held = sink.held();
                    crate::diagnostics::trace(
                        "render",
                        "suspend_started",
                        format_args!("frames={frames} held={held}"),
                    );
                    on_progress(frames.saturating_sub(thrown_away + held));
                    thrown_away += held;
                    if cushion.rewind(stop) {
                        offset = block.len();
                    }
                    let result = sink.suspend();
                    crate::diagnostics::trace(
                        "render",
                        "suspend_finished",
                        format_args!("result={result:?}"),
                    );
                    released = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            paused_since = None;
            if released {
                released = false;
                crate::diagnostics::trace("render", "wake_started", format_args!("frames={frames}"));
                if let Err(error) = sink.wake() {
                    crate::diagnostics::trace("render", "wake_failed", format_args!("error={error}"));
                    return Outcome::Failed { frames, error };
                }
                crate::diagnostics::trace("render", "wake_finished", format_args!("frames={frames}"));
            }
            if warned {
                crate::diagnostics::trace("render", "pause_left", format_args!("frames={frames}"));
                let _ = sink.set_paused(false);
                warned = false;
            }
            if offset >= block.len() {
                break;
            }
            match sink.write(&block[offset..]) {
                // Full: nothing was taken, and the rest of the block is
                // offered again. Taking this for "on to the next block"
                // dropped the rest of it, which on a device with a queue of
                // its own was most of every track.
                Ok(0) => {
                    let since = *full_since.get_or_insert_with(std::time::Instant::now);
                    if since.elapsed() > STALL_LIMIT {
                        crate::diagnostics::trace(
                            "render",
                            "sink_stalled",
                            format_args!("frames={frames} for={:?}", since.elapsed()),
                        );
                        return Outcome::Failed { frames, error: SinkError::Timeout(since.elapsed()) };
                    }
                    std::thread::sleep(FULL_WAIT);
                }
                Ok(written) => {
                    full_since = None;
                    xruns = 0;
                    offset += written * frame_bytes;
                    frames += written as u64;
                    // What was heard: not what the device still holds, nor
                    // what a seek threw away.
                    on_progress(frames.saturating_sub(thrown_away + sink.held()));
                }
                Err(e) if e.is_recoverable() => {
                    xruns += 1;
                    crate::diagnostics::trace(
                        "render",
                        "recoverable_error",
                        format_args!("frames={frames} consecutive={xruns} error={e}"),
                    );
                    if xruns >= MAX_CONSECUTIVE_XRUNS {
                        crate::diagnostics::trace(
                            "render",
                            "device_lost",
                            format_args!("frames={frames} reason=consecutive_xruns"),
                        );
                        return Outcome::Failed { frames, error: SinkError::DeviceLost };
                    }
                }
                Err(error) => {
                    crate::diagnostics::trace(
                        "render",
                        "write_failed",
                        format_args!("after={:?} frames={frames} error={error}", started.elapsed()),
                    );
                    return Outcome::Failed { frames, error };
                }
            }
        }
    }
    crate::diagnostics::trace(
        "render",
        "stopped",
        format_args!("after={:?} frames={frames}", started.elapsed()),
    );
    Outcome::Stopped { frames }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{SampleFormat, StreamFormat};
    use std::time::Duration;

    #[test]
    fn paused_nothing_is_written_and_resuming_picks_up_where_it_was() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let pause = PauseSignal::new();
        pause.set(true);
        let mut source = Blocks(vec![block(8)], None);
        let mut sink = FakeSink::new(100);
        let ready = Arc::new(AtomicBool::new(false));

        std::thread::scope(|s| {
            let pause_thread = pause.clone();
            let ready_thread = Arc::clone(&ready);
            s.spawn(move || {
                // While paused, nothing must go out.
                std::thread::sleep(Duration::from_millis(60));
                ready_thread.store(true, Ordering::Release);
                pause_thread.set(false);
            });
            let out = render_paused(&mut source, &mut sink, &StopSignal::new(), &pause, &mut |_| {});
            assert!(ready.load(Ordering::Acquire), "it did not write before the pause was lifted");
            assert_eq!(out, Outcome::Finished { frames: 8 });
        });
    }

    /// A source whose frames carry their own number, that can go back to a
    /// frame; and a device that holds the last `hold` frames written, lets
    /// go and takes back, and remembers what it was sent.
    struct Numbered {
        next: u16,
        end: u16,
        heard: std::sync::Arc<std::sync::atomic::AtomicU64>,
    }

    impl FrameSource for Numbered {
        fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
            if self.next >= self.end {
                return Ok(None);
            }
            let to = (self.next + 10).min(self.end);
            let block = (self.next..to).flat_map(|n| [n.to_le_bytes(), [0, 0]].concat()).collect();
            self.next = to;
            Ok(Some(block))
        }

        fn rewind(&mut self) -> bool {
            self.next = self.heard.load(std::sync::atomic::Ordering::Relaxed) as u16;
            true
        }
    }

    struct Letting {
        hold: u64,
        sent: Vec<u16>,
        suspended: bool,
        suspends: u32,
        wakes: u32,
        wake_fails: bool,
        pause_after_first: Option<PauseSignal>,
    }

    impl AudioSink for Letting {
        fn format(&self) -> StreamFormat {
            StreamFormat::new(SampleFormat::S16, 44_100, 2)
        }

        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            assert!(!self.suspended, "written to a device that was let go");
            self.sent.extend(frames.chunks(4).map(|f| u16::from_le_bytes([f[0], f[1]])));
            if let Some(pause) = self.pause_after_first.take() {
                pause.set(true);
            }
            Ok(frames.len() / 4)
        }

        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }

        fn held(&self) -> u64 {
            if self.suspended { 0 } else { self.hold.min(self.sent.len() as u64) }
        }

        fn suspend(&mut self) -> Result<(), SinkError> {
            self.suspended = true;
            self.suspends += 1;
            Ok(())
        }

        fn wake(&mut self) -> Result<(), SinkError> {
            self.wakes += 1;
            if self.wake_fails {
                return Err(SinkError::DeviceLost);
            }
            self.suspended = false;
            Ok(())
        }
    }

    #[test]
    fn a_long_pause_lets_the_device_go_and_resumes_from_what_was_heard() {
        let pause = PauseSignal::new();
        let heard = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut source = Numbered { next: 0, end: 30, heard: std::sync::Arc::clone(&heard) };
        let mut sink = Letting {
            hold: 4,
            sent: Vec::new(),
            suspended: false,
            suspends: 0,
            wakes: 0,
            wake_fails: false,
            pause_after_first: Some(pause.clone()),
        };
        let out = std::thread::scope(|s| {
            let resume = pause.clone();
            s.spawn(move || {
                std::thread::sleep(RELEASE_AFTER + Duration::from_millis(300));
                resume.set(false);
            });
            render_paused(&mut source, &mut sink, &StopSignal::new(), &pause, &mut |f| {
                heard.store(f, std::sync::atomic::Ordering::Relaxed)
            })
        });
        assert_eq!((sink.suspends, sink.wakes), (1, 1), "let go once, taken back once");
        // Ten went out and the device held four when it was let go: six were
        // heard, and the music goes on from frame 6, not from 10.
        assert_eq!(&sink.sent[..10], (0..10).collect::<Vec<_>>().as_slice());
        assert_eq!(&sink.sent[10..], (6..30).collect::<Vec<_>>().as_slice());
        assert_eq!(out, Outcome::Finished { frames: 34 });
    }

    #[test]
    fn a_device_that_cannot_be_had_back_fails_the_track() {
        let pause = PauseSignal::new();
        let heard = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut source = Numbered { next: 0, end: 30, heard: std::sync::Arc::clone(&heard) };
        let mut sink = Letting {
            hold: 4,
            sent: Vec::new(),
            suspended: false,
            suspends: 0,
            wakes: 0,
            wake_fails: true,
            pause_after_first: Some(pause.clone()),
        };
        let out = std::thread::scope(|s| {
            let resume = pause.clone();
            s.spawn(move || {
                std::thread::sleep(RELEASE_AFTER + Duration::from_millis(300));
                resume.set(false);
            });
            render_paused(&mut source, &mut sink, &StopSignal::new(), &pause, &mut |f| {
                heard.store(f, std::sync::atomic::Ordering::Relaxed)
            })
        });
        assert!(matches!(out, Outcome::Failed { error: SinkError::DeviceLost, .. }), "{out:?}");
    }

    #[test]
    fn stopping_from_a_pause_returns_at_once() {
        let pause = PauseSignal::new();
        pause.set(true);
        let stop = StopSignal::new();
        stop.stop();
        let mut source = Blocks(vec![block(8)], None);
        let mut sink = FakeSink::new(100);
        let out = render_paused(&mut source, &mut sink, &stop, &pause, &mut |_| {});
        assert_eq!(out, Outcome::Stopped { frames: 0 });
    }

    #[test]
    fn progress_reports_the_frames_that_actually_went_out() {
        let mut source = Blocks(vec![block(10), block(6)], None);
        let mut sink = FakeSink::new(4);
        let mut warnings = Vec::new();
        let out = render_with(&mut source, &mut sink, &StopSignal::new(), &mut |f| warnings.push(f));
        assert_eq!(out, Outcome::Finished { frames: 16 });
        assert_eq!(warnings.last(), Some(&16));
        assert!(warnings.windows(2).all(|p| p[0] < p[1]), "progress only moves forward");
    }

    #[test]
    fn a_source_that_breaks_is_not_a_track_that_ended() {
        let mut source = Blocks(vec![block(10), block(6)], Some("sector 555 is invalid".into()));
        let out = render(&mut source, &mut FakeSink::new(100), &StopSignal::new());
        assert_eq!(out, Outcome::Unreadable { frames: 16, reason: "sector 555 is invalid".into() });
        assert!(!out.may_advance_queue());
    }

    #[test]
    fn a_full_device_is_waited_for_and_nothing_is_dropped() {
        // JACK and CoreAudio answer 0 while their queue is full: nothing was
        // taken, and the caller comes back. The loop took 0 as "on to the
        // next block" and dropped the rest of this one.
        let mut source = Blocks(vec![block(10), block(6)], None);
        let mut sink = FakeSink::new(100);
        sink.full = 3;
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 16 });
        assert_eq!(sink.received, 16, "every frame reached the device");
    }

    #[test]
    fn stopping_while_the_device_is_full_returns_at_once() {
        let mut source = Blocks(vec![block(10)], None);
        let mut sink = FakeSink::new(100);
        sink.full = usize::MAX;
        let stop = StopSignal::new();
        let stopper = stop.clone();
        let asked = std::time::Instant::now();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            stopper.stop();
        });
        assert_eq!(render(&mut source, &mut sink, &stop), Outcome::Stopped { frames: 0 });
        assert!(asked.elapsed() < Duration::from_secs(1), "it waited {:?}", asked.elapsed());
    }

    /// A device that plays nothing: it holds all it is handed, until a flush.
    struct Holding(u64);

    impl AudioSink for Holding {
        fn format(&self) -> StreamFormat {
            StreamFormat::new(SampleFormat::S16, 44_100, 2)
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            self.0 += frames.len() as u64 / 4;
            Ok(frames.len() / 4)
        }
        fn held(&self) -> u64 {
            self.0
        }
        fn flush(&mut self) -> Result<(), SinkError> {
            self.0 = 0;
            Ok(())
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    /// Blocks, the second of them from somewhere else in the track.
    struct Moving(Vec<Vec<u8>>, bool);

    impl FrameSource for Moving {
        fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
            if self.0.is_empty() {
                return Ok(None);
            }
            self.1 = self.0.len() == 1;
            Ok(Some(self.0.remove(0)))
        }
        fn moved(&mut self) -> bool {
            std::mem::take(&mut self.1)
        }
    }

    #[test]
    fn progress_counts_what_was_heard_and_not_what_the_device_holds() {
        // The device holds everything and plays nothing, so nothing has been
        // heard: before a seek, and after one that threw away what it held.
        let mut heard = Vec::new();
        let out = render_with(
            &mut Moving(vec![block(10), block(10)], false),
            &mut Holding(0),
            &StopSignal::new(),
            &mut |f| heard.push(f),
        );
        assert_eq!(out, Outcome::Finished { frames: 10 }, "ten frames handed over: the ten read ahead of the seek went");
        assert_eq!(heard, [0], "and none of them heard");
    }

    /// Blocks to hand over, and then either the end or, when set, a failure.
    struct Blocks(Vec<Vec<u8>>, Option<String>);

    impl FrameSource for Blocks {
        fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
            if !self.0.is_empty() {
                return Ok(Some(self.0.remove(0)));
            }
            match self.1.take() {
                Some(reason) => Err(reason),
                None => Ok(None),
            }
        }
    }

    /// A fake device: it takes `accept` frames per write, answers the first
    /// `full` writes with nothing taken, and fails on write number `fail_at`
    /// with `error`.
    struct FakeSink {
        accept: usize,
        full: usize,
        fail_at: Option<(usize, SinkError)>,
        writes: usize,
        received: usize,
        closed: bool,
    }

    impl FakeSink {
        fn new(accept: usize) -> Self {
            Self { accept, full: 0, fail_at: None, writes: 0, received: 0, closed: false }
        }

        fn failing(accept: usize, at: usize, error: SinkError) -> Self {
            Self { fail_at: Some((at, error)), ..Self::new(accept) }
        }
    }

    impl AudioSink for FakeSink {
        fn format(&self) -> StreamFormat {
            StreamFormat::new(SampleFormat::S16, 44_100, 2)
        }

        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            self.writes += 1;
            if let Some((at, error)) = &self.fail_at {
                if self.writes == *at {
                    return Err(error.clone());
                }
            }
            if self.full > 0 {
                self.full -= 1;
                return Ok(0);
            }
            let fits = (frames.len() / 4).min(self.accept);
            self.received += fits;
            Ok(fits)
        }

        fn close(&mut self) -> Result<(), SinkError> {
            self.closed = true;
            Ok(())
        }
    }

    fn block(frames: usize) -> Vec<u8> {
        vec![0u8; frames * 4]
    }

    /// A source that hands over `blocks` blocks of `frames` frames at once,
    /// except one, which it holds for `stall`.
    struct Stalling {
        blocks: usize,
        frames: usize,
        given: usize,
        stalls_at: usize,
        stall: Duration,
    }

    impl FrameSource for Stalling {
        fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
            if self.given >= self.blocks {
                return Ok(None);
            }
            if self.given == self.stalls_at {
                std::thread::sleep(self.stall);
            }
            self.given += 1;
            Ok(Some(block(self.frames)))
        }
    }

    /// A device that plays what it is given in real time, and remembers the
    /// longest it was left waiting between two writes.
    struct Paced {
        rate: u64,
        last: Option<std::time::Instant>,
        longest_wait: Duration,
        writes: usize,
    }

    impl AudioSink for Paced {
        fn format(&self) -> StreamFormat {
            StreamFormat::new(SampleFormat::S16, self.rate as u32, 2)
        }

        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            if let Some(last) = self.last {
                self.longest_wait = self.longest_wait.max(last.elapsed());
            }
            let count = frames.len() / 4;
            std::thread::sleep(Duration::from_micros(count as u64 * 1_000_000 / self.rate));
            self.last = Some(std::time::Instant::now());
            self.writes += 1;
            Ok(count)
        }

        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    /// The source stops answering for longer than the device could hold,
    /// and the device never notices: what was read ahead carries it.
    #[test]
    fn a_source_that_stalls_does_not_leave_the_device_waiting() {
        // 8,000 frames a second, 400 frames a block: 50 ms each, and the
        // cushion holds two seconds of them. The stall comes once the
        // cushion is full.
        let mut source = Stalling { blocks: 80, frames: 400, given: 0, stalls_at: 60, stall: Duration::from_millis(600) };
        let mut sink = Paced { rate: 8_000, last: None, longest_wait: Duration::ZERO, writes: 0 };
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 80 * 400 });
        assert_eq!(sink.writes, 80);
        assert!(sink.longest_wait < Duration::from_millis(300), "the device waited {:?} on a 600 ms stall", sink.longest_wait);
    }

    /// Blocks numbered as they are read, the fifth of which comes from a
    /// seek; it ends after the seventh, by itself or by a skip.
    struct Marked {
        next: u8,
        end_after: u8,
        skip: bool,
        skipped: bool,
    }

    impl FrameSource for Marked {
        fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
            if self.next > self.end_after {
                self.skipped = self.skip;
                return Ok(None);
            }
            self.next += 1;
            Ok(Some(vec![self.next - 1; 4]))
        }
        fn moved(&mut self) -> bool {
            self.next == 5
        }
        fn ended_early(&self) -> bool {
            self.skipped
        }
    }

    /// Reads the whole source ahead, the device taking nothing meanwhile,
    /// and hands back what is left to take.
    fn read_whole<S: FrameSource + Send>(source: &mut S) -> Vec<Result<Option<(Vec<u8>, Option<Option<u64>>)>, String>> {
        let cushion = Cushion::new(1 << 20);
        let stop = StopSignal::new();
        std::thread::scope(|threads| {
            threads.spawn(|| cushion.read(source, &stop));
            while cushion.lock().end.is_none() {
                std::thread::sleep(Duration::from_millis(5));
            }
            let mut taken = Vec::new();
            loop {
                let next = cushion.take(&stop);
                let over = !matches!(next, Ok(Some(_)));
                taken.push(next);
                if over {
                    break;
                }
            }
            cushion.finish();
            taken
        })
    }

    /// What was read ahead of a seek is from where the music no longer is,
    /// and goes; what was read ahead of a skip is not music to play.
    #[test]
    fn a_seek_and_a_skip_throw_away_what_was_read_ahead() {
        let taken = read_whole(&mut Marked { next: 0, end_after: 6, skip: false, skipped: false });
        let blocks: Vec<(u8, bool)> = taken.iter().filter_map(|t| t.as_ref().ok()?.as_ref()).map(|(block, moved)| (block[0], moved.is_some())).collect();
        assert_eq!(blocks, [(4, true), (5, false), (6, false)], "the blocks before the seek went");
        assert!(matches!(taken.last(), Some(Ok(None))));

        let taken = read_whole(&mut Marked { next: 0, end_after: 6, skip: true, skipped: false });
        assert!(matches!(taken.as_slice(), [Ok(None)]), "everything read ahead of a skip is dropped");
    }

    /// A source that cannot be read on says so through the cushion.
    #[test]
    fn a_failure_reaches_the_device_side_in_order() {
        let taken = read_whole(&mut Blocks(vec![block(2)], Some("a bad sector".into())));
        assert!(matches!(taken[0], Ok(Some(_))), "what was read before the failure still plays");
        assert_eq!(taken[1], Err("a bad sector".into()));
        assert_eq!(taken.len(), 2);
    }

    #[test]
    fn a_finished_track_lets_the_queue_advance() {
        let mut source = Blocks(vec![block(10), block(10)], None);
        let mut sink = FakeSink::new(100);
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 20 });
        assert!(out.may_advance_queue());
    }

    #[test]
    fn partial_writes_are_retried_until_everything_is_delivered() {
        let mut source = Blocks(vec![block(10)], None);
        let mut sink = FakeSink::new(3);
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 10 });
        assert_eq!(sink.received, 10);
        assert!(sink.writes > 1, "several writes were needed");
    }

    #[test]
    fn a_lost_device_is_not_mistaken_for_end_of_track() {
        // The finding: in QBZ this advanced the whole queue without a word.
        let mut source = Blocks(vec![block(10), block(10)], None);
        let mut sink = FakeSink::failing(100, 1, SinkError::DeviceLost);
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::DeviceLost });
        assert!(!out.may_advance_queue(), "the queue does NOT advance");
    }

    #[test]
    fn a_timeout_is_a_failure_too_and_does_not_wait_forever() {
        let mut source = Blocks(vec![block(4)], None);
        let wait = Duration::from_millis(200);
        let mut sink = FakeSink::failing(100, 1, SinkError::Timeout(wait));
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::Timeout(wait) });
    }

    #[test]
    fn a_lone_xrun_is_absorbed_and_the_track_finishes() {
        let mut source = Blocks(vec![block(6)], None);
        let mut sink = FakeSink::failing(100, 1, SinkError::Xrun);
        assert_eq!(render(&mut source, &mut sink, &StopSignal::new()), Outcome::Finished { frames: 6 });
    }

    #[test]
    fn many_xruns_in_a_row_give_the_device_up_for_lost() {
        struct AlwaysXrun;
        impl AudioSink for AlwaysXrun {
            fn format(&self) -> StreamFormat {
                StreamFormat::new(SampleFormat::S16, 44_100, 2)
            }
            fn write(&mut self, _: &[u8]) -> Result<usize, SinkError> {
                Err(SinkError::Xrun)
            }
            fn close(&mut self) -> Result<(), SinkError> {
                Ok(())
            }
        }
        let mut source = Blocks(vec![block(4)], None);
        let out = render(&mut source, &mut AlwaysXrun, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::DeviceLost });
    }

    #[test]
    fn stopping_does_not_wait_for_the_device() {
        // The signal is already set before starting: the loop sees it
        // between blocks and returns without asking the device anything.
        let stop = StopSignal::new();
        stop.stop();
        let mut source = Blocks(vec![block(10)], None);
        let mut sink = FakeSink::new(1);
        assert_eq!(render(&mut source, &mut sink, &stop), Outcome::Stopped { frames: 0 });
        assert_eq!(sink.writes, 0, "nothing was written to the device");
    }

    #[test]
    fn stopping_mid_track_returns_between_writes() {
        struct StopsOnTheThird(StopSignal, usize);
        impl AudioSink for StopsOnTheThird {
            fn format(&self) -> StreamFormat {
                StreamFormat::new(SampleFormat::S16, 44_100, 2)
            }
            fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
                self.1 += 1;
                if self.1 == 3 {
                    self.0.stop();
                }
                Ok((frames.len() / 4).min(2))
            }
            fn close(&mut self) -> Result<(), SinkError> {
                Ok(())
            }
        }
        let stop = StopSignal::new();
        let mut source = Blocks(vec![block(20)], None);
        let mut sink = StopsOnTheThird(stop.clone(), 0);
        assert_eq!(render(&mut source, &mut sink, &stop), Outcome::Stopped { frames: 6 });
    }
}
