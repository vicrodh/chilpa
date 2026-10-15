use super::{Preset, biquad::Biquad};
use crate::format::StreamFormat;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};

#[derive(Default)]
struct Shared {
    preset: Mutex<Option<Arc<Preset>>>,
    revision: AtomicU64,
    touched: AtomicBool,
    active: AtomicBool,
    clipped: AtomicU64,
}

/// The control thread validates before publishing. The renderer only tries
/// the lock when a revision changes; it never waits for a UI or plugin call.
#[derive(Clone, Default)]
pub struct Control(Arc<Shared>);
impl Control {
    pub fn begin_stream(&self) {
        self.0.touched.store(false, Ordering::Release);
        self.0.active.store(false, Ordering::Release);
    }
    pub fn set(&self, preset: Option<Preset>) -> Result<(), String> {
        if let Some(preset) = &preset { preset.validate()?; }
        let mut current = self.0.preset.lock().map_err(|_| chilpa_i18n::t("DSP control is unavailable."))?;
        *current = preset.map(Arc::new);
        self.0.revision.fetch_add(1, Ordering::Release);
        Ok(())
    }
    pub fn touched(&self) -> bool { self.0.touched.load(Ordering::Acquire) }
    pub fn active(&self) -> bool { self.0.active.load(Ordering::Acquire) }
    pub fn clipped_samples(&self) -> u64 { self.0.clipped.load(Ordering::Relaxed) }
    pub fn reset_meter(&self) { self.0.clipped.store(0, Ordering::Relaxed); }
    pub fn requested(&self) -> bool {
        self.0.preset.lock().map(|p| p.as_ref().is_some_and(|p| !p.is_identity())).unwrap_or(false)
    }
}

struct Bank {
    preset: Arc<Preset>,
    filters: Vec<Vec<Biquad>>,
    gain: f64,
    cross: [f64; 2],
    cross_alpha: f64,
    delay: [Vec<f64>; 2],
    cursor: [usize; 2],
    envelope: f64,
    attack: f64,
    release: f64,
}
impl Bank {
    fn new(preset: Arc<Preset>, format: StreamFormat) -> Self {
        let rate = f64::from(format.sample_rate_hz);
        let filters = (0..format.channels).map(|_| preset.filters.iter().map(|f| Biquad::new(f, format.sample_rate_hz)).collect()).collect();
        let delay = [preset.delay_left_ms, preset.delay_right_ms].map(|ms| vec![0.0; (rate * ms / 1000.0).round() as usize]);
        Self { gain: 10_f64.powf(preset.preamp_for_rate(format.sample_rate_hz) / 20.0), filters,
            preset, cross: [0.0; 2], cross_alpha: 1.0 - (-2.0 * std::f64::consts::PI * 700.0 / rate).exp(),
            delay, cursor: [0; 2], envelope: 0.0, attack: (-1.0 / (rate * 0.005)).exp(), release: (-1.0 / (rate * 0.180)).exp() }
    }

    fn frame(&mut self, input: &[f64], output: &mut [f64]) {
        for (channel, (x, y)) in input.iter().zip(output.iter_mut()).enumerate() {
            *y = *x * self.gain;
            for filter in &mut self.filters[channel] { *y = filter.tick(*y); }
        }
        // Stereo tools leave surround channels unmapped and untouched by
        // the matrix. EQ and linked dynamics still apply to every channel.
        if output.len() == 2 {
            let p = &self.preset;
            for (state, sample) in self.cross.iter_mut().zip(output.iter()) { *state += self.cross_alpha * (*sample - *state); }
            let left = (output[0] + p.crossfeed * self.cross[1]) / (1.0 + p.crossfeed);
            let right = (output[1] + p.crossfeed * self.cross[0]) / (1.0 + p.crossfeed);
            let mid = (left + right) * 0.5;
            let side = (left - right) * p.width * 0.5;
            output[0] = (mid + side) * (1.0 - p.balance.max(0.0)) * if p.invert_left { -1.0 } else { 1.0 };
            output[1] = (mid - side) * (1.0 + p.balance.min(0.0)) * if p.invert_right { -1.0 } else { 1.0 };
            for (channel, sample) in output.iter_mut().enumerate() {
                if !self.delay[channel].is_empty() {
                    let held = self.delay[channel][self.cursor[channel]];
                    self.delay[channel][self.cursor[channel]] = *sample;
                    self.cursor[channel] = (self.cursor[channel] + 1) % self.delay[channel].len();
                    *sample = held;
                }
            }
        }
        if self.preset.compressor_ratio > 1.0 {
            let peak = output.iter().map(|x| x.abs()).fold(0.0, f64::max);
            let speed = if peak > self.envelope { self.attack } else { self.release };
            self.envelope = speed * self.envelope + (1.0 - speed) * peak;
            let over = (20.0 * self.envelope.max(1e-15).log10() - self.preset.compressor_threshold_db).max(0.0);
            let reduction = 10_f64.powf(-over * (1.0 - 1.0 / self.preset.compressor_ratio) / 20.0);
            for sample in output { *sample *= reduction; }
        }
    }
}

/// One renderer owns its histories. Bypass preserves original packed bytes.
pub struct Processor {
    control: Control,
    format: StreamFormat,
    revision: u64,
    current: Option<Bank>,
    previous: Option<Bank>,
    transition: usize,
    transition_length: usize,
}
impl Processor {
    pub fn new(control: Control, format: StreamFormat) -> Self {
        control.begin_stream();
        Self { control, format, revision: u64::MAX, current: None, previous: None,
            transition: 0, transition_length: (format.sample_rate_hz as usize / 100).max(1) }
    }
    fn update(&mut self) {
        let revision = self.control.0.revision.load(Ordering::Acquire);
        if revision == self.revision { return; }
        let preset = match self.control.0.preset.try_lock() { Ok(preset) => preset.clone(), Err(_) => return };
        self.previous = self.current.take();
        self.current = preset.filter(|p| !p.is_identity()).map(|p| Bank::new(p, self.format));
        self.transition = self.transition_length;
        self.revision = revision;
    }
    pub fn reset(&mut self) {
        self.previous = None;
        self.current = None;
        self.revision = u64::MAX;
        self.transition = 0;
    }
    pub fn process(&mut self, bytes: &mut [u8]) {
        let channels = usize::from(self.format.channels);
        if self.format.sample_format.is_dsd() || channels == 0 || channels > 32 || self.format.sample_rate_hz < 1000 { return; }
        self.update();
        if self.current.is_none() && self.previous.is_none() {
            self.transition = 0;
            self.control.0.active.store(false, Ordering::Release);
            return;
        }
        self.control.0.touched.store(true, Ordering::Release);
        self.control.0.active.store(true, Ordering::Release);
        let width = self.format.sample_format.bytes();
        let mut clipped = 0;
        for frame in bytes.chunks_exact_mut(width * channels) {
            let mut input = [0.0; 32];
            let mut output = [0.0; 32];
            let mut old = [0.0; 32];
            for (sample, value) in frame.chunks_exact(width).zip(input.iter_mut()) { *value = super::wire::read(sample, self.format.sample_format); }
            let input = &input[..channels];
            let output = &mut output[..channels];
            let old = &mut old[..channels];
            if let Some(bank) = &mut self.current { bank.frame(input, output); } else { output.copy_from_slice(input); }
            if self.transition > 0 {
                if let Some(bank) = &mut self.previous { bank.frame(input, old); } else { old.copy_from_slice(input); }
                let mix = 1.0 - self.transition as f64 / self.transition_length as f64;
                for (y, x) in output.iter_mut().zip(old.iter()) { *y = *x + (*y - *x) * mix; }
                self.transition -= 1;
                if self.transition == 0 { self.previous = None; }
            }
            for (sample, value) in frame.chunks_exact_mut(width).zip(output.iter()) {
                if value.abs() > 1.0 { clipped += 1; }
                super::wire::write(sample, self.format.sample_format, *value);
            }
        }
        self.control.0.clipped.fetch_add(clipped, Ordering::Relaxed);
    }
}
impl Drop for Processor {
    fn drop(&mut self) { self.control.0.active.store(false, Ordering::Release); }
}
