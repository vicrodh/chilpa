use super::{Preset, Control, biquad::Biquad, dynamics};
use crate::format::StreamFormat;
use std::sync::{Arc, atomic::Ordering};

pub(super) struct Bank {
    preset: Arc<Preset>,
    filters: Vec<Vec<Biquad>>,
    gain: f64,
    gain_db: f64,
    cross: [f64; 2],
    cross_alpha: f64,
    delay: [Vec<f64>; 2],
    cursor: [usize; 2],
    dynamics: dynamics::Dynamics,
    limiter: Option<dynamics::Limiter>,
    compression_db: f64,
}
impl Bank {
    pub(super) fn new(preset: Arc<Preset>, format: StreamFormat) -> Self {
        let rate = f64::from(format.sample_rate_hz);
        let filters = (0..format.channels).map(|channel| preset.routed_filters(usize::from(channel)).iter().map(|f| Biquad::new(f, format.sample_rate_hz)).collect()).collect();
        let delay = [preset.delay_left_ms, preset.delay_right_ms].map(|ms| vec![0.0; (rate * ms / 1000.0).round() as usize]);
        let dynamics = dynamics::Dynamics::new(&preset, format.sample_rate_hz, usize::from(format.channels));
        let limiter = dynamics::Limiter::new(&preset, format.sample_rate_hz, usize::from(format.channels));
        let gain_db = preset.preamp_for_channels(format.sample_rate_hz, format.channels);
        Self { gain: 10_f64.powf(gain_db / 20.0), gain_db, filters,
            preset, cross: [0.0; 2], cross_alpha: 1.0 - (-2.0 * std::f64::consts::PI * 700.0 / rate).exp(),
            delay, cursor: [0; 2], dynamics, limiter, compression_db: 0.0 }
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
            if p.crossfeed != 0.0 || p.width != 1.0 || p.balance != 0.0 || p.invert_left || p.invert_right {
            for (state, sample) in self.cross.iter_mut().zip(output.iter()) { *state += self.cross_alpha * (*sample - *state); }
            let left = (output[0] + p.crossfeed * self.cross[1]) / (1.0 + p.crossfeed);
            let right = (output[1] + p.crossfeed * self.cross[0]) / (1.0 + p.crossfeed);
            let mid = (left + right) * 0.5;
            let side = (left - right) * p.width * 0.5;
            output[0] = (mid + side) * (1.0 - p.balance.max(0.0)) * if p.invert_left { -1.0 } else { 1.0 };
            output[1] = (mid - side) * (1.0 + p.balance.min(0.0)) * if p.invert_right { -1.0 } else { 1.0 };
            }
            for (channel, sample) in output.iter_mut().enumerate() {
                if !self.delay[channel].is_empty() {
                    let held = self.delay[channel][self.cursor[channel]];
                    self.delay[channel][self.cursor[channel]] = *sample;
                    self.cursor[channel] = (self.cursor[channel] + 1) % self.delay[channel].len();
                    *sample = held;
                }
            }
        }
        self.compression_db = self.dynamics.frame(&self.preset, output);

    }
    fn inherit(&mut self, old: &Self) {
        for (filters, previous) in self.filters.iter_mut().zip(&old.filters) {
            for (filter, previous) in filters.iter_mut().zip(previous) { filter.inherit(previous); }
        }
        for channel in 0..2 {
            if self.delay[channel].len() == old.delay[channel].len() {
                self.delay[channel].copy_from_slice(&old.delay[channel]);
                self.cursor[channel] = old.cursor[channel];
            }
        }
        self.cross = old.cross;
        self.dynamics.inherit(&old.dynamics, self.preset.dynamics.sidechain_hpf_hz == old.preset.dynamics.sidechain_hpf_hz);
        if let (Some(current), Some(previous)) = (&mut self.limiter, &old.limiter) { current.inherit(previous); }
    }
    fn limit(&mut self, samples: &mut [f64]) -> f64 { self.limiter.as_mut().map_or(0.0, |limiter| limiter.frame(samples)) }

}

/// One renderer owns its histories. Bypass preserves original packed bytes.
pub struct Processor {
    control: Control,
    format: StreamFormat,
    revision: u64,
    stream: u64,
    current: Option<Bank>,
    previous: Option<Bank>,
    transition: usize,
    transition_length: usize,
}
impl Processor {
    pub fn new(control: Control, format: StreamFormat) -> Self {
        control.begin_stream();
        let stream = control.register(format);
        Self { control, format, revision: u64::MAX, stream, current: None, previous: None,
            transition: 0, transition_length: (format.sample_rate_hz as usize / 100).max(1) }
    }
    fn update(&mut self) {
        let revision = self.control.0.revision.load(Ordering::Acquire);
        if revision == self.revision { return; }
        let Ok(mut mailbox) = self.control.0.mailbox.try_lock() else { return; };
        if mailbox.stream != self.stream || mailbox.retired.is_some() { return; }
        let Some(mut pending) = mailbox.pending.take() else { return; };
        if let (Some(current), Some(next)) = (&self.current, &mut pending) { next.inherit(current); }
        if let Some(next) = &pending { self.control.0.headroom.store(next.gain_db.to_bits(), Ordering::Relaxed); }
        mailbox.retired = self.previous.take();
        self.previous = self.current.take();
        self.current = pending;
        self.transition = self.transition_length;
        self.revision = revision;
    }
    pub fn reset(&mut self) {
        self.previous = None;
        self.current = None;
        self.stream = self.control.register(self.format);
        self.revision = u64::MAX;
        self.transition = 0;
    }
    pub fn process(&mut self, bytes: &mut [u8]) {
        let channels = usize::from(self.format.channels);
        if self.format.sample_format.is_dsd() || channels == 0 || channels > 32 || self.format.sample_rate_hz < 1000 { return; }
        self.update();
        if self.current.is_none() && (self.previous.is_none() || self.transition == 0) {
            self.transition = 0;
            self.retire_previous();
            let _ = self.control.0.active_stream.compare_exchange(self.stream, 0, Ordering::AcqRel, Ordering::Acquire);
            return;
        }
        if self.control.0.stream.load(Ordering::Acquire) == self.stream {
            self.control.0.touched.store(true, Ordering::Release);
            self.control.0.active_stream.store(self.stream, Ordering::Release);
        }
        let width = self.format.sample_format.bytes();
        let mut clipped = 0;
        let (mut input_peak, mut output_peak, mut compression, mut limiting) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        for frame in bytes.chunks_exact_mut(width * channels) {
            let mut input = [0.0; 32];
            let mut output = [0.0; 32];
            let mut old = [0.0; 32];
            for (sample, value) in frame.chunks_exact(width).zip(input.iter_mut()) { *value = super::wire::read(sample, self.format.sample_format); }
            let input = &input[..channels];
            let output = &mut output[..channels];
            let old = &mut old[..channels];
            input_peak = input.iter().map(|sample| sample.abs()).fold(input_peak, f64::max);
            if let Some(bank) = &mut self.current { bank.frame(input, output);
                compression = compression.max(bank.compression_db); } else { output.copy_from_slice(input); }
            let limited = self.current.as_ref().is_some_and(|bank| bank.limiter.is_some());
            if self.transition > 0 {
                if let Some(bank) = &mut self.previous { bank.frame(input, old); if !limited { limiting = limiting.max(bank.limit(old)); } } else { old.copy_from_slice(input); }
                let mix = 1.0 - self.transition as f64 / self.transition_length as f64;
                for (y, x) in output.iter_mut().zip(old.iter()) { *y = *x + (*y - *x) * mix; }
                self.transition -= 1;

            }
            if let Some(bank) = &mut self.current { limiting = limiting.max(bank.limit(output)); }
            for (sample, value) in frame.chunks_exact_mut(width).zip(output.iter()) {
                output_peak = output_peak.max(value.abs());
                if value.abs() > 1.0 { clipped += 1; }
                super::wire::write(sample, self.format.sample_format, *value);
            }
        }
        self.control.0.clipped.fetch_add(clipped, Ordering::Relaxed);
        for (counter, value) in [(&self.control.0.input_peak, input_peak), (&self.control.0.output_peak, output_peak),
            (&self.control.0.compression, compression), (&self.control.0.limiting, limiting)] {
            counter.fetch_max(if value > 0.0 { value.to_bits() } else { 0 }, Ordering::Relaxed);
        }
        if self.transition == 0 { self.retire_previous(); }
    }
    fn retire_previous(&mut self) {
        if self.previous.is_none() { return; }
        if let Ok(mut mailbox) = self.control.0.mailbox.try_lock() {
            if mailbox.retired.is_none() { mailbox.retired = self.previous.take(); }
        }
    }
}
impl Drop for Processor {
    fn drop(&mut self) { self.control.unregister(self.stream); }
}
