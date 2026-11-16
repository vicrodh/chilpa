use super::*;
use crate::format::{SampleFormat, StreamFormat};
use std::time::Duration;

/// Whether `run` panicked, and did so in time: a render that waits on a
/// thread that died never returns, and that is the failure looked for.
fn panicked_in_time(run: impl FnOnce() + Send + 'static) -> bool {
    let (done, seen) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
        let _ = done.send(result.is_err());
    });
    seen.recv_timeout(Duration::from_secs(15)).expect("the render never returned")
}

/// Blocks without end, so the reader fills the cushion and waits for room.
struct Endless;

impl FrameSource for Endless {
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(block(64)))
    }
}

/// A device that panics the first time it is written to.
struct PanickingSink;

impl AudioSink for PanickingSink {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, _: &[u8]) -> Result<usize, SinkError> {
        panic!("the device panicked");
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// One block, then a panic: on the next read, or when asked to go back.
struct DiesReading {
    given: bool,
    on_rewind: bool,
}

impl FrameSource for DiesReading {
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        if !std::mem::replace(&mut self.given, true) {
            return Ok(Some(block(8)));
        }
        if self.on_rewind {
            return Ok(None);
        }
        panic!("the source panicked reading");
    }
    fn rewind(&mut self) -> bool {
        panic!("the source panicked going back");
    }
}

#[test]
fn a_device_that_panics_lets_the_reader_go_and_the_panic_out() {
    assert!(panicked_in_time(|| {
        render(&mut Endless, &mut PanickingSink, &StopSignal::new());
    }));
}

#[test]
fn a_reader_that_panics_while_playing_ends_the_render_as_a_panic() {
    assert!(panicked_in_time(|| {
        let mut source = DiesReading { given: false, on_rewind: false };
        render(&mut source, &mut FakeSink::new(100), &StopSignal::new());
    }));
}

#[test]
fn a_reader_that_panics_during_a_pause_ends_the_render_with_no_stop() {
    assert!(panicked_in_time(|| {
        let pause = PauseSignal::new();
        pause.set(true);
        let mut source = DiesReading { given: false, on_rewind: false };
        render_paused(&mut source, &mut FakeSink::new(100), &StopSignal::new(), &pause, &mut |_| {});
    }));
}

#[test]
fn a_reader_that_panics_going_back_ends_the_wait_for_its_answer() {
    assert!(panicked_in_time(|| {
        let pause = PauseSignal::new();
        pause.set(true);
        let mut source = DiesReading { given: false, on_rewind: true };
        render_paused(&mut source, &mut FakeSink::new(100), &StopSignal::new(), &pause, &mut |_| {});
    }));
}

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

/// A device that holds all it is handed until a flush, and says how much
/// it was handed in all.
struct Counting(Holding, std::sync::Arc<std::sync::atomic::AtomicU64>);

impl AudioSink for Counting {
    fn format(&self) -> StreamFormat {
        self.0.format()
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let taken = self.0.write(frames)?;
        self.1.fetch_add(taken as u64, std::sync::atomic::Ordering::Release);
        Ok(taken)
    }
    fn held(&self) -> u64 {
        self.0.held()
    }
    fn flush(&mut self) -> Result<(), SinkError> {
        self.0.flush()
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// Ten frames; then, once the device has them, ten more from somewhere
/// else in the track; then a read that fails.
struct SoughtThenBroken(usize, std::sync::Arc<std::sync::atomic::AtomicU64>, bool);

impl FrameSource for SoughtThenBroken {
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.0 += 1;
        match self.0 {
            1 => Ok(Some(block(10))),
            2 => {
                while self.1.load(std::sync::atomic::Ordering::Acquire) < 10 {
                    std::thread::sleep(Duration::from_millis(1));
                }
                self.2 = true;
                Ok(Some(block(10)))
            }
            _ => Err("the file is cut here".into()),
        }
    }
    fn moved(&mut self) -> bool {
        std::mem::take(&mut self.2)
    }
}

#[test]
fn a_source_that_breaks_after_a_seek_does_not_count_what_the_seek_threw_away() {
    // Ten frames are in the device when the seek comes: they are flushed
    // unheard. The ten after the seek are all that can be heard.
    let written = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let out = render(
        &mut SoughtThenBroken(0, written.clone(), false),
        &mut Counting(Holding(0), written.clone()),
        &StopSignal::new(),
    );
    assert_eq!(written.load(std::sync::atomic::Ordering::Acquire), 20, "twenty frames were handed over");
    assert_eq!(out, Outcome::Unreadable { frames: 10, reason: "the file is cut here".into() });
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
    ahead: Duration,
}

impl FrameSource for Stalling {
    fn ahead(&self) -> Duration {
        self.ahead
    }

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
    let mut source = Stalling { blocks: 80, frames: 400, given: 0, stalls_at: 60, stall: Duration::from_millis(600), ahead: AHEAD };
    let mut sink = Paced { rate: 8_000, last: None, longest_wait: Duration::ZERO, writes: 0 };
    let out = render(&mut source, &mut sink, &StopSignal::new());
    assert_eq!(out, Outcome::Finished { frames: 80 * 400 });
    assert_eq!(sink.writes, 80);
    assert!(sink.longest_wait < Duration::from_millis(300), "the device waited {:?} on a 600 ms stall", sink.longest_wait);
}

/// The cushion is as deep as the source asks: one that asks for 300 ms
/// is read 300 ms ahead, so a 600 ms stall reaches the device. A track
/// over the network asks for ten seconds the same way.
#[test]
fn the_cushion_is_as_deep_as_the_source_asks() {
    let mut source = Stalling { blocks: 40, frames: 400, given: 0, stalls_at: 30, stall: Duration::from_millis(600), ahead: Duration::from_millis(300) };
    let mut sink = Paced { rate: 8_000, last: None, longest_wait: Duration::ZERO, writes: 0 };
    let out = render(&mut source, &mut sink, &StopSignal::new());
    assert_eq!(out, Outcome::Finished { frames: 40 * 400 });
    assert!(sink.longest_wait >= Duration::from_millis(200), "a 300 ms cushion hid a 600 ms stall: the device waited {:?}", sink.longest_wait);
    assert!(NETWORK_AHEAD > AHEAD);
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
fn read_whole<S: FrameSource + Send>(source: &mut S) -> Vec<Result<Option<(Vec<u8>, Option<(Option<u64>, Option<u64>)>)>, String>> {
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
