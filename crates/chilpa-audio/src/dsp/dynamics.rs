//! Linked peak dynamics and a separate zero-latency sample-peak limiter.
use super::{Preset, biquad::Biquad, Filter, Shape};

pub(super) struct Dynamics {
    envelope: f64,
    attack: f64,
    release: f64,
    pre_gain: f64,
    detector: Vec<Biquad>,
    gate_gain: f64,
}
impl Dynamics {
    pub(super) fn new(preset: &Preset, rate: u32, channels: usize) -> Self {
        let config = &preset.dynamics;
        let detector = (0..channels).map(|_| if config.sidechain_hpf_hz == 0.0 { Biquad::identity() }
            else { Biquad::new(&Filter { shape: Shape::HighPass, frequency: config.sidechain_hpf_hz,
                ..Filter::default() }, rate) }).collect();
        Self { envelope: 0.0, attack: (-1000.0 / (f64::from(rate) * config.attack_ms)).exp(),
            release: (-1000.0 / (f64::from(rate) * config.release_ms)).exp(),
            pre_gain: 10_f64.powf(config.pre_gain_db / 20.0), detector, gate_gain: 1.0 }
    }
    pub(super) fn inherit(&mut self, old: &Self, same_detector: bool) {
        self.envelope = old.envelope; self.gate_gain = old.gate_gain;
        if same_detector && self.detector.len() == old.detector.len() {
            for (filter, previous) in self.detector.iter_mut().zip(&old.detector) { filter.inherit(previous); }
        }
    }
    pub(super) fn frame(&mut self, preset: &Preset, samples: &mut [f64]) -> f64 {
        if !preset.dynamics.active(preset.compressor_ratio) { return 0.0; }
        let mut peak = 0.0_f64;
        for (sample, detector) in samples.iter_mut().zip(&mut self.detector) {
            *sample *= self.pre_gain;
            peak = peak.max(detector.tick(*sample).abs());
        }
        let speed = if peak > self.envelope { self.attack } else { self.release };
        self.envelope = speed * self.envelope + (1.0 - speed) * peak;
        let level = 20.0 * self.envelope.max(1e-15).log10();
        let config = &preset.dynamics;
        let compression = knee(level - preset.compressor_threshold_db, config.knee_db) * (1.0 - 1.0 / preset.compressor_ratio);
        let expansion = knee(config.gate_threshold_db - level, config.knee_db) * (config.expander_ratio - 1.0);
        let reduction = (compression + expansion).min(120.0);
        let gate = if config.gate_enabled && level < config.gate_threshold_db { 0.0 } else { 1.0 };
        let gate_speed = if gate > self.gate_gain { self.attack } else { self.release };
        self.gate_gain = gate_speed * self.gate_gain + (1.0 - gate_speed) * gate;
        let gain = 10_f64.powf((config.makeup_db - reduction) / 20.0) * self.gate_gain;
        for sample in samples { *sample *= gain; }
        reduction - 20.0 * self.gate_gain.max(1e-6).log10()
    }
}
fn knee(over: f64, width: f64) -> f64 {
    if width == 0.0 || over >= width * 0.5 { over.max(0.0) }
    else if over <= -width * 0.5 { 0.0 }
    else { (over + width * 0.5).powi(2) / (2.0 * width) }
}

pub(super) struct Limiter { ceiling: f64, release: f64, gain: f64 }
impl Limiter {
    pub(super) fn new(preset: &Preset, rate: u32, _channels: usize) -> Option<Self> {
        let config = &preset.limiter;
        config.enabled.then(|| Self { ceiling: 10_f64.powf(config.ceiling_db / 20.0),
            release: (-1000.0 / (f64::from(rate) * config.release_ms)).exp(), gain: 1.0 })
    }
    pub(super) fn inherit(&mut self, old: &Self) { self.gain = old.gain; }
    pub(super) fn frame(&mut self, samples: &mut [f64]) -> f64 {
        let peak = samples.iter().map(|sample| sample.abs()).fold(0.0, f64::max);
        let desired = (self.ceiling / peak.max(1e-30)).min(1.0);
        self.gain = if desired < self.gain { desired } else { self.release * self.gain + (1.0 - self.release) * desired };
        for sample in samples { *sample *= self.gain; }
        if self.gain < 1.0 { -20.0 * self.gain.max(1e-15).log10() } else { 0.0 }
    }
}
