//! The gain on the samples (QBZ, `dynamic_amplify.rs`): the start gain from
//! the first sample, and when the analyzer writes a new one a linear glide
//! to it over 400 ms. 50 ms took the click away but still read as a step;
//! 400 ms is a glide. The shared value is read every 1,024 samples while
//! nothing glides: one atomic load per sample was 384,000 a second at
//! 192 kHz stereo.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub const RAMP_MS: u32 = 400;
const POLL_EVERY: u32 = 1024;

pub struct GainRamp {
    shared: Arc<AtomicU32>,
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
    /// Samples (all channels) in one glide at this rate.
    length: u32,
    countdown: u32,
}

impl GainRamp {
    pub fn new(shared: Arc<AtomicU32>, initial: f32, rate: u32, channels: u16) -> Self {
        let length = (u64::from(rate) * u64::from(channels) * u64::from(RAMP_MS) / 1000) as u32;
        Self { shared, current: initial, target: initial, step: 0.0, remaining: 0, length, countdown: 0 }
    }

    fn poll(&mut self) {
        let wanted = f32::from_bits(self.shared.load(Ordering::Relaxed));
        // Zero means not worked out yet.
        if wanted == 0.0 || !wanted.is_finite() || (wanted - self.target).abs() <= f32::EPSILON {
            return;
        }
        self.target = wanted;
        if self.length > 0 {
            self.step = (self.target - self.current) / self.length as f32;
            self.remaining = self.length;
        } else {
            self.current = self.target;
        }
    }

    /// The gain for the next sample.
    #[inline]
    pub(super) fn next(&mut self) -> f32 {
        if self.remaining == 0 {
            if self.countdown == 0 {
                self.poll();
                self.countdown = POLL_EVERY;
            } else {
                self.countdown -= 1;
            }
        }
        if self.remaining > 0 {
            self.current += self.step;
            self.remaining -= 1;
            if self.remaining == 0 {
                // The end of a glide lands exactly, without float drift.
                self.current = self.target;
            }
        }
        self.current
    }

    /// Floats, times the gain and `volume`.
    pub fn apply_float(&mut self, samples: &mut [f32], volume: f32) {
        for sample in samples {
            *sample *= self.next() * volume;
        }
    }

    /// Integers of `bits`, times the gain and `volume`, rounded and kept
    /// within their range.
    pub fn apply_int(&mut self, samples: &mut [i32], bits: u8, volume: f32) {
        let top = top(bits);
        for sample in samples {
            let gain = f64::from(self.next() * volume);
            if gain != 1.0 {
                *sample = scaled(*sample, gain, top);
            }
        }
    }
}

fn top(bits: u8) -> i64 {
    (1i64 << (bits.clamp(2, 32) - 1)) - 1
}

fn scaled(sample: i32, gain: f64, top: i64) -> i32 {
    ((f64::from(sample) * gain).round() as i64).clamp(-top - 1, top) as i32
}

/// Integers of `bits` times one `gain`, rounded and clamped, never wrapped.
pub(crate) fn scale_int(samples: &mut [i32], bits: u8, gain: f32) {
    let (top, gain) = (top(bits), f64::from(gain));
    samples.iter_mut().for_each(|sample| *sample = scaled(*sample, gain, top));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_gain_applies_from_the_first_sample_without_a_glide() {
        let shared = Arc::new(AtomicU32::new(0.5f32.to_bits()));
        let mut ramp = GainRamp::new(shared, 0.5, 1000, 1);
        let mut samples = vec![1.0f32; 4];
        ramp.apply_float(&mut samples, 1.0);
        assert_eq!(samples, vec![0.5; 4]);
    }

    #[test]
    fn a_new_gain_glides_over_the_ramp_and_lands_exactly() {
        let shared = Arc::new(AtomicU32::new(0.5f32.to_bits()));
        let mut ramp = GainRamp::new(shared.clone(), 0.5, 1000, 1);
        shared.store(0.25f32.to_bits(), Ordering::Relaxed);
        let mut samples = vec![1.0f32; 400];
        ramp.apply_float(&mut samples, 1.0);
        assert!(samples[0] < 0.5 && samples[0] > 0.25, "already moving: {}", samples[0]);
        assert!((samples[199] - 0.375).abs() < 0.01, "halfway: {}", samples[199]);
        assert_eq!(samples[399], 0.25, "lands on the target");
        let mut after = vec![1.0f32; 2];
        ramp.apply_float(&mut after, 1.0);
        assert_eq!(after, vec![0.25; 2]);
    }

    #[test]
    fn integers_are_scaled_rounded_and_clipped_to_their_range() {
        let shared = Arc::new(AtomicU32::new(2.0f32.to_bits()));
        let mut ramp = GainRamp::new(shared, 2.0, 1000, 1);
        let mut samples = vec![100, -100, 30000, -30000];
        ramp.apply_int(&mut samples, 16, 1.0);
        assert_eq!(samples, vec![200, -200, 32767, -32768]);
        let unity = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let mut same = GainRamp::new(unity, 1.0, 1000, 1);
        let mut untouched = vec![12345, -1];
        same.apply_int(&mut untouched, 24, 1.0);
        assert_eq!(untouched, vec![12345, -1], "unity leaves the samples as they are");
    }

    #[test]
    fn a_constant_gain_stays_within_the_bits() {
        let mut s = vec![16_000, -16_000, 32_767, -32_768];
        scale_int(&mut s, 16, 2.0);
        assert_eq!(s, [32_000, -32_000, 32_767, -32_768], "clamped, not wrapped");
        let mut s = vec![1_000];
        scale_int(&mut s, 16, 0.5);
        assert_eq!(s, [500]);
    }
}
