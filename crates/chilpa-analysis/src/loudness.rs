//! Measuring how loud a track sounds, in LUFS.
//!
//! This is ITU-R BS.1770: filter with the K curve — which is what resembles
//! how a head hears — measure power in 400 ms blocks, and average only the
//! blocks that pass two gates, so the silence between songs does not drag
//! the reading down.
//!
//! **This measures, it does not correct.** Applying a gain would change the
//! samples and there goes bit-perfect. What this module does is say a
//! number; what is done with it is another decision.
//!
//! Measured on the fly, block by block: an hour-long track has no business
//! fitting in memory just to learn how loud it is.

/// The standard's offset. It cancels the K curve's gain at 1 kHz, which is
/// why a 0 dBFS sine on one channel measures -3.01 LKFS.
const OFFSET: f64 = -0.691;

/// Below this a block is silence and does not count.
const ABSOLUTE_GATE: f64 = -70.0;

/// The second gate sits ten units below the average.
const RELATIVE_GATE: f64 = -10.0;

/// How long a block lasts, and how far apart two of them start.
const BLOCK_MS: f64 = 400.0;
const STEP_MS: f64 = 100.0;

/// A biquad in transposed direct form II.
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn step(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    /// The K curve's first stage: a treble shelf, the one that imitates the
    /// head.
    fn boost(rate: f64) -> Self {
        let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let vh = 10f64.powf(gain_db / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        Self {
            b0: (vh + vb * k / q + k * k) / a0,
            b1: 2.0 * (k * k - vh) / a0,
            b2: (vh - vb * k / q + k * k) / a0,
            a1: 2.0 * (k * k - 1.0) / a0,
            a2: (1.0 - k / q + k * k) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// The second stage: it takes out what sits so low it is not heard as
    /// loudness.
    fn high_pass(rate: f64) -> Self {
        let (f0, q) = (38.13547087602444, 0.5003270373238773);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        Self {
            b0: 1.0,
            b1: -2.0,
            b2: 1.0,
            a1: 2.0 * (k * k - 1.0) / a0,
            a2: (1.0 - k / q + k * k) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

/// Each channel's weight, per the standard.
///
/// The fronts weigh one; the rears a little more, because surround is
/// perceived as louder than it measures.
fn weight(channel: usize, channels: usize) -> f64 {
    // With stereo or mono there are no rears to weigh.
    if channels <= 2 {
        return 1.0;
    }
    // In 5.1 the usual order is L, R, C, LFE, Ls, Rs.
    match channel {
        3 => 0.0,           // the LFE does not count
        4 | 5 => 1.41,      // the rears
        _ => 1.0,
    }
}

/// A meter that eats the signal block by block.
pub struct Meter {
    channels: usize,
    filters: Vec<(Biquad, Biquad)>,
    /// How many samples per channel the current block holds.
    in_block: usize,
    samples_per_block: usize,
    samples_per_step: usize,
    /// The sum of squares per channel, for each `STEP_MS` stretch.
    segments: Vec<Vec<f64>>,
    current_segment: Vec<f64>,
    /// The power of each block already closed.
    blocks: Vec<f64>,
}

impl Meter {
    pub fn new(sample_rate_hz: u32, channels: u16) -> Option<Self> {
        let channels = channels.max(1) as usize;
        if sample_rate_hz < 8_000 {
            return None;
        }
        let rate = f64::from(sample_rate_hz);
        Some(Self {
            channels,
            filters: (0..channels)
                .map(|_| (Biquad::boost(rate), Biquad::high_pass(rate)))
                .collect(),
            in_block: 0,
            samples_per_block: (rate * BLOCK_MS / 1000.0).round() as usize,
            samples_per_step: (rate * STEP_MS / 1000.0).round() as usize,
            segments: Vec::new(),
            current_segment: vec![0.0; channels],
            blocks: Vec::new(),
        })
    }

    /// Eats a chunk of channel-interleaved audio.
    pub fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(self.channels) {
            for (c, m) in frame.iter().enumerate() {
                let (boost, high_pass) = &mut self.filters[c];
                let y = high_pass.step(boost.step(f64::from(*m)));
                self.current_segment[c] += y * y;
            }
            self.in_block += 1;
            if self.in_block == self.samples_per_step {
                self.close_segment();
            }
        }
    }

    fn close_segment(&mut self) {
        self.segments.push(std::mem::replace(&mut self.current_segment, vec![0.0; self.channels]));
        self.in_block = 0;
        // A block is four stretches; one closes for every new stretch, which
        // is the 75% overlap the standard asks for.
        let per_block = self.samples_per_block / self.samples_per_step;
        if self.segments.len() >= per_block {
            let from = self.segments.len() - per_block;
            let mut z = 0.0;
            for c in 0..self.channels {
                let sum: f64 = self.segments[from..].iter().map(|t| t[c]).sum();
                z += weight(c, self.channels) * sum / self.samples_per_block as f64;
            }
            self.blocks.push(z);
        }
    }

    /// The integrated loudness, in LUFS.
    ///
    /// `None` when not one block passed the gate: silence, or a track
    /// shorter than 400 ms.
    pub fn integrated(&self) -> Option<f64> {
        if self.blocks.is_empty() {
            return None;
        }
        let level = |z: f64| if z > 0.0 { OFFSET + 10.0 * z.log10() } else { f64::NEG_INFINITY };

        // First gate: anything that is silence does not count.
        let pass: Vec<f64> =
            self.blocks.iter().copied().filter(|z| level(*z) > ABSOLUTE_GATE).collect();
        if pass.is_empty() {
            return None;
        }
        // Second gate: ten units below the average of what was left. It is
        // what keeps the silences between sections from dragging down a
        // dynamic track's reading.
        let mean = pass.iter().sum::<f64>() / pass.len() as f64;
        let threshold = level(mean) + RELATIVE_GATE;
        let kept: Vec<f64> =
            pass.into_iter().filter(|z| level(*z) > threshold).collect();
        if kept.is_empty() {
            return None;
        }
        let mean = kept.iter().sum::<f64>() / kept.len() as f64;
        let l = level(mean);
        l.is_finite().then_some(l)
    }
}

/// Measures a signal that is already complete.
pub fn integrated(interleaved: &[f32], sample_rate_hz: u32, channels: u16) -> Option<f64> {
    let mut m = Meter::new(sample_rate_hz, channels)?;
    m.push(interleaved);
    m.integrated()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// A 997 Hz sine spread across whatever channels are asked for.
    fn sine(amplitude: f32, rate: u32, seconds: u32, channels: u16, only_one: bool) -> Vec<f32> {
        let frames = (rate * seconds) as usize;
        let mut v = Vec::with_capacity(frames * channels as usize);
        for i in 0..frames {
            let m = amplitude * (2.0 * PI * 997.0 * i as f64 / f64::from(rate)).sin() as f32;
            for c in 0..channels {
                v.push(if only_one && c > 0 { 0.0 } else { m });
            }
        }
        v
    }

    #[test]
    fn a_zero_dbfs_sine_on_one_channel_measures_minus_three_lkfs() {
        // It is the standard's calibration: 0 dBFS at 997 Hz on a single
        // channel is -3.01 LKFS. If this moves, the K curve is wrong.
        let data = sine(1.0, 48_000, 5, 2, true);
        let l = integrated(&data, 48_000, 2).expect("there is sound");
        assert!((l - (-3.01)).abs() < 0.1, "it measured {l:.3} LKFS, -3.01 was expected");
    }

    #[test]
    fn the_same_sine_on_both_channels_measures_three_units_more() {
        let one = integrated(&sine(1.0, 48_000, 5, 2, true), 48_000, 2).unwrap();
        let two = integrated(&sine(1.0, 48_000, 5, 2, false), 48_000, 2).unwrap();
        assert!((two - one - 3.01).abs() < 0.1, "de {one:.2} a {two:.2}");
    }

    #[test]
    fn twice_the_amplitude_is_six_units_more() {
        let low = integrated(&sine(0.25, 48_000, 5, 2, false), 48_000, 2).unwrap();
        let high = integrated(&sine(0.5, 48_000, 5, 2, false), 48_000, 2).unwrap();
        assert!((high - low - 6.02).abs() < 0.1, "de {low:.2} a {high:.2}");
    }

    #[test]
    fn the_measurement_does_not_depend_on_the_sample_rate() {
        // The K curve is recomputed for each rate; if it were fixed at 48
        // kHz, these two would drift apart.
        let a = integrated(&sine(0.5, 44_100, 5, 2, false), 44_100, 2).unwrap();
        let b = integrated(&sine(0.5, 48_000, 5, 2, false), 48_000, 2).unwrap();
        let c = integrated(&sine(0.5, 96_000, 5, 2, false), 96_000, 2).unwrap();
        assert!((a - b).abs() < 0.1, "44.1 gave {a:.2} and 48 gave {b:.2}");
        assert!((c - b).abs() < 0.1, "96 gave {c:.2} and 48 gave {b:.2}");
    }

    #[test]
    fn silence_has_no_loudness() {
        assert_eq!(integrated(&vec![0.0; 48_000 * 4], 48_000, 2), None);
    }

    #[test]
    fn a_track_shorter_than_a_block_gives_no_number() {
        // 400 ms is the block; 200 is not enough.
        assert_eq!(integrated(&sine(0.5, 48_000, 0, 2, false), 48_000, 2), None);
        let short = sine(0.5, 48_000, 1, 2, false);
        assert!(integrated(&short[..48_000 * 2 / 5], 48_000, 2).is_none());
    }

    #[test]
    fn silences_do_not_drag_the_reading_down() {
        // Five seconds of tone and five of silence: the relative gate has to
        // leave the silence out.
        let mut data = sine(0.5, 48_000, 5, 2, false);
        let tone_only = integrated(&data, 48_000, 2).unwrap();
        data.extend(std::iter::repeat(0.0).take(48_000 * 5 * 2));
        let with_silence = integrated(&data, 48_000, 2).unwrap();
        assert!(
            (tone_only - with_silence).abs() < 0.2,
            "the silence moved the reading from {tone_only:.2} to {with_silence:.2}"
        );
    }

    #[test]
    fn an_impossible_sample_rate_builds_no_meter() {
        assert!(Meter::new(0, 2).is_none());
        assert!(Meter::new(4_000, 2).is_none());
    }
}
