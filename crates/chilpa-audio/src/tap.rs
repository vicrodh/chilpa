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

/// How many frames are kept to look at. At 192 kHz that is about 43 ms,
/// which is more than fits in a screen frame.
pub const FRAMES_KEPT: usize = 8192;

/// The last thing that went out, for whoever wants to draw it.
#[derive(Clone, Default)]
pub struct Tap(Arc<Mutex<State>>);

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
    fn prepare(&self, format: StreamFormat) {
        if let Ok(mut e) = self.0.lock() {
            let channels = format.channels.max(1);
            e.samples = vec![0.0; FRAMES_KEPT * channels as usize];
            e.cursor = 0;
            e.channels = channels;
            e.sample_rate_hz = format.sample_rate_hz;
            e.frames = 0;
        }
    }

    /// Copies what was delivered. If the lock is busy, it does nothing.
    fn copy(&self, bytes: &[u8], format: StreamFormat) {
        let Ok(mut e) = self.0.try_lock() else { return };
        if e.samples.is_empty() {
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
        for sample in bytes.chunks_exact(width) {
            e.samples[cursor] = normalize(sample, format.sample_format);
            cursor = (cursor + 1) % total;
            written += 1;
        }
        e.cursor = cursor;
        e.frames += (written / channels) as u64;
    }

    /// The last thing that went out, in chronological order.
    pub fn snapshot(&self) -> Snapshot {
        let Ok(e) = self.0.lock() else { return Snapshot::default() };
        if e.samples.is_empty() {
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

    /// How many frames have been copied.
    pub fn frames(&self) -> u64 {
        self.0.lock().map(|e| e.frames).unwrap_or(0)
    }

    /// Wraps a device to watch what is delivered to it.
    pub fn wrap<S: AudioSink>(&self, sink: S) -> TapSink<S> {
        let format = sink.format();
        self.prepare(format);
        TapSink { inner: sink, tap: self.clone(), format }
    }
}

/// A wrapped device: it copies what is delivered and passes it through.
pub struct TapSink<S: AudioSink> {
    inner: S,
    tap: Tap,
    format: StreamFormat,
}

impl<S: AudioSink> AudioSink for TapSink<S> {
    fn format(&self) -> StreamFormat {
        self.inner.format()
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        // Write first, then copy what the device accepted: copying more would
        // show samples that never played.
        let accepted = self.inner.write(frames)?;
        let bytes = accepted * self.format.frame_bytes();
        if let Some(delivered) = frames.get(..bytes) {
            self.tap.copy(delivered, self.format);
        }
        Ok(accepted)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        self.inner.close()
    }

    fn handoff(&mut self) -> Result<(), SinkError> {
        self.inner.handoff()
    }

    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        self.inner.drain(give_up)
    }

    fn flush(&mut self) -> Result<(), SinkError> {
        self.inner.flush()
    }

    fn held(&self) -> u64 {
        self.inner.held()
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        self.inner.set_paused(paused)
    }

    fn suspend(&mut self) -> Result<(), SinkError> {
        self.inner.suspend()
    }

    fn wake(&mut self) -> Result<(), SinkError> {
        self.inner.wake()
    }
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
