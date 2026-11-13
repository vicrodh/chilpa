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

    /// I retain floating PCM headroom until gain, before integer conversion.
    fn pcm_format(&self) -> Option<crate::format::StreamFormat> { None }

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
    fn position_at_move(&self) -> Option<u64> { None }

    /// How far ahead of the device it wants to be read: [`AHEAD`] by
    /// default. A track read over the network asks for [`NETWORK_AHEAD`].
    fn ahead(&self) -> std::time::Duration {
        AHEAD
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

/// The cushion of a track read over the network: ten seconds. A source can
/// take seconds to answer one read -- a plugin that serves its stream from
/// its own process while it also opens the next track -- and two seconds
/// ran a 192 kHz stream dry on a 3 s read (the owner's trace, 2026-10-09).
/// At 192 kHz in 32-bit stereo it holds 15 MB.
pub const NETWORK_AHEAD: std::time::Duration = std::time::Duration::from_secs(10);

/// A read that takes this long is written down: it is what the cushion is
/// for, and the log should say when it was needed.
const SLOW_READ: std::time::Duration = std::time::Duration::from_millis(100);

/// What was read ahead and not yet handed to the device. A block from
/// somewhere else in the track carries how much had been heard when the
/// source moved, when the source counts that: what the loop wrote after
/// that, from the old place, is not where the music is.
struct Ahead {
    blocks: std::collections::VecDeque<(Vec<u8>, Option<(Option<u64>, Option<u64>)>)>,
    bytes: usize,
    /// How the source ended, once it has: `Ok` when it was over, `Err` with
    /// the reason when it could not be read on.
    end: Option<Result<(), String>>,
    /// The loop asked the source to go back to what was heard, and is
    /// waiting to hear whether it could.
    rewind: Option<Option<bool>>,
    /// The loop is done with the source.
    done: bool,
    /// The thread reading the source died. It stays said: whoever waits on
    /// that thread, now or later, is not kept waiting for an answer that
    /// will never come.
    failed: bool,
}

/// What a wait on a reader that died is answered with. The loop does not
/// carry it far: it joins the reader next and hands its panic on.
const READER_DIED: &str = "the thread reading the source died";

/// Lets the reader go once the loop is done with it, and when the loop
/// unwinds: a reader left waiting for room keeps the scope from ever joining.
struct Finish<'a>(&'a Cushion);

impl Drop for Finish<'_> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

/// Says the reader died, when it unwinds, so the loop stops waiting on it.
struct Reading<'a>(&'a Cushion);

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.lock().failed = true;
            self.0.changed.notify_all();
        }
    }
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
            ahead: std::sync::Mutex::new(Ahead { blocks: Default::default(), bytes: 0, end: None, rewind: None, done: false, failed: false }),
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
            match read {
                Ok(Some(block)) => {
                    let moved = source.moved().then(|| (source.heard_at_move(), source.position_at_move()));
                    let mut ahead = self.lock();
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
                    let ended_early = source.ended_early();
                    let mut ahead = self.lock();
                    if ended_early {
                        ahead.blocks.clear();
                        ahead.bytes = 0;
                        crate::diagnostics::trace("render", "ahead_dropped", format_args!("reason=skip"));
                    }
                    if ahead.end.is_none() {
                        ahead.end = Some(Ok(()));
                    }
                }
                Err(reason) => self.lock().end = Some(Err(reason)),
            }
            self.changed.notify_all();
        }
    }

    /// The next block read ahead and, when it comes from somewhere else in
    /// the track, how much had been heard when the source moved; `Ok(None)`
    /// once the source is over, `Err` when it could not be read on. Waits
    /// for one, minding the stop.
    fn take(&self, stop: &StopSignal) -> Result<Option<(Vec<u8>, Option<(Option<u64>, Option<u64>)>)>, String> {
        let mut ahead = self.lock();
        let mut starved: Option<std::time::Instant> = None;
        loop {
            if ahead.failed {
                return Err(READER_DIED.into());
            }
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
            // Nobody is left to answer: checked on every turn, so a wait
            // that starts after the reader died ends too.
            if ahead.failed {
                ahead.rewind = None;
                return false;
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

    /// Whether the thread reading the source died.
    fn failed(&self) -> bool {
        self.lock().failed
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
    /// ONLY ending that counts as played whole.
    Finished { frames: u64 },
    /// Somebody asked to stop.
    Stopped { frames: u64 },
    /// The device failed. The queue does not advance and the user finds out.
    Failed { frames: u64, error: SinkError },
    /// The source could not be read on: a damaged file, a disc image with a
    /// bad sector. Not the device: a queue says so and passes over it.
    /// `frames` leaves out what a seek threw away unheard: the queue goes on,
    /// what the device still holds of the track is heard, and the count is
    /// what is logged as listened to.
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
    render_processed(source, sink, stop, pause, None, on_progress)
}

/// PCM only: processing runs after read-ahead, once per block, before any
/// partial device writes. Raw one-bit sources use `render_paused` instead.
pub fn render_processed<S, K>(
    source: &mut S,
    sink: &mut K,
    stop: &StopSignal,
    pause: &PauseSignal,
    control: Option<crate::dsp::Control>,
    on_progress: &mut dyn FnMut(u64),
) -> Outcome
where
    S: FrameSource + Send,
    K: AudioSink,
{
    let mut processor = control.map(|control| crate::dsp::Processor::new(control, sink.format()));
    render_with_processor(source, sink, stop, pause, processor.as_mut(), on_progress)
}

/// A queue lends its processor across contiguous gapless tracks, keeping
/// filter and delay histories at the boundary. Seeking still resets them.
pub fn render_with_processor<S, K>(
    source: &mut S,
    sink: &mut K,
    stop: &StopSignal,
    pause: &PauseSignal,
    processor: Option<&mut crate::dsp::Processor>,
    on_progress: &mut dyn FnMut(u64),
) -> Outcome
where
    S: FrameSource + Send,
    K: AudioSink,
{
    render_with_volume(source, sink, stop, pause, processor, None, on_progress)
}

/// I apply combined PCM gains after read-ahead and before DSP.
/// One-bit callers leave volume absent, including DoP in PCM containers.
pub fn render_with_volume<S: FrameSource + Send, K: AudioSink>(
    source: &mut S,
    sink: &mut K,
    stop: &StopSignal,
    pause: &PauseSignal,
    processor: Option<&mut crate::dsp::Processor>,
    volume: Option<&mut dyn FnMut(&mut [u8])>,
    on_progress: &mut dyn FnMut(u64),
) -> Outcome {
    let format = sink.format();
    let source_format = source.pcm_format().unwrap_or(format);
    let frame_bytes = source_format.frame_bytes();
    let fill = frame_bytes * format.sample_rate_hz as usize * source.ahead().as_millis() as usize / 1000;
    let cushion = Cushion::new(fill.max(frame_bytes));
    let started = std::time::Instant::now();
    std::thread::scope(|threads| {
        let reader = std::thread::Builder::new().name("chilpa-read-ahead".into());
        let reading = reader.spawn_scoped(threads, || {
            let _reading = Reading(&cushion);
            cushion.read(source, stop)
        });
        let Ok(reading) = reading else {
            return Outcome::Failed { frames: 0, error: SinkError::Backend(chilpa_i18n::t("the read-ahead thread could not start")) };
        };
        let outcome = {
            let _finish = Finish(&cushion);
            deliver(&cushion, sink, stop, pause, on_progress, frame_bytes, started, processor, volume, Some(source_format))
        };
        // A reader that panicked is not a track that ended: whoever plays
        // hears of it as the panic it was.
        if let Err(panic) = reading.join() {
            std::panic::resume_unwind(panic);
        }
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
    mut processor: Option<&mut crate::dsp::Processor>,
    mut volume: Option<&mut dyn FnMut(&mut [u8])>,
    source_format: Option<crate::format::StreamFormat>,
) -> Outcome {
    let format = sink.format();
    let source_format = source_format.unwrap_or(format);
    let conversion = source_format.sample_format == crate::format::SampleFormat::F32
        || source_format.sample_format != format.sample_format;
    // Existing gain/conversion chunks are already bounded. Keep one source
    // copy across partial writes; visibility never changes audio chunk sizes.
    let mut source_pcm = [0u8; 16 * 1024];
    let mut captured_chunk = false;
    let separate_source = volume.is_some() || conversion;
    // I reserve the bound once so a short first chunk cannot make Vec growth
    // retain more than 16 KiB. Matching integer formats need no conversion allocation.
    let mut converted = if conversion { Vec::with_capacity(16 * 1024) } else { Vec::new() };
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
        let (mut block, moved) = match cushion.take(stop) {
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
                return Outcome::Unreadable { frames: frames.saturating_sub(thrown_away), reason };
            }
        };
        // From somewhere else in the track: what the device holds is from
        // before, and it goes; so does what went out from before since the
        // source moved, which was heard but is not where the music is.
        if let Some((heard_at_move, position)) = moved {
            if let Some(processor) = &mut processor { processor.reset(); }
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
            sink.position_changed(position);
        }
        let mut offset = 0;
        let mut processed_end = 0;
        let mut chunk_start = 0;
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
                // A pause is waited out, but not on a reader that died.
                if cushion.failed() {
                    return Outcome::Stopped { frames };
                }
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
            if offset == processed_end {
                // I prepare at most 5 ms (and 16 KiB) when live gain is
                // present. Retries reuse this chunk without advancing a
                // normalization ramp or DSP history twice. Read-ahead stays
                // unchanged; a blocked device can retain this one chunk.
                let chunk = if volume.is_some() || conversion {
                    (format.sample_rate_hz as usize / 200).max(1)
                        .min((16 * 1024 / frame_bytes.max(format.frame_bytes())).max(1)) * frame_bytes
                } else { block.len() };
                chunk_start = offset;
                processed_end = (offset + chunk).min(block.len());
                let bytes = &mut block[offset..processed_end];
                captured_chunk = separate_source && sink.wants_source_pcm();
                if captured_chunk { source_pcm[..bytes.len()].copy_from_slice(bytes); }
                if let Some(volume) = &mut volume { volume(bytes); }
                if conversion {
                    crate::dsp::volume::convert_pcm(bytes, source_format.sample_format, format.sample_format, &mut converted);
                    if let Some(processor) = &mut processor { processor.process(&mut converted); }
                } else if let Some(processor) = &mut processor { processor.process(bytes); }
            }
            let bytes = if conversion {
                &converted[(offset - chunk_start) / frame_bytes * format.frame_bytes()..]
            } else { &block[offset..processed_end] };
            let result = if separate_source {
                let source = captured_chunk.then(|| crate::sink::SourcePcm {
                    bytes: &source_pcm[offset - chunk_start..processed_end - chunk_start],
                    format: source_format,
                });
                sink.write_with_source(bytes, source)
            } else {
                // Matching integer PCM without gain keeps the existing output
                // history, including DSP, and its original full-block processing.
                sink.write(bytes)
            };
            match result {
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
#[path = "render_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "render_lock_tests.rs"]
mod lock_tests;

#[cfg(test)]
#[path = "render_volume_tests.rs"]
mod volume_tests;

#[cfg(test)]
#[path = "render_source_tests.rs"]
mod source_tests;
