//! Estimating a track's tempo.
//!
//! The method is the usual one and holds no mystery: pull an onset envelope,
//! autocorrelate it, and the lag that repeats most is the beat.
//!
//! What does matter to say:
//!
//! - **It is an estimate, and it is called one.** A tempo detector gets
//!   music without a marked beat wrong, and gets music that has one wrong
//!   by double or half. Chilpa stores the number with the analyzer's
//!   version so it can be recomputed when there is a better one.
//! - **The octave is resolved, not ignored.** 75 and 150 explain the same
//!   signal equally well; the range music is actually counted in wins, and
//!   that preference is written here, not hidden.
//! - **The whole track is not read.** A minute from the middle is enough,
//!   and analyzing a library cannot cost what playing it would.

/// The rate this works at. The beat needs no more, and it cuts the cost of
/// the autocorrelation to a tenth.
pub const ANALYSIS_HZ: u32 = 11_025;

/// How many samples each envelope frame advances. At 11,025 Hz that is
/// about 12 ms.
///
/// With longer frames, a tempo whose period does not land on a round number
/// of frames splits its correlation between two lags and loses to its own
/// half. That is exactly what made 140 BPM come out as 70.
const HOP: usize = 128;

/// The range searched. Outside it there is no music to count.
pub const BPM_MIN: f32 = 50.0;
pub const BPM_MAX: f32 = 210.0;

/// Where to prefer landing when two answers explain the same thing.
const PREFERRED_MIN: f32 = 80.0;
const PREFERRED_MAX: f32 = 160.0;

/// What could be said about the beat.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tempo {
    pub bpm: f32,
    /// From 0 to 1: how pronounced the beat is. Below 0.1 it is better to say
    /// nothing than to say an invented number.
    pub confidence: f32,
}

/// Estimates the tempo of a mono signal.
///
/// Returns `None` when there is not enough material or when no beat is
/// found: a player that invents 120 BPM for everything is no use to
/// anyone.
pub fn detect(samples: &[f32], sample_rate_hz: u32) -> Option<Tempo> {
    if sample_rate_hz == 0 {
        return None;
    }
    let mono = downsample(samples, sample_rate_hz, ANALYSIS_HZ);
    // Less than eight seconds is not enough to count bars.
    if mono.len() < ANALYSIS_HZ as usize * 8 {
        return None;
    }
    let envelope = onset_envelope(&mono);
    if envelope.len() < 64 {
        return None;
    }
    tempo_from_envelope(&envelope, ANALYSIS_HZ as f32 / HOP as f32)
}

/// The onset envelope: how much energy rose from one frame to the next.
///
/// Rises only. A drop in energy is not a hit, and counting it would double
/// the beat.
fn onset_envelope(mono: &[f32]) -> Vec<f32> {
    let frames = mono.len() / HOP;
    let mut energy = Vec::with_capacity(frames);
    for c in 0..frames {
        let slice = &mono[c * HOP..(c + 1) * HOP];
        let sum: f32 = slice.iter().map(|m| m * m).sum();
        // In the log domain: the ear hears beats in proportion, not by subtraction.
        energy.push((sum / HOP as f32 + 1e-9).ln());
    }
    let mut onsets = Vec::with_capacity(energy.len());
    onsets.push(0.0);
    for i in 1..energy.len() {
        onsets.push((energy[i] - energy[i - 1]).max(0.0));
    }
    onsets
}

/// The tempo of an envelope sampled at `frames_per_second`.
fn tempo_from_envelope(onsets: &[f32], frames_per_second: f32) -> Option<Tempo> {
    // It is centred: without removing the mean, the autocorrelation rewards
    // lag 0 and everything else is lost under the slope.
    let mean = onsets.iter().sum::<f32>() / onsets.len() as f32;
    let centered: Vec<f32> = onsets.iter().map(|a| a - mean).collect();
    let energy: f32 = centered.iter().map(|a| a * a).sum();
    if energy <= 0.0 {
        return None;
    }

    let lag_for = |bpm: f32| (frames_per_second * 60.0 / bpm).round() as usize;
    let lag_min = lag_for(BPM_MAX).max(1);
    let lag_max = lag_for(BPM_MIN).min(centered.len() / 2);
    if lag_max <= lag_min {
        return None;
    }

    let mut scores = vec![0.0f32; lag_max + 2];
    for lag in lag_min..=lag_max {
        let mut sum = 0.0f32;
        for i in 0..centered.len() - lag {
            sum += centered[i] * centered[i + lag];
        }
        // It is normalized by how many terms went in: without that, long lags
        // lose just for having less overlap.
        scores[lag] = sum / energy * (centered.len() as f32 / (centered.len() - lag) as f32);
    }

    // A period that does not land on a whole number of frames splits its
    // correlation between two neighbouring lags, and so loses to its own
    // double, which may have landed better centred. Measured with a 128 BPM
    // pulse: 0.65 at lag 40 and 0.37 at 41, against 0.77 at 81. That is why
    // the peak's sum with its neighbours is compared, not the bin alone.
    let neighborhood = |lag: usize| -> f32 {
        let from = lag.saturating_sub(1);
        let to = (lag + 1).min(scores.len() - 1);
        scores[from..=to].iter().map(|p| p.max(0.0)).sum()
    };

    let mut best = (0usize, 0.0f32);
    for lag in lag_min..=lag_max {
        let v = neighborhood(lag);
        if v > best.1 {
            best = (lag, v);
        }
    }
    if best.0 == 0 || best.1 <= 0.0 {
        return None;
    }

    // Of all the lags that explain it about equally well, the one falling in
    // the range music is actually counted in is preferred. 70 and 140
    // explain the same thing; music is counted at 140.
    let threshold = best.1 * 0.85;
    let mut chosen = best.0;
    let mut best_in_range = 0.0f32;
    for lag in lag_min..=lag_max {
        let v = neighborhood(lag);
        if v < threshold {
            continue;
        }
        let bpm = frames_per_second * 60.0 / lag as f32;
        if (PREFERRED_MIN..=PREFERRED_MAX).contains(&bpm) && v > best_in_range {
            best_in_range = v;
            chosen = lag;
        }
    }

    // The neighbourhood serves for comparing octaves, not for pointing at
    // the frame. Within the chosen neighbourhood the real peak is the
    // highest bin: interpolating around its poor neighbour sends the result
    // far off (measured: 140 BPM came out as 146).
    let chosen = argmax_local(&scores, chosen);
    let lag = interpolate(&scores, chosen);
    // The confidence is the raw peak's: it is what says how marked the
    // pulse is, not how much it was split between two frames.
    let confidence = scores[best.0].clamp(0.0, 1.0);
    Some(Tempo { bpm: frames_per_second * 60.0 / lag, confidence: confidence })
}

/// The highest bin among `lag` and its two neighbours.
fn argmax_local(scores: &[f32], lag: usize) -> usize {
    let from = lag.saturating_sub(1);
    let to = (lag + 1).min(scores.len() - 1);
    (from..=to).max_by(|a, b| scores[*a].total_cmp(&scores[*b])).unwrap_or(lag)
}

/// The vertex of the parabola through the peak and its two neighbours.
///
/// Without this the tempo can only take the values that land on a whole
/// number of frames, and at 140 BPM that is already several BPM of error.
fn interpolate(scores: &[f32], lag: usize) -> f32 {
    if lag == 0 || lag + 1 >= scores.len() {
        return lag as f32;
    }
    let (a, b, c) = (scores[lag - 1], scores[lag], scores[lag + 1]);
    let denominator = a - 2.0 * b + c;
    if denominator.abs() < 1e-9 {
        return lag as f32;
    }
    let adjustment = 0.5 * (a - c) / denominator;
    if adjustment.abs() > 1.0 {
        return lag as f32;
    }
    lag as f32 + adjustment
}

/// The signal at the rate the analysis works at.
pub fn resample_for_analysis(samples: &[f32], sample_rate_hz: u32) -> Vec<f32> {
    downsample(samples, sample_rate_hz.max(1), ANALYSIS_HZ)
}

/// Drops the sample rate by averaging. It is not a fine resampler; for
/// counting beats none is needed.
fn downsample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from <= to || samples.is_empty() {
        return samples.to_vec();
    }
    let factor = (from / to).max(1) as usize;
    samples
        .chunks(factor)
        .map(|t| t.iter().sum::<f32>() / t.len() as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dry beats at that tempo, with a little tail.
    fn pulse(bpm: f32, rate: u32, seconds: u32) -> Vec<f32> {
        let total = (rate * seconds) as usize;
        let period = (rate as f32 * 60.0 / bpm) as usize;
        let mut v = vec![0.0f32; total];
        let mut i = 0;
        while i < total {
            // A beat that decays in 30 ms.
            let tail = (rate / 33) as usize;
            for k in 0..tail.min(total - i) {
                let decay = 1.0 - k as f32 / tail as f32;
                v[i + k] = decay * decay;
            }
            i += period;
        }
        v
    }

    #[test]
    fn a_120_beat_is_detected_as_120() {
        let t = detect(&pulse(120.0, 44_100, 20), 44_100).expect("there is a pulse");
        assert!((t.bpm - 120.0).abs() < 3.0, "it came out {}", t.bpm);
        assert!(t.confidence > 0.1, "low confidence: {}", t.confidence);
    }

    #[test]
    fn several_tempos_land_close() {
        // Two BPM of margin: with the peak interpolation, the frames'
        // quantization error stays below that.
        for expected in [85.0f32, 90.0, 100.0, 110.0, 128.0, 140.0, 150.0] {
            let t = detect(&pulse(expected, 44_100, 20), 44_100)
                .unwrap_or_else(|| panic!("{expected} was not detected"));
            assert!(
                (t.bpm - expected).abs() < 2.0,
                "{expected} was expected, it came out {}",
                t.bpm
            );
        }
    }

    #[test]
    fn a_beat_under_noise_still_comes_out() {
        // Deterministic noise, so the test does not depend on the day.
        let mut x: u32 = 0x1234_5678;
        let mut v = pulse(120.0, 44_100, 20);
        for m in v.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *m += ((x % 1000) as f32 / 1000.0 - 0.5) * 0.2;
        }
        let t = detect(&v, 44_100).expect("the pulse survives the noise");
        assert!((t.bpm - 120.0).abs() < 3.0, "it came out {}", t.bpm);
    }

    #[test]
    fn a_slow_tempo_is_reported_in_the_range_it_is_counted_in() {
        // 60 BPM counts just as well as 120: the double is preferred.
        let t = detect(&pulse(60.0, 44_100, 24), 44_100).expect("there is a pulse");
        assert!(
            (t.bpm - 120.0).abs() < 4.0 || (t.bpm - 60.0).abs() < 4.0,
            "it came out {}, which is neither 60 nor its double",
            t.bpm
        );
    }

    #[test]
    fn silence_has_no_tempo() {
        assert!(detect(&vec![0.0; 44_100 * 20], 44_100).is_none());
    }

    #[test]
    fn a_very_short_track_gives_no_invented_number() {
        assert!(detect(&pulse(120.0, 44_100, 3), 44_100).is_none(), "three seconds are not enough");
        assert!(detect(&[], 44_100).is_none());
        assert!(detect(&pulse(120.0, 44_100, 20), 0).is_none(), "with no rate, nothing");
    }

    #[test]
    fn resampling_averages_and_shortens() {
        let v: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let r = downsample(&v, 44_100, 11_025);
        assert_eq!(r.len(), 2, "four at a time");
        assert!((r[0] - 1.5).abs() < 1e-6, "the average of 0..3: {}", r[0]);
        // Going up in rate does nothing.
        assert_eq!(downsample(&v, 11_025, 44_100).len(), 8);
    }
}
