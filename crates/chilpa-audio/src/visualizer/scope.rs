//! The goniometer and the oscilloscope, from the same samples the bars
//! use. Port of `qbz-audio/src/visualizer/scope.rs`; they run only while a
//! panel asks for them, since the oscilloscope's FIR is not cheap.

use super::analysis::{hann, spectrum_of};
use std::f32::consts::{FRAC_1_SQRT_2, PI, TAU};

/// The bits of [`super::Controls::set_scopes`].
pub const GONIOMETER: u32 = 0b01;
pub const OSCILLOSCOPE: u32 = 0b10;
pub const GONIOMETER_POINTS: usize = 256;
pub const OSCILLOSCOPE_POINTS: usize = 512;

const PITCH_MIN_HZ: f32 = 40.0;
const PITCH_MAX_HZ: f32 = 1_000.0;
const FIR_TAPS: usize = 257;

/// A second-order low-pass, Butterworth.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn low_pass(sample_rate: u32, corner_hz: f32) -> Self {
        let rate = sample_rate.max(1) as f32;
        let corner = corner_hz.clamp(1.0, rate * 0.49);
        let omega = TAU * corner / rate;
        let (sin, cos) = (omega.sin(), omega.cos());
        let alpha = sin * FRAC_1_SQRT_2;
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cos) * 0.5) / a0,
            b1: (1.0 - cos) / a0,
            b2: ((1.0 - cos) * 0.5) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, sample: f32) -> f32 {
        let out = self.b0 * sample + self.z1;
        self.z1 = self.b1 * sample - self.a1 * out + self.z2;
        self.z2 = self.b2 * sample - self.a2 * out;
        out
    }
}

#[derive(Default)]
pub struct ScopeProcessor {
    raw: Vec<f32>,
    conditioned: Vec<f32>,
    trigger: Vec<f32>,
    draw: Vec<f32>,
    fir: Vec<f32>,
    pitch_hz: f32,
    fir_center_hz: f32,
    fir_sample_rate: u32,
}

impl ScopeProcessor {
    /// Side/mid points after two cascaded 8 kHz low-pass stages, and the
    /// correlation of the two channels over the whole window.
    pub fn goniometer(&mut self, interleaved: &[f32], sample_rate: u32) -> (Box<[f32; GONIOMETER_POINTS * 2]>, f32) {
        let frames = interleaved.len() / 2;
        let mut points = Box::new([0.0; GONIOMETER_POINTS * 2]);
        if frames == 0 {
            return (points, 0.0);
        }
        let mut ll1 = Biquad::low_pass(sample_rate, 8_000.0);
        let mut ll2 = Biquad::low_pass(sample_rate, 8_000.0);
        let mut rr1 = Biquad::low_pass(sample_rate, 8_000.0);
        let mut rr2 = Biquad::low_pass(sample_rate, 8_000.0);
        let step = (frames / GONIOMETER_POINTS).max(1);
        let mut next = frames.saturating_sub(step * GONIOMETER_POINTS);
        let mut point = 0usize;
        let (mut sum_cross, mut sum_left, mut sum_right) = (0.0f64, 0.0f64, 0.0f64);
        for frame in 0..frames {
            let left = interleaved[frame * 2];
            let right = interleaved[frame * 2 + 1];
            sum_cross += (left * right) as f64;
            sum_left += (left * left) as f64;
            sum_right += (right * right) as f64;
            let left = ll2.process(ll1.process(left));
            let right = rr2.process(rr1.process(right));
            if frame >= next && point < GONIOMETER_POINTS {
                points[point * 2] = ((right - left) * FRAC_1_SQRT_2).clamp(-1.0, 1.0);
                points[point * 2 + 1] = ((right + left) * FRAC_1_SQRT_2).clamp(-1.0, 1.0);
                point += 1;
                next = next.saturating_add(step);
            }
        }
        let denominator = (sum_left * sum_right).sqrt();
        let correlation = if denominator > f64::EPSILON { (sum_cross / denominator).clamp(-1.0, 1.0) as f32 } else { 0.0 };
        (points, correlation)
    }

    /// Mono, locked to the pitch: the pitch is estimated on one conditioned
    /// copy, the trigger found on a narrow band-pass of it, and the drawing
    /// taken from a gentler low-pass. The FIR is redesigned only after the
    /// pitch moves by more than 5 %.
    pub fn oscilloscope(&mut self, interleaved: &[f32], sample_rate: u32) -> Box<[f32; OSCILLOSCOPE_POINTS]> {
        let frames = interleaved.len() / 2;
        let mut output = Box::new([0.0; OSCILLOSCOPE_POINTS]);
        if frames < 64 {
            return output;
        }
        self.resize(frames);
        for frame in 0..frames {
            self.raw[frame] = (interleaved[frame * 2] + interleaved[frame * 2 + 1]) * 0.5;
        }
        self.condition_for_pitch(sample_rate);
        if let Some(target) = estimate_pitch(&self.conditioned, sample_rate) {
            self.pitch_hz = if self.pitch_hz <= 0.0 || (target / self.pitch_hz).max(self.pitch_hz / target) > 1.8 {
                target
            } else {
                self.pitch_hz * 0.82 + target * 0.18
            };
        }
        let pitch = self.pitch_hz.clamp(PITCH_MIN_HZ, PITCH_MAX_HZ);
        if self.fir.is_empty() || self.fir_sample_rate != sample_rate || relative_change(pitch, self.fir_center_hz) > 0.05 {
            self.fir = design_bandpass(pitch, sample_rate, FIR_TAPS);
            self.fir_center_hz = pitch;
            self.fir_sample_rate = sample_rate;
        }
        convolve(&self.conditioned, &self.fir, &mut self.trigger);
        low_pass_draw(&self.raw, sample_rate, &mut self.draw);

        let period = (sample_rate.max(1) as f32 / pitch).clamp(2.0, frames as f32);
        let span = (period * 2.0).clamp(96.0, frames.saturating_sub(4) as f32);
        let required = span.ceil() as usize + 2;
        let latest = frames.saturating_sub(required).max(1);
        let trigger_rms = rms(&self.trigger);
        let quarter = (period * 0.25).round().max(1.0) as usize;
        let threshold = (trigger_rms * 0.18).max(0.0005);
        let trigger = rising_trigger(&self.trigger, latest, quarter, threshold).unwrap_or(frames.saturating_sub(required) as f32);
        let gain = (0.22 / rms(&self.draw).max(0.015)).clamp(0.5, 6.0);
        for (index, sample) in output.iter_mut().enumerate() {
            let position = trigger + span * index as f32 / (OSCILLOSCOPE_POINTS - 1) as f32;
            *sample = (interpolate(&self.draw, position) * gain).clamp(-1.0, 1.0);
        }
        output
    }

    fn resize(&mut self, frames: usize) {
        self.raw.resize(frames, 0.0);
        self.conditioned.resize(frames, 0.0);
        self.trigger.resize(frames, 0.0);
        self.draw.resize(frames, 0.0);
    }

    /// A low shelf at 400 Hz lifted against the rest, then two 18 kHz
    /// low-pass stages.
    fn condition_for_pitch(&mut self, sample_rate: u32) {
        let rate = sample_rate.max(1) as f32;
        let low_alpha = (-TAU * 400.0 / rate).exp();
        let mut shelf_low = 0.0f32;
        let mut low_a = Biquad::low_pass(sample_rate, 18_000.0);
        let mut low_b = Biquad::low_pass(sample_rate, 18_000.0);
        for (input, output) in self.raw.iter().zip(self.conditioned.iter_mut()) {
            shelf_low = shelf_low * low_alpha + *input * (1.0 - low_alpha);
            let high = *input - shelf_low;
            *output = low_b.process(low_a.process(shelf_low + high * FRAC_1_SQRT_2));
        }
    }
}

/// The strongest bin between 40 Hz and 1 kHz, weighted toward the low
/// end, on the largest power of two that fits.
fn estimate_pitch(samples: &[f32], sample_rate: u32) -> Option<f32> {
    if samples.len() < 64 || rms(samples) < 0.002 {
        return None;
    }
    let n = 1usize << (usize::BITS - 1 - samples.len().leading_zeros());
    let windowed: Vec<f32> = samples[..n].iter().enumerate().map(|(i, s)| s * hann(i, n)).collect();
    spectrum_of(&windowed, sample_rate.max(1), PITCH_MIN_HZ, PITCH_MAX_HZ)
        .into_iter()
        .max_by(|(hz_a, mag_a), (hz_b, mag_b)| {
            let a = mag_a / hz_a.max(PITCH_MIN_HZ).sqrt();
            let b = mag_b / hz_b.max(PITCH_MIN_HZ).sqrt();
            a.total_cmp(&b)
        })
        .map(|(hz, _)| hz)
}

fn relative_change(a: f32, b: f32) -> f32 {
    if a <= 0.0 || b <= 0.0 {
        return f32::INFINITY;
    }
    (a - b).abs() / b
}

/// A windowed-sinc band-pass 5 % wide around `center_hz`, normalized to
/// unit gain at its center.
fn design_bandpass(center_hz: f32, sample_rate: u32, taps: usize) -> Vec<f32> {
    let rate = sample_rate.max(1) as f32;
    let half_width = (center_hz * 0.05).max(rate / taps as f32);
    let low = (center_hz - half_width).max(PITCH_MIN_HZ * 0.5) / rate;
    let high = (center_hz + half_width).min(rate * 0.48) / rate;
    let middle = (taps - 1) as f32 * 0.5;
    let mut kernel = Vec::with_capacity(taps);
    for index in 0..taps {
        let m = index as f32 - middle;
        let ideal = if m.abs() < f32::EPSILON { 2.0 * (high - low) } else { ((TAU * high * m).sin() - (TAU * low * m).sin()) / (PI * m) };
        let window = 0.5 - 0.5 * (TAU * index as f32 / (taps - 1) as f32).cos();
        kernel.push(ideal * window);
    }
    let omega = TAU * center_hz / rate;
    let (real, imaginary) = kernel.iter().enumerate().fold((0.0f32, 0.0f32), |(real, imaginary), (i, tap)| {
        let phase = omega * i as f32;
        (real + tap * phase.cos(), imaginary - tap * phase.sin())
    });
    let gain = (real * real + imaginary * imaginary).sqrt().max(1.0e-6);
    for tap in &mut kernel {
        *tap /= gain;
    }
    kernel
}

fn convolve(input: &[f32], kernel: &[f32], output: &mut [f32]) {
    for (index, out) in output.iter_mut().enumerate() {
        let count = kernel.len().min(index + 1);
        *out = (0..count).map(|tap| input[index - tap] * kernel[tap]).sum();
    }
}

fn low_pass_draw(input: &[f32], sample_rate: u32, output: &mut [f32]) {
    let mut filter = Biquad::low_pass(sample_rate, 12_000.0);
    for (input, output) in input.iter().zip(output.iter_mut()) {
        *output = filter.process(*input);
    }
}

/// The latest rising zero crossing, confirmed a quarter period later.
fn rising_trigger(signal: &[f32], latest: usize, quarter: usize, threshold: f32) -> Option<f32> {
    for index in (1..=latest.min(signal.len().saturating_sub(2))).rev() {
        let before = signal[index - 1];
        let after = signal[index];
        let validation = index.saturating_add(quarter).min(signal.len() - 1);
        if before <= 0.0 && after > 0.0 && signal[validation].abs() >= threshold {
            let fraction = (-before / (after - before).max(f32::EPSILON)).clamp(0.0, 1.0);
            return Some(index as f32 - 1.0 + fraction);
        }
    }
    None
}

fn interpolate(samples: &[f32], position: f32) -> f32 {
    let base = position.floor().max(0.0) as usize;
    let next = (base + 1).min(samples.len().saturating_sub(1));
    let fraction = position - base as f32;
    samples.get(base).copied().unwrap_or(0.0) * (1.0 - fraction) + samples.get(next).copied().unwrap_or(0.0) * fraction
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo_sine(sample_rate: u32, hz: f32, phase: f32, inverted: bool) -> Vec<f32> {
        (0..2048)
            .flat_map(|frame| {
                let v = (TAU * hz * frame as f32 / sample_rate as f32 + phase).sin() * 0.5;
                [v, if inverted { -v } else { v }]
            })
            .collect()
    }

    #[test]
    fn a_mono_signal_draws_a_vertical_line_and_correlates_fully() {
        let mut scope = ScopeProcessor::default();
        let (points, correlation) = scope.goniometer(&stereo_sine(48_000, 440.0, 0.0, false), 48_000);
        let side = points.iter().step_by(2).fold(0.0f32, |peak, v| peak.max(v.abs()));
        assert!(side < 0.001, "no side: {side}");
        assert!(correlation > 0.999);
    }

    #[test]
    fn an_inverted_pair_draws_horizontal_and_correlates_negatively() {
        let mut scope = ScopeProcessor::default();
        let (points, correlation) = scope.goniometer(&stereo_sine(48_000, 440.0, 0.0, true), 48_000);
        let mid = points.iter().skip(1).step_by(2).fold(0.0f32, |peak, v| peak.max(v.abs()));
        assert!(mid < 0.001, "no mid: {mid}");
        assert!(correlation < -0.999);
    }

    #[test]
    fn the_oscilloscope_locks_a_tone_on_a_rising_crossing() {
        let mut scope = ScopeProcessor::default();
        let output = scope.oscilloscope(&stereo_sine(48_000, 440.0, 1.7, false), 48_000);
        assert!(scope.pitch_hz > 400.0 && scope.pitch_hz < 500.0, "pitch {}", scope.pitch_hz);
        assert!(output.iter().all(|v| v.is_finite()));
        assert!(output.iter().any(|v| v.abs() > 0.1));
    }

    #[test]
    fn silence_has_no_pitch_and_too_few_frames_draw_nothing() {
        assert!(estimate_pitch(&vec![0.0; 2048], 48_000).is_none());
        let mut scope = ScopeProcessor::default();
        assert!(scope.oscilloscope(&[0.1; 64], 48_000).iter().all(|v| *v == 0.0));
    }
}
