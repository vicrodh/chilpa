//! Streaming half-band FIR used after the eight-to-one DSD stage.

use std::sync::OnceLock;

const HALF_BAND_TAPS: usize = 63;

fn half_band_taps() -> &'static [f32; HALF_BAND_TAPS] {
    static TAPS: OnceLock<[f32; HALF_BAND_TAPS]> = OnceLock::new();
    TAPS.get_or_init(|| {
        let center = (HALF_BAND_TAPS - 1) as f64 / 2.0;
        let mut taps = [0.0; HALF_BAND_TAPS];
        let mut sum = 0.0;
        for (index, tap) in taps.iter_mut().enumerate() {
            let x = index as f64 - center;
            let sinc = if x == 0.0 {
                0.5
            } else {
                (std::f64::consts::PI * 0.5 * x).sin() / (std::f64::consts::PI * x)
            };
            let window = 0.42
                - 0.5
                    * (2.0 * std::f64::consts::PI * index as f64 / (HALF_BAND_TAPS - 1) as f64)
                        .cos()
                + 0.08
                    * (4.0 * std::f64::consts::PI * index as f64 / (HALF_BAND_TAPS - 1) as f64)
                        .cos();
            let value = sinc * window;
            *tap = value as f32;
            sum += value;
        }
        let scale = (1.0 / sum) as f32;
        for tap in &mut taps {
            *tap *= scale;
        }
        taps
    })
}

pub struct HalfBand {
    pending: Vec<f32>,
    next: usize,
}

impl HalfBand {
    pub fn new() -> Self {
        Self {
            pending: vec![0.0; HALF_BAND_TAPS - 1],
            next: HALF_BAND_TAPS - 1,
        }
    }

    pub fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        let taps = half_band_taps();
        self.pending.extend_from_slice(input);
        let mut center = self.next;
        while center < self.pending.len() {
            let window = &self.pending[center + 1 - HALF_BAND_TAPS..=center];
            let mut sum = 0.0;
            for tap in (0..HALF_BAND_TAPS).step_by(2) {
                sum += taps[tap] * window[HALF_BAND_TAPS - 1 - tap];
            }
            sum += taps[HALF_BAND_TAPS / 2] * window[HALF_BAND_TAPS / 2];
            output.push(sum);
            center += 2;
        }
        let keep_from = self.pending.len().saturating_sub(HALF_BAND_TAPS - 1);
        let overshoot = center - self.pending.len();
        self.pending.drain(..keep_from);
        self.next = HALF_BAND_TAPS - 1 + overshoot;
    }
}
