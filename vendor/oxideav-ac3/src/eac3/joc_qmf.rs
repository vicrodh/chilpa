//! Complex 64-band quadrature mirror filter bank from ETSI TS 103 420,
//! clauses 7.2 through 7.4.

// Modulation indices are bounded by the fixed 64/128-point transforms. The
// final f64-to-f32 conversion intentionally stores single-precision DSP tables.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::{f64::consts::PI, sync::OnceLock};

use super::joc_qmf_table::QMF_WINDOW;

pub(super) const QMF_SUBBANDS: usize = 64;
pub(super) const QMF_TIMESLOTS_PER_EAC3_FRAME: usize = 24;
/// Integer delay of the analysis/synthesis pair. Keep metadata and
/// bypassed channels on the same timeline as reconstructed objects.
pub(super) const QMF_RECONSTRUCTION_DELAY: usize = 577;
const QMF_FILTER_LENGTH: usize = 640;
const QMF_MODULATION_SIZE: usize = QMF_SUBBANDS * 2;
const QMF_SYNTHESIS_STATE_LENGTH: usize = QMF_FILTER_LENGTH * 2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Complex32 {
    pub(super) re: f32,
    pub(super) im: f32,
}

/// Modulation matrices, stored one array per component and laid out so
/// each transform walks its input once while updating a contiguous run
/// of independent accumulators (which the compiler can vectorise).
struct ModulationTables {
    /// `[modulation sample][subband]` cosines for analysis.
    analysis_cos: Vec<f32>,
    /// `[modulation sample][subband]` sines for analysis.
    analysis_sin: Vec<f32>,
    /// `[subband][modulation sample]` cosine / n for synthesis.
    synthesis_cos: Vec<f32>,
    /// `[subband][modulation sample]` sine / n for synthesis.
    synthesis_sin: Vec<f32>,
}

fn modulation_tables() -> &'static ModulationTables {
    static TABLES: OnceLock<ModulationTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut analysis_cos = vec![0.0; QMF_MODULATION_SIZE * QMF_SUBBANDS];
        let mut analysis_sin = vec![0.0; QMF_MODULATION_SIZE * QMF_SUBBANDS];
        for subband in 0..QMF_SUBBANDS {
            for sample in 0..QMF_MODULATION_SIZE {
                let phase =
                    PI * (subband as f64 + 0.5) * (sample as f64 - 0.5) / QMF_SUBBANDS as f64;
                analysis_cos[sample * QMF_SUBBANDS + subband] = phase.cos() as f32;
                analysis_sin[sample * QMF_SUBBANDS + subband] = phase.sin() as f32;
            }
        }

        let mut synthesis_cos = vec![0.0; QMF_SUBBANDS * QMF_MODULATION_SIZE];
        let mut synthesis_sin = vec![0.0; QMF_SUBBANDS * QMF_MODULATION_SIZE];
        for sample in 0..QMF_MODULATION_SIZE {
            for subband in 0..QMF_SUBBANDS {
                // Use the normative matrix equation on TS 103 420 page 67:
                // (j - 2n + 1/2) / n. Pseudocode 14 on that page has a
                // conflicting expanded expression; the equation composes with
                // the analysis matrix into the specified reconstruction bank.
                let phase = PI / (4 * QMF_SUBBANDS) as f64
                    * (2 * subband + 1) as f64
                    * (2.0 * sample as f64 - 4.0 * QMF_SUBBANDS as f64 + 1.0);
                synthesis_cos[subband * QMF_MODULATION_SIZE + sample] =
                    (phase.cos() / QMF_SUBBANDS as f64) as f32;
                synthesis_sin[subband * QMF_MODULATION_SIZE + sample] =
                    (phase.sin() / QMF_SUBBANDS as f64) as f32;
            }
        }

        ModulationTables {
            analysis_cos,
            analysis_sin,
            synthesis_cos,
            synthesis_sin,
        }
    })
}

#[derive(Clone)]
pub(super) struct QmfAnalysis {
    state: [f32; QMF_FILTER_LENGTH],
}

impl Default for QmfAnalysis {
    fn default() -> Self {
        Self {
            state: [0.0; QMF_FILTER_LENGTH],
        }
    }
}

impl QmfAnalysis {
    /// Transform 64 consecutive time-domain samples into one complex QMF
    /// timeslot. Input order is chronological.
    ///
    /// Every accumulator below receives its terms in the same order as
    /// the textbook form (taps in ascending group order, modulation
    /// samples in ascending order); only the loop nesting differs, so
    /// the inner loops run over independent accumulators.
    pub(super) fn process(&mut self, pcm: &[f32; QMF_SUBBANDS]) -> [Complex32; QMF_SUBBANDS] {
        self.state
            .copy_within(..QMF_FILTER_LENGTH - QMF_SUBBANDS, QMF_SUBBANDS);
        for (destination, sample) in self.state[..QMF_SUBBANDS].iter_mut().zip(pcm.iter().rev()) {
            *destination = *sample;
        }

        let mut folded = [0.0_f32; QMF_MODULATION_SIZE];
        for (state, window) in self
            .state
            .as_chunks::<QMF_MODULATION_SIZE>()
            .0
            .iter()
            .zip(QMF_WINDOW.as_chunks::<QMF_MODULATION_SIZE>().0)
        {
            for ((value, &state), &window) in folded.iter_mut().zip(state).zip(window) {
                *value = (state * window).mul_add(1.0, *value);
            }
        }

        let tables = modulation_tables();
        let mut real = [0.0_f32; QMF_SUBBANDS];
        let mut imaginary = [0.0_f32; QMF_SUBBANDS];
        for ((&sample, cosines), sines) in folded
            .iter()
            .zip(tables.analysis_cos.as_chunks::<QMF_SUBBANDS>().0)
            .zip(tables.analysis_sin.as_chunks::<QMF_SUBBANDS>().0)
        {
            for (value, &cosine) in real.iter_mut().zip(cosines) {
                *value = sample.mul_add(cosine, *value);
            }
            for (value, &sine) in imaginary.iter_mut().zip(sines) {
                *value = sample.mul_add(sine, *value);
            }
        }
        let mut output = [Complex32::default(); QMF_SUBBANDS];
        for ((value, re), im) in output.iter_mut().zip(real).zip(imaginary) {
            *value = Complex32 { re, im };
        }
        output
    }
}

#[derive(Clone)]
pub(super) struct QmfSynthesis {
    state: [f32; QMF_SYNTHESIS_STATE_LENGTH],
    /// No non-zero sample has entered the state since it was cleared.
    silent: bool,
}

impl Default for QmfSynthesis {
    fn default() -> Self {
        Self {
            state: [0.0; QMF_SYNTHESIS_STATE_LENGTH],
            silent: true,
        }
    }
}

impl QmfSynthesis {
    pub(super) fn is_silent(&self) -> bool {
        self.silent || self.state.iter().all(|sample| sample.abs() <= f32::EPSILON)
    }

    /// Transform one complex QMF timeslot into 64 chronological PCM samples.
    ///
    /// As in [`QmfAnalysis::process`], the accumulation order of every
    /// output is the textbook one (subbands ascending, then tap groups
    /// ascending with the first window half before the second).
    pub(super) fn process(&mut self, qmf: &[Complex32; QMF_SUBBANDS]) -> [f32; QMF_SUBBANDS] {
        self.state.copy_within(
            ..QMF_SYNTHESIS_STATE_LENGTH - QMF_MODULATION_SIZE,
            QMF_MODULATION_SIZE,
        );

        let tables = modulation_tables();
        let mut modulated = [0.0_f32; QMF_MODULATION_SIZE];
        for ((qmf_sample, cosines), sines) in qmf
            .iter()
            .zip(tables.synthesis_cos.as_chunks::<QMF_MODULATION_SIZE>().0)
            .zip(tables.synthesis_sin.as_chunks::<QMF_MODULATION_SIZE>().0)
        {
            let re = qmf_sample.re;
            let negated_im = -qmf_sample.im;
            for ((value, &cosine), &sine) in modulated.iter_mut().zip(cosines).zip(sines) {
                *value = re.mul_add(cosine, negated_im.mul_add(sine, *value));
            }
        }
        self.state[..QMF_MODULATION_SIZE].copy_from_slice(&modulated);
        self.silent = false;

        let mut output = [0.0_f32; QMF_SUBBANDS];
        for tap_group in 0..QMF_FILTER_LENGTH / QMF_MODULATION_SIZE {
            let first_window = &QMF_WINDOW[tap_group * QMF_MODULATION_SIZE..][..QMF_SUBBANDS];
            let second_window =
                &QMF_WINDOW[tap_group * QMF_MODULATION_SIZE + QMF_SUBBANDS..][..QMF_SUBBANDS];
            let first_state = &self.state[tap_group * QMF_SUBBANDS * 4..][..QMF_SUBBANDS];
            let second_state =
                &self.state[tap_group * QMF_SUBBANDS * 4 + QMF_SUBBANDS * 3..][..QMF_SUBBANDS];
            for ((((value, &state_a), &window_a), &state_b), &window_b) in output
                .iter_mut()
                .zip(first_state)
                .zip(first_window)
                .zip(second_state)
                .zip(second_window)
            {
                *value = (state_a * window_a).mul_add(1.0, *value);
                *value = (state_b * window_b).mul_add(1.0, *value);
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_and_synthesis_are_a_near_perfect_delayed_round_trip() {
        const BLOCKS: usize = 96;
        let mut seed = 0x5eed_1234_u32;
        let mut input = Vec::with_capacity(BLOCKS * QMF_SUBBANDS);
        for _ in 0..input.capacity() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let signed_seed = i32::from_ne_bytes(seed.to_ne_bytes());
            input.push((signed_seed as f32 / i32::MAX as f32) * 0.25);
        }

        let mut analysis = QmfAnalysis::default();
        let mut synthesis = QmfSynthesis::default();
        let mut output = Vec::with_capacity(input.len());
        for pcm in input.as_chunks::<QMF_SUBBANDS>().0 {
            output.extend(synthesis.process(&analysis.process(pcm)));
        }

        let mut best = (0_usize, f64::NEG_INFINITY, 0.0_f64);
        for delay in 0..=QMF_SYNTHESIS_STATE_LENGTH {
            let source = &input[..input.len() - delay];
            let decoded = &output[delay..];
            let dot: f64 = source
                .iter()
                .zip(decoded)
                .map(|(&left, &right)| f64::from(left) * f64::from(right))
                .sum();
            let source_energy: f64 = source.iter().map(|&sample| f64::from(sample).powi(2)).sum();
            let decoded_energy: f64 = decoded
                .iter()
                .map(|&sample| f64::from(sample).powi(2))
                .sum();
            let correlation = dot.abs() / (source_energy * decoded_energy).sqrt();
            if correlation > best.1 {
                best = (delay, correlation, dot / source_energy);
            }
        }

        assert!(
            best.1 > 0.999,
            "QMF round-trip correlation is too low: delay={}, correlation={}, gain={}",
            best.0,
            best.1,
            best.2,
        );
        assert_eq!(best.0, QMF_RECONSTRUCTION_DELAY);
        assert!(
            (best.2.abs() - 1.0).abs() < 0.01,
            "QMF round-trip gain is not unity: delay={}, correlation={}, gain={}",
            best.0,
            best.1,
            best.2,
        );
    }
}
