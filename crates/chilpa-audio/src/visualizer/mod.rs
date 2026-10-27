//! What the immersive view draws from: the samples the device accepted,
//! analyzed thirty times a second.
//!
//! The producer reads the tap ([`Tap::snapshot`]) and hands typed frames to
//! a [`Sink`]: sixteen log-spaced bars, five energy bands, a transient when
//! one lands, 256 points per channel of waveform, 512 spectral bands for
//! the ribbon, and the goniometer and oscilloscope while a scope asks for
//! them. The numbers are QBZ's (`qbz-audio/src/visualizer`): the same
//! windows, compressions and smoothing, so what moves here moves as it
//! does there.
//!
//! Two things keep it from costing anything while nothing shows it: the
//! thread parks while it is disabled or the player is paused, waking on its
//! own within 200 ms, and a scope's DSP runs only while its bit is set.
//!
//! The tap retains bounded stereo history while a view is visible and aligns
//! the analysis window with the output delay reported by the backend. The
//! reusable window does not copy the metering snapshot or compute its RMS.

mod analysis;
mod dsd;
mod ribbon;
mod scope;

pub use scope::{GONIOMETER, OSCILLOSCOPE};

use crate::tap::{Snapshot, Tap};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The bars of the spectrum, spread logarithmically from 20 Hz to 20 kHz.
pub const BARS: usize = 16;
/// Interleaved samples per analysis: 2,048 stereo frames.
pub const FFT_SIZE: usize = 4096;
/// How often a frame of everything goes out.
pub const TARGET_FPS: u64 = 30;
/// Sub-bass, bass, mids, presence and air.
pub const ENERGY_BANDS: usize = 5;
/// Waveform points per channel.
pub const WAVE_POINTS: usize = 256;
/// Bands of the spectral ribbon.
pub const SPECTRAL_BANDS: usize = 512;
const SPECTRAL_RATE_HZ: u32 = 58;
const SPECTRAL_SMOOTHING: f32 = 0.30;
/// How long the thread sleeps between looks while disabled or paused.
const IDLE_POLL: Duration = Duration::from_millis(200);

/// One frame of one stream.
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    /// The sixteen bars, 0 to 1.
    Bars([f32; BARS]),
    /// 256 left samples followed by 256 right ones.
    Wave(Box<[f32; WAVE_POINTS * 2]>),
    /// The ribbon's bands, 0 to 1, at its own cadence.
    Spectral(Vec<f32>),
    /// The five energy bands, 0 to 1.
    Energy([f32; ENERGY_BANDS]),
    /// A transient's intensity, sent only when one lands.
    Transient(f32),
    /// 256 side/mid pairs and the correlation of the two channels.
    Goniometer { points: Box<[f32; scope::GONIOMETER_POINTS * 2]>, correlation: f32 },
    /// 512 mono samples locked to the pitch.
    Oscilloscope(Box<[f32; scope::OSCILLOSCOPE_POINTS]>),
}

/// Whoever receives the frames. The window latches them for its drain.
pub trait Sink: Send + Sync {
    fn submit(&self, frame: Frame);
    /// I publish one completed analysis batch after its last stream arrives.
    fn finish_frame(&self) {}
}

/// What the window tells the producer: whether anybody is looking, whether
/// the player is paused, and which scopes are on screen.
#[derive(Clone, Default)]
pub struct Controls {
    enabled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    scopes: Arc<AtomicU32>,
    compact_only: Arc<AtomicBool>,
    compact_mode: Arc<AtomicU32>,
}

impl Controls {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// While paused the tap gets no new samples: analyzing it again would
    /// burn CPU on the same picture.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// I skip unused analysis and frame allocation for a compact-only consumer.
    pub fn set_compact(&self, only: bool, mode: u32) {
        self.compact_mode.store(mode, Ordering::Relaxed);
        self.compact_only.store(only, Ordering::Relaxed);
    }

    pub fn scopes(&self) -> u32 {
        self.scopes.load(Ordering::Relaxed)
    }

    /// [`GONIOMETER`] and [`OSCILLOSCOPE`] bits; zero keeps their DSP idle.
    pub fn set_scopes(&self, mask: u32) {
        self.scopes.store(mask, Ordering::Relaxed);
    }
}

/// Starts the producer. The handle's thread can be unparked when the view
/// opens, for an instant first frame; without that it wakes within 200 ms.
pub fn spawn(tap: Tap, controls: Controls, sink: Arc<dyn Sink>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("chilpa-viz".into())
        .spawn(move || run(tap, controls, sink))
        .expect("the visualizer thread starts")
}

struct Producer {
    samples: Vec<f32>,
    analyzer: analysis::Analyzer,
    ribbon: ribbon::SpectralAnalyzer,
    scopes: scope::ScopeProcessor,
    dsd: dsd::Sidechain,
}

impl Producer {
    fn new() -> Self {
        Self {
            samples: vec![0.0f32; FFT_SIZE],
            analyzer: analysis::Analyzer::default(),
            ribbon: ribbon::SpectralAnalyzer::new(44_100, FFT_SIZE, SPECTRAL_BANDS, SPECTRAL_RATE_HZ, SPECTRAL_SMOOTHING),
            scopes: scope::ScopeProcessor::default(),
            dsd: dsd::Sidechain::default(),
        }
    }

    fn produce(&mut self, tap: &Tap, controls: &Controls, sink: &dyn Sink) {
        if !controls.enabled() || controls.paused() {
            self.dsd.reset();
            return;
        }
        let update = self.dsd.poll(tap);
        if update.reset {
            self.analyzer = analysis::Analyzer::default();
            self.ribbon = ribbon::SpectralAnalyzer::new(44_100, FFT_SIZE, SPECTRAL_BANDS, SPECTRAL_RATE_HZ, SPECTRAL_SMOOTHING);
            self.scopes = scope::ScopeProcessor::default();
        }
        let rate = if update.active {
            let Some(snapshot) = update.snapshot else { return };
            if !fill(&snapshot, &mut self.samples) { return; }
            snapshot.sample_rate_hz
        } else {
            tap.visualizer_snapshot(&mut self.samples)
        };
        if rate == 0 { return; }
        let samples = &self.samples;
        let mask = controls.scopes();
        if mask & GONIOMETER != 0 {
            let (points, correlation) = self.scopes.goniometer(samples, rate);
            sink.submit(Frame::Goniometer { points, correlation });
        }
        if mask & OSCILLOSCOPE != 0 {
            sink.submit(Frame::Oscilloscope(self.scopes.oscilloscope(samples, rate)));
        }
        let compact = controls.compact_only.load(Ordering::Relaxed);
        let mode = controls.compact_mode.load(Ordering::Relaxed);
        if !compact && self.ribbon.process(samples, rate) {
            sink.submit(Frame::Spectral(self.ribbon.bands().to_vec()));
        }
        if !compact || matches!(mode, 0 | 2 | 5 | 6) {
            let analysis::Analyzed { bars, energy, transient } = self.analyzer.analyze(samples, rate);
            if !compact || mode != 2 { sink.submit(Frame::Bars(bars)); }
            if !compact || mode == 2 { sink.submit(Frame::Energy(energy)); }
            if !compact {
                if let Some(intensity) = transient { sink.submit(Frame::Transient(intensity)); }
            }
        }
        if !compact || mode == 1 { sink.submit(Frame::Wave(wave(samples))); }
        sink.finish_frame();
    }
}

fn run(tap: Tap, controls: Controls, sink: Arc<dyn Sink>) {
    let mut producer = Producer::new();
    let period = Duration::from_micros(1_000_000 / TARGET_FPS);
    loop {
        if !controls.enabled() || controls.paused() {
            producer.dsd.reset();
            std::thread::park_timeout(IDLE_POLL);
            continue;
        }
        let started = Instant::now();
        producer.produce(&tap, &controls, sink.as_ref());
        let elapsed = started.elapsed();
        if elapsed < period {
            std::thread::sleep(period - elapsed);
        }
    }
}

/// The last [`FFT_SIZE`] interleaved stereo samples of the snapshot. Mono is
/// heard in both ears; of more channels, the first two are drawn. False with
/// nothing to draw.
fn fill(snapshot: &Snapshot, out: &mut [f32]) -> bool {
    let channels = snapshot.channels.max(1) as usize;
    let frames_kept = snapshot.samples.len() / channels;
    if frames_kept == 0 {
        return false;
    }
    let frames = out.len() / 2;
    out.fill(0.0);
    // Fewer frames than asked for are drawn at the end, after silence: the
    // latest is what is heard.
    let taken = frames.min(frames_kept);
    let first = frames_kept - taken;
    let into = frames - taken;
    for i in 0..taken {
        let frame = &snapshot.samples[(first + i) * channels..(first + i + 1) * channels];
        let left = frame[0];
        let right = if channels > 1 { frame[1] } else { left };
        out[(into + i) * 2] = left;
        out[(into + i) * 2 + 1] = right;
    }
    true
}

/// Every other frame of each channel: left first, then right.
fn wave(samples: &[f32]) -> Box<[f32; WAVE_POINTS * 2]> {
    let frames = samples.len() / 2;
    let step = (frames / WAVE_POINTS).max(1);
    let mut out = Box::new([0.0f32; WAVE_POINTS * 2]);
    for i in 0..WAVE_POINTS {
        let base = i * step * 2;
        if base + 1 < samples.len() {
            out[i] = samples[base];
            out[WAVE_POINTS + i] = samples[base + 1];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(channels: u16, frames: usize) -> Snapshot {
        let mut samples = Vec::with_capacity(frames * channels as usize);
        for f in 0..frames {
            for c in 0..channels {
                samples.push(f as f32 + c as f32 * 0.1);
            }
        }
        Snapshot { samples, channels, sample_rate_hz: 48_000, peaks: vec![], rms: vec![] }
    }

    #[test]
    fn the_latest_stereo_frames_are_taken_and_fewer_come_after_silence() {
        let mut out = vec![0.0f32; 8];
        assert!(fill(&snapshot(2, 10), &mut out));
        assert_eq!(out, [6.0, 6.1, 7.0, 7.1, 8.0, 8.1, 9.0, 9.1]);
        assert!(fill(&snapshot(2, 2), &mut out));
        assert_eq!(out, [0.0, 0.0, 0.0, 0.0, 0.0, 0.1, 1.0, 1.1]);
    }

    #[test]
    fn mono_is_heard_in_both_ears_and_extra_channels_are_left_out() {
        let mut out = vec![0.0f32; 4];
        assert!(fill(&snapshot(1, 3), &mut out));
        assert_eq!(out, [1.0, 1.0, 2.0, 2.0]);
        assert!(fill(&snapshot(6, 2), &mut out));
        assert_eq!(out, [0.0, 0.1, 1.0, 1.1]);
        assert!(!fill(&snapshot(2, 0), &mut out), "an empty tap draws nothing");
    }

    #[test]
    fn the_wave_takes_every_other_frame_left_then_right() {
        let samples: Vec<f32> = (0..FFT_SIZE).map(|i| i as f32).collect();
        let wave = wave(&samples);
        // 2,048 frames into 256 points: every eighth frame, 16 samples on.
        assert_eq!(wave[0], 0.0);
        assert_eq!(wave[1], 16.0, "the second point is the eighth frame");
        assert_eq!(wave[WAVE_POINTS], 1.0, "the right channel follows the left");
        assert_eq!(wave[WAVE_POINTS + 255], (255 * 16 + 1) as f32);
    }

    #[test]
    fn the_controls_start_off_and_not_paused() {
        let controls = Controls::new();
        assert!(!controls.enabled());
        assert!(!controls.paused());
        assert_eq!(controls.scopes(), 0);
        controls.set_enabled(true);
        controls.set_scopes(GONIOMETER | OSCILLOSCOPE);
        assert!(controls.enabled());
        assert_eq!(controls.scopes(), 0b11);
    }
}
