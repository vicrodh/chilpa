//! The bars, the energy bands and the transient, from one spectrum.
//!
//! Port of `qbz-audio/src/visualizer/processor.rs`, with the FFT of
//! [`crate::spectrum`] instead of a library: the magnitudes come out
//! divided by the square root of the size, as that library scaled them.
//!
//! Two things are kept on purpose. QBZ transforms the interleaved stereo
//! as if it were one mono signal, so a sound equal in both ears comes out
//! at half its frequency and what differs between the ears lands mirrored
//! against the top. And it windows the samples and then multiplies the
//! samples by that windowed signal again (`processor.rs`, lines 184 to
//! 187), so what it transforms is the signal squared under a Hann window,
//! which doubles every frequency and piles the level up at the bottom. For
//! a tone in both ears the two cancel and it lights its own bar. The bars,
//! the bands and the transient all come from that spectrum, and they move
//! as they do there only if the spectrum is the same. A corrected spectrum
//! is a visible change, so it is not made here.

use super::{BARS, ENERGY_BANDS};
use crate::spectrum::fft;
use std::f32::consts::PI;

const MIN_HZ: f32 = 20.0;
const MAX_HZ: f32 = 20_000.0;
/// Sub-bass, bass, mids, presence and air, in hertz.
const ENERGY_RANGES: [(f32, f32); ENERGY_BANDS] = [(20.0, 60.0), (60.0, 250.0), (250.0, 2000.0), (2000.0, 6000.0), (6000.0, 20_000.0)];
/// The jump in level that counts as a transient, and the frames it then
/// waits: about 100 ms at thirty a second.
const TRANSIENT_THRESHOLD: f32 = 0.04;
const TRANSIENT_COOLDOWN: u32 = 3;
/// How much of the old bar stays when it falls.
const BAR_DECAY: f32 = 0.65;

/// One analysis: the bars and bands after smoothing, and a transient if
/// one landed.
pub struct Analyzed {
    pub bars: [f32; BARS],
    pub energy: [f32; ENERGY_BANDS],
    pub transient: Option<f32>,
}

/// The smoothing between frames: fast up, slow down.
#[derive(Default)]
pub struct Analyzer {
    bars: [f32; BARS],
    energy: [f32; ENERGY_BANDS],
    previous_rms: f32,
    cooldown: u32,
}

impl Analyzer {
    pub fn analyze(&mut self, samples: &[f32], sample_rate_hz: u32) -> Analyzed {
        let spectrum = squared_spectrum(samples, sample_rate_hz, MIN_HZ, MAX_HZ);

        let mut bars = [0.0f32; BARS];
        log_bars(&spectrum, &mut bars);
        for (bar, kept) in bars.iter_mut().zip(self.bars.iter_mut()) {
            *kept = if *bar > *kept { *kept * 0.3 + *bar * 0.7 } else { *kept * BAR_DECAY + *bar * (1.0 - BAR_DECAY) };
            *bar = *kept;
        }

        let mut energy = [0.0f32; ENERGY_BANDS];
        let mut raw_sum = 0.0f32;
        for (band, &(lo, hi)) in ENERGY_RANGES.iter().enumerate() {
            let (mut sum_sq, mut count) = (0.0f32, 0u32);
            for &(hz, magnitude) in &spectrum {
                if hz >= lo && hz < hi {
                    sum_sq += magnitude * magnitude;
                    count += 1;
                }
            }
            let rms = if count > 0 { (sum_sq / count as f32).sqrt() } else { 0.0 };
            let compressed = (rms * 6.0).powf(0.5).clamp(0.0, 1.0);
            let kept = &mut self.energy[band];
            *kept = if compressed > *kept { *kept * 0.2 + compressed * 0.8 } else { *kept * 0.85 + compressed * 0.15 };
            energy[band] = *kept;
            // The transient hears the raw level, with the two bass bands
            // counting double.
            raw_sum += compressed * if band < 2 { 2.0 } else { 1.0 };
        }

        let raw_rms = raw_sum / (ENERGY_BANDS as f32 + 2.0);
        let delta = raw_rms - self.previous_rms;
        self.cooldown = self.cooldown.saturating_sub(1);
        let transient = if delta > TRANSIENT_THRESHOLD && self.cooldown == 0 {
            self.cooldown = TRANSIENT_COOLDOWN;
            Some((delta * 5.0).clamp(0.0, 1.0))
        } else {
            None
        };
        self.previous_rms = raw_rms;

        Analyzed { bars, energy, transient }
    }
}

/// The Hann coefficient at `i` of `n`, as the library QBZ used defines it.
pub fn hann(i: usize, n: usize) -> f32 {
    0.5 * (1.0 - (2.0 * PI * i as f32 / n as f32).cos())
}

/// The magnitudes, divided by the square root of the size, of the bins
/// whose frequency lies within `lo..=hi`, each with its frequency.
pub fn spectrum_of(windowed: &[f32], sample_rate_hz: u32, lo: f32, hi: f32) -> Vec<(f32, f32)> {
    let n = windowed.len();
    let mut re = windowed.to_vec();
    let mut im = vec![0.0f32; n];
    fft(&mut re, &mut im);
    let scale = 1.0 / (n as f32).sqrt();
    let per_bin = sample_rate_hz as f32 / n as f32;
    (0..n / 2)
        .map(|k| (k as f32 * per_bin, (re[k] * re[k] + im[k] * im[k]).sqrt() * scale))
        .filter(|&(hz, _)| hz >= lo && hz <= hi)
        .collect()
}

/// The spectrum of the signal squared under a Hann window: see the module
/// doc for why the square is kept.
fn squared_spectrum(samples: &[f32], sample_rate_hz: u32, lo: f32, hi: f32) -> Vec<(f32, f32)> {
    let n = samples.len();
    let windowed: Vec<f32> = samples.iter().enumerate().map(|(i, s)| s * s * hann(i, n)).collect();
    spectrum_of(&windowed, sample_rate_hz, lo, hi)
}

/// The bins averaged into bars spread logarithmically between 20 Hz and
/// 20 kHz, the bass weighted up and the highs down, then compressed.
fn log_bars(spectrum: &[(f32, f32)], out: &mut [f32]) {
    let bars = out.len();
    let (min_log, max_log) = (MIN_HZ.ln(), MAX_HZ.ln());
    for (i, bar) in out.iter_mut().enumerate() {
        let low = (min_log + (max_log - min_log) * i as f32 / bars as f32).exp();
        let high = (min_log + (max_log - min_log) * (i + 1) as f32 / bars as f32).exp();
        let (mut sum, mut count) = (0.0f32, 0u32);
        for &(hz, magnitude) in spectrum {
            if hz >= low && hz < high {
                let weight = if hz < 200.0 { 1.5 } else if hz < 2000.0 { 1.0 } else { 0.8 };
                sum += magnitude * weight;
                count += 1;
            }
        }
        let average = if count > 0 { sum / count as f32 } else { 0.0 };
        *bar = (average * 4.0).powf(0.6).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visualizer::FFT_SIZE;

    /// A stereo tone, interleaved, as the producer hands it over.
    fn tone(hz: f32, rate: u32, amplitude: f32) -> Vec<f32> {
        (0..FFT_SIZE / 2)
            .flat_map(|i| {
                let v = amplitude * (2.0 * PI * hz * i as f32 / rate as f32).sin();
                [v, v]
            })
            .collect()
    }

    fn bar_of(hz: f32) -> usize {
        let (min_log, max_log) = (MIN_HZ.ln(), MAX_HZ.ln());
        ((hz.ln() - min_log) / (max_log - min_log) * BARS as f32).floor() as usize
    }

    #[test]
    fn the_bars_are_spread_logarithmically() {
        let (min_log, max_log) = (MIN_HZ.ln(), MAX_HZ.ln());
        let at = |i: usize| (min_log + (max_log - min_log) * i as f32 / BARS as f32).exp();
        assert!(at(0) > 19.0 && at(0) < 25.0);
        assert!(at(BARS / 2) > 500.0 && at(BARS / 2) < 800.0, "the middle bar sits near the geometric mean");
        assert!(at(BARS - 1) > 10_000.0);
    }

    #[test]
    fn silence_moves_nothing() {
        let mut analyzer = Analyzer::default();
        let out = analyzer.analyze(&vec![0.0; FFT_SIZE], 48_000);
        assert!(out.bars.iter().all(|b| *b == 0.0));
        assert!(out.energy.iter().all(|e| *e == 0.0));
        assert!(out.transient.is_none());
    }

    /// Interleaving halves a tone heard in both ears and the square
    /// doubles it back: the bar that lights is the tone's own, as QBZ
    /// draws it.
    #[test]
    fn a_tone_in_both_ears_lights_its_own_bar() {
        let mut analyzer = Analyzer::default();
        let out = analyzer.analyze(&tone(1_000.0, 48_000, 0.8), 48_000);
        let loudest = out.bars.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i).unwrap();
        assert_eq!(loudest, bar_of(1_000.0), "{:?}", out.bars);
        assert!(out.bars[loudest] > 0.2, "it is seen: {}", out.bars[loudest]);
    }

    /// A 1 kHz tone lands in the mids band; the bands above it stay quiet.
    #[test]
    fn a_tone_puts_its_energy_in_its_band() {
        let mut analyzer = Analyzer::default();
        let out = analyzer.analyze(&tone(1_000.0, 48_000, 0.8), 48_000);
        assert!(out.energy[2] > out.energy[4], "mids {} over air {}", out.energy[2], out.energy[4]);
        assert!(out.energy[2] > 0.1);
    }

    #[test]
    fn a_jump_in_level_fires_one_transient_and_then_cools_down() {
        let mut analyzer = Analyzer::default();
        assert!(analyzer.analyze(&vec![0.0; FFT_SIZE], 48_000).transient.is_none());
        let loud = tone(100.0, 48_000, 0.9);
        let first = analyzer.analyze(&loud, 48_000);
        assert!(first.transient.is_some_and(|t| t > 0.0 && t <= 1.0), "the jump is a transient");
        for _ in 0..TRANSIENT_COOLDOWN {
            assert!(analyzer.analyze(&loud, 48_000).transient.is_none(), "steady level is not");
        }
    }

    #[test]
    fn the_bars_rise_fast_and_fall_slowly() {
        let mut analyzer = Analyzer::default();
        let up = analyzer.analyze(&tone(1_000.0, 48_000, 0.8), 48_000);
        let bar = bar_of(1_000.0);
        let risen = up.bars[bar];
        let down = analyzer.analyze(&vec![0.0; FFT_SIZE], 48_000);
        assert!(down.bars[bar] > 0.0 && down.bars[bar] < risen, "it falls by steps: {} after {risen}", down.bars[bar]);
        assert!((down.bars[bar] - risen * BAR_DECAY).abs() < 1e-5);
    }
}
