//! The spectral ribbon's bands: a compact spectrogram column at its own
//! cadence. Port of `qbz-audio/src/analysis/spectral_ribbon.rs` on the FFT
//! of [`crate::spectrum`]; the magnitudes are divided by the size, the
//! bands are RMS scaled by 25 and smoothed with a faster attack than
//! release. The buffers are allocated once.

use crate::spectrum::fft;
use std::f32::consts::PI;
use std::time::{Duration, Instant};

const MIN_HZ: f32 = 20.0;
const MAX_HZ: f32 = 20_000.0;

pub struct SpectralAnalyzer {
    sample_rate_hz: u32,
    fft_size: usize,
    bands: usize,
    smoothing: f32,
    interval: Duration,
    last: Instant,
    window: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    magnitudes: Vec<f32>,
    ranges: Vec<(usize, usize)>,
    raw: Vec<f32>,
    smoothed: Vec<f32>,
    latest: Vec<f32>,
}

impl SpectralAnalyzer {
    /// `fft_size` is one of 512 to 8192, else 1024; `bands` 48 to 1024;
    /// `rate_hz` 20 to 60; `smoothing` 0 to 0.98.
    pub fn new(sample_rate_hz: u32, fft_size: usize, bands: usize, rate_hz: u32, smoothing: f32) -> Self {
        let fft_size = match fft_size {
            512 | 1024 | 2048 | 4096 | 8192 => fft_size,
            _ => 1024,
        };
        let bands = bands.clamp(48, 1024);
        let rate_hz = rate_hz.clamp(20, 60);
        let mut analyzer = Self {
            sample_rate_hz,
            fft_size,
            bands,
            smoothing: smoothing.clamp(0.0, 0.98),
            interval: Duration::from_secs_f32(1.0 / rate_hz as f32),
            last: Instant::now() - Duration::from_secs(1),
            window: (0..fft_size).map(|n| 0.5 * (1.0 - (2.0 * PI * n as f32 / (fft_size - 1) as f32).cos())).collect(),
            re: vec![0.0; fft_size],
            im: vec![0.0; fft_size],
            magnitudes: vec![0.0; fft_size / 2],
            ranges: vec![(0, 0); bands],
            raw: vec![0.0; bands],
            smoothed: vec![0.0; bands],
            latest: vec![0.0; bands],
        };
        analyzer.rebuild_ranges(sample_rate_hz);
        analyzer
    }

    /// Analyzes `samples` if its cadence allows it, and says whether the
    /// bands were refreshed. The samples are taken as they come: QBZ hands
    /// it the interleaved stereo and so does the producer here.
    pub fn process(&mut self, samples: &[f32], sample_rate_hz: u32) -> bool {
        if samples.len() < self.fft_size {
            return false;
        }
        let now = Instant::now();
        if now.duration_since(self.last) < self.interval {
            return false;
        }
        self.last = now;
        if sample_rate_hz != self.sample_rate_hz {
            self.sample_rate_hz = sample_rate_hz;
            self.rebuild_ranges(sample_rate_hz);
        }

        for (i, sample) in samples.iter().take(self.fft_size).enumerate() {
            self.re[i] = sample * self.window[i];
            self.im[i] = 0.0;
        }
        fft(&mut self.re, &mut self.im);
        let norm = 1.0 / self.fft_size as f32;
        for i in 0..self.fft_size / 2 {
            self.magnitudes[i] = (self.re[i] * self.re[i] + self.im[i] * self.im[i]).sqrt() * norm;
        }

        for band in 0..self.bands {
            let (start, end) = self.ranges[band];
            if end <= start {
                self.raw[band] = 0.0;
                continue;
            }
            let sum: f32 = self.magnitudes[start..end].iter().map(|m| m * m).sum();
            let rms = (sum / (end - start) as f32).sqrt();
            self.raw[band] = (rms * 25.0).clamp(0.0, 1.0);
        }
        for i in 0..self.bands {
            let (new, previous) = (self.raw[i], self.smoothed[i]);
            let alpha = if new > previous { 1.0 - self.smoothing * 0.5 } else { 1.0 - self.smoothing };
            let smoothed = previous + alpha * (new - previous);
            self.smoothed[i] = smoothed;
            self.latest[i] = smoothed.clamp(0.0, 1.0);
        }
        true
    }

    pub fn bands(&self) -> &[f32] {
        &self.latest
    }

    fn rebuild_ranges(&mut self, sample_rate_hz: u32) {
        let nyquist = sample_rate_hz as f32 * 0.5;
        let max_hz = MAX_HZ.min(nyquist.max(MIN_HZ + 1.0));
        let (min_log, max_log) = (MIN_HZ.ln(), max_hz.ln());
        let per_bin = sample_rate_hz as f32 / self.fft_size as f32;
        let max_bin = self.magnitudes.len().saturating_sub(1);
        for band in 0..self.bands {
            let low = (min_log + (max_log - min_log) * band as f32 / self.bands as f32).exp();
            let high = (min_log + (max_log - min_log) * (band + 1) as f32 / self.bands as f32).exp();
            let start = ((low / per_bin).floor() as usize).min(max_bin);
            let mut end = ((high / per_bin).ceil() as usize).min(max_bin.saturating_add(1));
            if end <= start {
                end = (start + 1).min(max_bin.saturating_add(1));
            }
            self.ranges[band] = (start, end);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_returns_as_many_bands_as_asked() {
        let mut analyzer = SpectralAnalyzer::new(48_000, 1024, 64, 24, 0.8);
        let _ = analyzer.process(&vec![0.0f32; 1024], 48_000);
        assert_eq!(analyzer.bands().len(), 64);
    }

    #[test]
    fn a_tone_lands_in_the_band_that_covers_it_and_the_cadence_gates_the_next() {
        let mut analyzer = SpectralAnalyzer::new(48_000, 4096, 512, 58, 0.30);
        let tone: Vec<f32> = (0..4096).map(|i| 0.8 * (2.0 * PI * 1_000.0 * i as f32 / 48_000.0).sin()).collect();
        assert!(analyzer.process(&tone, 48_000), "the first frame always goes through");
        let loudest = analyzer.bands().iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i).unwrap();
        let (min_log, max_log) = (MIN_HZ.ln(), MAX_HZ.ln());
        let low = (min_log + (max_log - min_log) * loudest as f32 / 512.0).exp();
        let high = (min_log + (max_log - min_log) * (loudest + 1) as f32 / 512.0).exp();
        // Neighboring bands share bins at the edges, so the loudest one
        // may sit one bin to a side of the tone: within 3 % is its place.
        assert!(low <= 1_030.0 && high >= 970.0, "the peak landed at {low:.0}..{high:.0} Hz");
        assert!(!analyzer.process(&tone, 48_000), "a second frame within 17 ms waits");
    }

    #[test]
    fn out_of_range_sizes_fall_back() {
        let analyzer = SpectralAnalyzer::new(44_100, 3000, 10, 5, 2.0);
        assert_eq!(analyzer.fft_size, 1024);
        assert_eq!(analyzer.bands, 48);
        assert_eq!(analyzer.smoothing, 0.98);
    }
}
