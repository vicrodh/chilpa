//! Watching what goes out, without touching it.
//!
//! A visualizer needs the samples that were really delivered. The only
//! honest way to give it them is to copy them **after** packing and
//! **before** writing, without changing a single byte of what goes to the
//! device.
//!
//! Three rules that keep this from eating bit-perfect or the CPU:
//!
//! 1. **What is delivered is never modified.** [`TapSink`] copies and
//!    forwards; what it writes is exactly what it was given, and only what
//!    the device accepted counts as delivered.
//! 2. **The copy is fixed-size and overwrites itself.** A small ring
//!    buffer: if nobody looks, the old is dropped. It never grows and never
//!    blocks the audio.
//! 3. **If the lock is busy, the copy is skipped.** The thread that plays
//!    does not wait for the one that draws. Losing a visualizer frame
//!    matters to nobody; a gap in the audio does.

use crate::format::{SampleFormat, StreamFormat};
use crate::sink::{AudioSink, SinkError};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

mod history;
use history::History;

/// How many frames are kept to look at. At 192 kHz that is about 43 ms,
/// which is more than fits in a screen frame.
pub const FRAMES_KEPT: usize = 8192;

/// The last thing that went out, for whoever wants to draw it.
#[derive(Clone, Default)]
pub struct Tap(Arc<Mutex<State>>, Arc<crate::tap_dsd::RawTap>, Arc<AtomicBool>);

#[derive(Default)]
struct State {
    /// Samples normalized to -1.0..1.0, interleaved by channel.
    samples: Vec<f32>,
    /// Where the write head is inside the ring.
    cursor: usize,
    channels: u16,
    sample_rate_hz: u32,
    /// How many frames have been copied since it opened.
    frames: u64,
    dsd: bool,
    /// Only a visible visualizer retains output-playhead history.
    visualizing: bool,
    history: History,
}

/// A snapshot of what just went out.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Snapshot {
    /// The samples in chronological order, interleaved by channel.
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate_hz: u32,
    /// The per-channel peak of what was kept, from 0.0 to 1.0.
    pub peaks: Vec<f32>,
    /// The per-channel RMS, which is what is heard as loudness.
        pub rms: Vec<f32>,
}

impl Tap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Prepares the ring for a format. Called when the device opens.
    fn prepare(&self, format: StreamFormat, dop: bool) {
        self.1.prepare(crate::tap_dsd::Encoding { format, dop });
        if let Ok(mut e) = self.0.lock() {
            let channels = format.channels.max(1);
            e.samples = vec![0.0; FRAMES_KEPT * channels as usize];
            e.cursor = 0;
            e.channels = channels;
            e.sample_rate_hz = format.sample_rate_hz;
            e.frames = 0;
            e.dsd = format.sample_format.is_dsd() || dop;
            let active = e.visualizing && !e.dsd;
            e.history.prepare(format.sample_rate_hz, active);
        }
    }

    /// Copies what was delivered. If the lock is busy, it does nothing.
    fn copy(&self, bytes: &[u8], format: StreamFormat, held: u64) {
        let Ok(mut e) = self.0.try_lock() else { return };
        if e.samples.is_empty() || e.dsd {
            return;
        }
        let channels = e.channels.max(1) as usize;
        let width = format.sample_format.bytes();
        if width == 0 {
            return;
        }
        let total = e.samples.len();
        let mut cursor = e.cursor;
        let mut written = 0usize;
        if e.visualizing {
            for frame in bytes.chunks_exact(width * channels) {
                let mut stereo = [0.0; 2];
                for (channel, sample) in frame.chunks_exact(width).enumerate() {
                    let value = normalize(sample, format.sample_format);
                    e.samples[cursor] = value;
                    cursor = (cursor + 1) % total;
                    written += 1;
                    if channel < 2 { stereo[channel] = value; }
                }
                if channels == 1 { stereo[1] = stereo[0]; }
                e.history.push(stereo);
            }
            e.history.set_delay(held);
        } else {
            for sample in bytes.chunks_exact(width) {
                e.samples[cursor] = normalize(sample, format.sample_format);
                cursor = (cursor + 1) % total;
                written += 1;
            }
        }
        e.cursor = cursor;
        e.frames += (written / channels) as u64;
    }

    /// The last thing that went out, in chronological order.
    pub fn snapshot(&self) -> Snapshot {
        let Ok(e) = self.0.lock() else { return Snapshot::default() };
        if e.samples.is_empty() || e.dsd {
            return Snapshot::default();
        }
        let channels = e.channels.max(1) as usize;
        // The ring is straightened: from the cursor onwards is the oldest
        // part.
        let mut samples = Vec::with_capacity(e.samples.len());
        samples.extend_from_slice(&e.samples[e.cursor..]);
        samples.extend_from_slice(&e.samples[..e.cursor]);

        let mut peaks = vec![0.0f32; channels];
        let mut sums = vec![0.0f64; channels];
        for (i, m) in samples.iter().enumerate() {
            let c = i % channels;
            peaks[c] = peaks[c].max(m.abs());
            sums[c] += f64::from(*m) * f64::from(*m);
        }
        let per_channel = (samples.len() / channels).max(1) as f64;
        let rms = sums.iter().map(|s| (s / per_channel).sqrt() as f32).collect();

        Snapshot {
            samples: samples,
            channels: e.channels,
            sample_rate_hz: e.sample_rate_hz,
            peaks,
            rms,
        }
    }

    /// I retain at most 4 MiB of stereo history only while a view consumes it.
    /// I allocate on the control thread, never in the device write path.
    pub fn set_visualizing(&self, active: bool) {
        let Ok(mut state) = self.0.lock() else { return };
        if state.visualizing == active { return; }
        state.visualizing = active;
        let rate = state.sample_rate_hz;
        let pcm = !state.dsd;
        state.history.prepare(rate, active && pcm);
        self.2.store(active, Ordering::Relaxed);
    }

    /// I copy only the analysis window, without allocating or calculating meters.
    /// The device delay selects the audible window, not the last queued block.
    pub fn visualizer_snapshot(&self, stereo: &mut [f32]) -> u32 {
        let Ok(state) = self.0.lock() else { return 0 };
        if state.dsd || !state.visualizing || !state.history.snapshot(stereo) { return 0; }
        state.sample_rate_hz
    }

    fn reset_pcm(&self) {
        if let Ok(mut state) = self.0.lock() { state.history.reset(); }
    }

    pub(crate) fn drain_dsd(&self) -> crate::tap_dsd::Batch {
        self.1.drain()
    }

    /// How many frames have been copied.
    pub fn frames(&self) -> u64 {
        self.0.lock().map(|e| e.frames).unwrap_or(0)
    }

    /// Wraps a device to watch what is delivered to it.
    pub fn wrap<S: AudioSink>(&self, sink: S) -> TapSink<S> {
        let format = sink.format();
        self.prepare(format, false);
        TapSink { inner: sink, tap: self.clone(), format, dop: false }
    }
}

/// A wrapped device: it copies what is delivered and passes it through.
pub struct TapSink<S: AudioSink> {
    inner: S,
    tap: Tap,
    format: StreamFormat,
    dop: bool,
}

impl<S: AudioSink> AudioSink for TapSink<S> {
    fn set_dop(&mut self, dop: bool) {
        self.inner.set_dop(dop);
        self.dop = dop;
        self.format = self.inner.format();
        self.tap.prepare(self.format, dop);
    }

    fn position_changed(&mut self, position: Option<u64>) {
        self.tap.1.reset();
        self.tap.reset_pcm();
        self.inner.position_changed(position);
    }

    fn format(&self) -> StreamFormat {
        self.inner.format()
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let visualizing = self.tap.2.load(Ordering::Relaxed);
        // A visible PCM visualizer needs a regular feed instead of long
        // accepted-write bursts. Partial writes preserve pending DSP/gain and
        // device buffer sizes. Hidden taps use the original write unchanged.
        let frames = if visualizing && !self.dop && !self.format.sample_format.is_dsd() {
            let limit = pcm_feed_frames(self.format.sample_rate_hz) * self.format.frame_bytes();
            &frames[..frames.len().min(limit)]
        } else { frames };
        // Write first, then copy what the device accepted: copying more would
        // show samples that never played.
        let accepted = self.inner.write(frames)?;
        let Some(bytes) = accepted.checked_mul(self.format.frame_bytes()) else { return Ok(accepted) };
        if let Some(delivered) = frames.get(..bytes) {
            if self.dop || self.format.sample_format.is_dsd() {
                self.tap.1.copy(delivered);
            } else {
                let held = if visualizing { self.inner.held() } else { 0 };
                self.tap.copy(delivered, self.format, held);
            }
        }
        Ok(accepted)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        self.tap.reset_pcm();
        self.inner.close()
    }

    fn handoff(&mut self) -> Result<(), SinkError> {
        self.tap.reset_pcm();
        self.inner.handoff()
    }

    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        self.inner.drain(give_up)
    }

    fn flush(&mut self) -> Result<(), SinkError> {
        self.tap.1.reset();
        self.tap.reset_pcm();
        self.inner.flush()
    }

    fn held(&self) -> u64 {
        self.inner.held()
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        self.inner.set_paused(paused)
    }

    fn suspend(&mut self) -> Result<(), SinkError> {
        self.tap.1.reset();
        self.tap.reset_pcm();
        self.inner.suspend()
    }

    fn wake(&mut self) -> Result<(), SinkError> {
        self.inner.wake()
    }
}

fn pcm_feed_frames(sample_rate: u32) -> usize {
    u64::from(sample_rate).div_ceil(100).clamp(64, 8192) as usize
}

/// A packed sample, taken to -1.0..1.0.
fn normalize(bytes: &[u8], format: SampleFormat) -> f32 {
    match format {
        SampleFormat::S16 => {
            let v = i16::from_le_bytes([bytes[0], bytes[1]]);
            f32::from(v) / 32_768.0
        }
        // S24In32 is left-justified, so it reads the same as a 32-bit
        // integer.
        SampleFormat::S24In32 | SampleFormat::S32 => {
            let v = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            v as f32 / 2_147_483_648.0
        }
        SampleFormat::S24Packed => {
            // Three signed bytes: sign-extended to 32 to read them.
            let v = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]);
            v as f32 / 2_147_483_648.0
        }
        SampleFormat::F32 => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        // A one-bit stream has no PCM amplitude for the oscilloscope. It is
        // still counted and passed through byte-for-byte.
        SampleFormat::DsdU32Be | SampleFormat::DsdU32Le => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct MirrorSink {
        format: StreamFormat,
        written: Rc<RefCell<Vec<u8>>>,
        /// How many frames it takes per write, or all of them if zero.
        limit: usize,
    }

    impl AudioSink for MirrorSink {
        fn format(&self) -> StreamFormat {
            self.format
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            let n = frames.len() / self.format.frame_bytes();
            let n = if self.limit > 0 { n.min(self.limit) } else { n };
            let bytes = n * self.format.frame_bytes();
            self.written.borrow_mut().extend_from_slice(&frames[..bytes]);
            Ok(n)
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    fn mirror(format: StreamFormat, limit: usize) -> MirrorSink {
        MirrorSink { format, written: Rc::new(RefCell::new(Vec::new())), limit }
    }

    /// A 16-bit stereo ramp.
    fn ramp(frames: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let m = (i as i16).wrapping_mul(300);
            v.extend_from_slice(&m.to_le_bytes());
            v.extend_from_slice(&(-m).to_le_bytes());
        }
        v
    }

    #[test]
    fn only_a_visible_pcm_visualizer_splits_the_device_feed() {
        for (rate, frames) in [(44_100, 441), (48_000, 480), (96_000, 960), (192_000, 1920), (768_000, 7680)] {
            assert_eq!(pcm_feed_frames(rate), frames);
        }
        assert_eq!(pcm_feed_frames(0), 64);
        assert_eq!(pcm_feed_frames(u32::MAX), 8192);
        let format = StreamFormat::new(SampleFormat::S16, 48_000, 2);
        let tap = Tap::new();
        let mut sink = tap.wrap(mirror(format, 0));
        let bytes = ramp(4800);
        assert_eq!(sink.write(&bytes).unwrap(), 4800, "off preserves the original one-call write");
        tap.set_visualizing(true);
        assert_eq!(sink.write(&bytes).unwrap(), 480, "visible capture receives a ten-ms piece");
        tap.set_visualizing(false);
        assert_eq!(sink.write(&bytes).unwrap(), 4800, "hiding restores the original write immediately");
    }

    #[test]
    fn delayed_partial_writes_publish_only_accepted_audio_and_preserve_bytes() {
        struct QueuedSink { inner: MirrorSink, delay: u64 }
        impl AudioSink for QueuedSink {
            fn format(&self) -> StreamFormat { self.inner.format() }
            fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> { self.inner.write(bytes) }
            fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
            fn held(&self) -> u64 { self.delay }
        }
        let format = StreamFormat::new(SampleFormat::S16, 48_000, 2);
        let mirror = mirror(format, 480);
        let written = mirror.written.clone();
        let tap = Tap::new();
        tap.set_visualizing(true);
        let mut sink = tap.wrap(QueuedSink { inner: mirror, delay: 240 });
        let bytes = ramp(4800);
        let mut offset = 0;
        let mut out = [0.0; 4096];
        while offset < bytes.len() {
            let accepted = sink.write(&bytes[offset..]).unwrap();
            assert_eq!(accepted, 480);
            offset += accepted * format.frame_bytes();
            assert_eq!(tap.frames(), (offset / format.frame_bytes()) as u64);
            assert_eq!(tap.visualizer_snapshot(&mut out), 48_000);
            let last = offset / format.frame_bytes() - 240 - 1;
            let expected = normalize(&bytes[last * 4..last * 4 + 2], SampleFormat::S16);
            assert_eq!(out[4094], expected, "the last sample ends at the audible playhead");
        }
        assert_eq!(*written.borrow(), bytes, "partial writes neither drop nor replay PCM");
        sink.set_paused(true).unwrap();
        let before = out;
        tap.visualizer_snapshot(&mut out);
        assert_eq!(out, before, "without new accepted frames a pause keeps the picture");
        sink.set_paused(false).unwrap();
        sink.position_changed(Some(0));
        assert_eq!(tap.visualizer_snapshot(&mut out), 0);
        assert_eq!(out, [0.0; 4096], "a seek does not expose history from the old cursor");
        sink.write(&bytes[..1920]).unwrap();
        sink.flush().unwrap();
        assert_eq!(tap.visualizer_snapshot(&mut out), 0);
        sink.write(&bytes[..1920]).unwrap();
        sink.close().unwrap();
        assert_eq!(tap.visualizer_snapshot(&mut out), 0, "closed output has no stale visual frame");
    }

    #[test]
    fn watching_changes_not_a_single_byte() {
        let format = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let data = ramp(1000);

        let direct = mirror(format, 0);
        let mut d = direct.clone();
        d.write(&data).unwrap();

        let examined = mirror(format, 0);
        let tap = Tap::new();
        let mut m = tap.wrap(examined.clone());
        m.write(&data).unwrap();

        assert_eq!(
            *direct.written.borrow(),
            *examined.written.borrow(),
            "what is delivered with and without a tap is the same"
        );
        assert_eq!(*examined.written.borrow(), data, "and it is what it was given");
    }

    #[test]
    fn only_what_the_device_accepted_is_copied() {
        let format = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let tap = Tap::new();
        // It takes 10 frames per write; it is given 100.
        let mut s = tap.wrap(mirror(format, 10));
        let accepted = s.write(&ramp(100)).unwrap();
        assert_eq!(accepted, 10);
        assert_eq!(tap.frames(), 10, "the ones that did not play were not copied");
    }

    #[test]
    fn peak_and_rms_come_out_per_channel() {
        let format = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let tap = Tap::new();
        let mut s = tap.wrap(mirror(format, 0));

        // Left at full scale, right silent, filling the ring.
        let mut data = Vec::new();
        for _ in 0..FRAMES_KEPT {
            data.extend_from_slice(&i16::MAX.to_le_bytes());
            data.extend_from_slice(&0i16.to_le_bytes());
        }
        s.write(&data).unwrap();

        let snapshot = tap.snapshot();
        assert_eq!(snapshot.channels, 2);
        assert_eq!(snapshot.sample_rate_hz, 44_100);
        assert!(snapshot.peaks[0] > 0.99, "the left one is at full scale: {}", snapshot.peaks[0]);
        assert_eq!(snapshot.peaks[1], 0.0, "the right one is silent");
        assert!(snapshot.rms[0] > 0.99);
        assert_eq!(snapshot.rms[1], 0.0);
    }

    #[test]
    fn the_ring_is_straightened_and_the_latest_remains() {
        let format = StreamFormat::new(SampleFormat::S16, 44_100, 1);
        let tap = Tap::new();
        let mut s = tap.wrap(mirror(format, 0));

        // More is written than fits: the old is overwritten.
        let mut data = Vec::new();
        for i in 0..(FRAMES_KEPT + 100) {
            data.extend_from_slice(&((i % 1000) as i16).to_le_bytes());
        }
        s.write(&data).unwrap();

        let snapshot = tap.snapshot();
        assert_eq!(snapshot.samples.len(), FRAMES_KEPT);
        // The last sample is the last one written, not one from the middle.
        let last = (((FRAMES_KEPT + 99) % 1000) as i16) as f32 / 32_768.0;
        let seen = *snapshot.samples.last().unwrap();
        assert!((seen - last).abs() < 1e-6, "it ended up {seen}, {last} was expected");
    }

    #[test]
    fn with_nothing_written_the_snapshot_is_empty_and_does_not_crash() {
        let tap = Tap::new();
        assert_eq!(tap.snapshot(), Snapshot::default());
        assert_eq!(tap.frames(), 0);
    }

    #[test]
    fn samples_are_normalized_according_to_their_format() {
        assert!((normalize(&i16::MAX.to_le_bytes(), SampleFormat::S16) - 1.0).abs() < 1e-4);
        assert_eq!(normalize(&i16::MIN.to_le_bytes(), SampleFormat::S16), -1.0);
        assert!((normalize(&i32::MAX.to_le_bytes(), SampleFormat::S32) - 1.0).abs() < 1e-6);
        assert_eq!(normalize(&1.0f32.to_le_bytes(), SampleFormat::F32), 1.0);
        // The packed 24 bits go in three bytes: the top is 0x7FFFFF.
        assert!((normalize(&[0xFF, 0xFF, 0x7F], SampleFormat::S24Packed) - 1.0).abs() < 1e-4);
    }
}
