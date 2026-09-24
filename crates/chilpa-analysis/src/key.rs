//! Estimating a track's key.
//!
//! A chromagram is built — how much energy sits on each of the twelve
//! pitch classes, whatever the octave — and compared against the
//! Krumhansl-Schmuckler profiles, which came out of asking people how well
//! each note fits in a key. The rotation that correlates best is the key.
//!
//! What has to be said about this:
//!
//! - **It confuses relative keys.** A major and its relative minor use the
//!   same notes; what tells them apart is which one feels like rest, and
//!   that is not in the chromagram. Hence the confidence figure.
//! - **It is wrong about music that modulates.** It returns the key that
//!   weighs most over what was examined, not a history.
//! - **It comes out in two notations.** The musical one ("F#m") and the
//!   Camelot one ("11A"), which is the one you mix with.

use std::f32::consts::PI;

/// The names of the twelve pitch classes, with sharps.
const NOTES: [&str; 12] =
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Krumhansl-Kessler profiles for major and minor.
const MAYOR: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// How many samples fit in a window. At 11,025 Hz that is 0.74 s, giving
/// 1.35 Hz per bin: three bins per semitone down low.
const WINDOW: usize = 8192;

/// The key that was found.
#[derive(Clone, PartialEq, Debug)]
pub struct Key {
    /// How it is written: "C", "F#m".
    pub name: String,
    /// How you mix with it: "8B", "11A".
    pub camelot: String,
    /// From 0 to 1. Below 0.6 the profile barely tells one from another and
    /// it is better to claim nothing.
    pub confidence: f32,
}

/// Estimates the key of a mono signal.
pub fn detect(samples: &[f32], sample_rate_hz: u32) -> Option<Key> {
    let chroma = chroma(samples, sample_rate_hz)?;
    let (index, is_major, confidence) = best_profile(&chroma)?;
    Some(Key {
        name: name(index, is_major),
        camelot: camelot(index, is_major),
        confidence: confidence,
    })
}

/// How much energy sits on each pitch class, summing every octave.
pub fn chroma(samples: &[f32], sample_rate_hz: u32) -> Option<[f32; 12]> {
    if sample_rate_hz == 0 || samples.len() < WINDOW {
        return None;
    }
    let half = WINDOW / 2;
    let per_bin = sample_rate_hz as f32 / WINDOW as f32;
    // Looked at from 65 Hz (C2) to 2100 Hz (C7): below is kick drum noise and
    // above, the harmonics no longer say what key it is in.
    let k_min = ((65.0 / per_bin) as usize).max(1);
    let k_max = ((2100.0 / per_bin) as usize).min(half - 1);
    if k_max <= k_min {
        return None;
    }

    let mut chroma = [0.0f32; 12];
    let skip = WINDOW / 2;
    let mut start = 0;
    let mut windows = 0usize;
    while start + WINDOW <= samples.len() {
        let mut re: Vec<f32> = Vec::with_capacity(WINDOW);
        for (i, m) in samples[start..start + WINDOW].iter().enumerate() {
            let hann = 0.5 - 0.5 * (2.0 * PI * i as f32 / (WINDOW as f32 - 1.0)).cos();
            re.push(m * hann);
        }
        let mut im = vec![0.0f32; WINDOW];
        chilpa_audio::spectrum::fft(&mut re, &mut im);

        for k in k_min..=k_max {
            let magnitude = (re[k] * re[k] + im[k] * im[k]).sqrt();
            if magnitude <= 0.0 {
                continue;
            }
            let hz = k as f32 * per_bin;
            // Nota MIDI: 69 es el la de 440 Hz.
            let midi = 69.0 + 12.0 * (hz / 440.0).log2();
            let class = (midi.round() as i32).rem_euclid(12) as usize;
            chroma[class] += magnitude;
        }
        windows += 1;
        start += skip;
    }
    if windows == 0 {
        return None;
    }
    let total: f32 = chroma.iter().sum();
    if total <= 0.0 {
        return None;
    }
    for c in chroma.iter_mut() {
        *c /= total;
    }
    Some(chroma)
}

/// The rotation and mode that correlate best, with their confidence.
fn best_profile(chroma: &[f32; 12]) -> Option<(usize, bool, f32)> {
    let mut results: Vec<(usize, bool, f32)> = Vec::with_capacity(24);
    for tonic in 0..12 {
        for (is_major, profile) in [(true, &MAYOR), (false, &MINOR)] {
            let rotated: Vec<f32> = (0..12).map(|i| profile[(i + 12 - tonic) % 12]).collect();
            results.push((tonic, is_major, correlation(chroma, &rotated)));
        }
    }
    results.sort_by(|a, b| b.2.total_cmp(&a.2));
    let best = results.first().copied()?;
    if !best.2.is_finite() || best.2 <= 0.0 {
        return None;
    }
    // Confidence is how far it beats the runner-up: a chromagram that fits
    // two keys equally well says nothing.
    let second = results.get(1).map(|r| r.2).unwrap_or(0.0).max(0.0);
    let confidence = ((best.2 - second) / best.2).clamp(0.0, 1.0);
    Some((best.0, best.1, confidence))
}

/// Pearson correlation between two vectors of the same length.
fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let mut num = 0.0f32;
    let (mut da, mut db) = (0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        num += (x - ma) * (y - mb);
        da += (x - ma) * (x - ma);
        db += (y - mb) * (y - mb);
    }
    let den = (da * db).sqrt();
    if den <= 0.0 {
        0.0
    } else {
        num / den
    }
}

fn name(index: usize, is_major: bool) -> String {
    format!("{}{}", NOTES[index % 12], if is_major { "" } else { "m" })
}

/// The Camelot wheel: the number says how far along the circle of fifths it
/// sits, and the letter whether it is minor (A) or major (B).
fn camelot(index: usize, is_major: bool) -> String {
    // The circle of fifths starting at C. C major is 8B and A minor is 8A,
    // and it counts from there.
    let quintas = [0usize, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];
    let pos = quintas.iter().position(|q| *q == index % 12).unwrap_or(0);
    let number = if is_major {
        // C sits at position 0 and gets 8.
        (pos + 7) % 12 + 1
    } else {
        // A sits at position 3 and gets 8: the minor wheel runs three fifths
        // behind the major one.
        (pos + 4) % 12 + 1
    };
    format!("{number}{}", if is_major { "B" } else { "A" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A MIDI note's frequency.
    fn hz(midi: f32) -> f32 {
        440.0 * 2f32.powf((midi - 69.0) / 12.0)
    }

    /// A chord held on those MIDI notes.
    fn chord(notes: &[f32], rate: u32, seconds: u32) -> Vec<f32> {
        let total = (rate * seconds) as usize;
        (0..total)
            .map(|i| {
                notes
                    .iter()
                    .map(|n| (2.0 * PI * hz(*n) * i as f32 / rate as f32).sin())
                    .sum::<f32>()
                    / notes.len() as f32
            })
            .collect()
    }

    #[test]
    fn c_major_comes_out_as_c_major() {
        // The seven notes of the C scale, so the profile has something to
        // work with: on a lone triad, C major and A minor tie.
        let scale: Vec<f32> = [60.0, 62.0, 64.0, 65.0, 67.0, 69.0, 71.0].to_vec();
        // Weighting the tonic and the dominant, as in real music.
        let mut notes = scale.clone();
        notes.extend_from_slice(&[60.0, 67.0, 60.0]);
        let k = detect(&chord(&notes, 11_025, 6), 11_025).expect("there is a key");
        assert_eq!(k.name, "C", "it came out {} ({})", k.name, k.camelot);
        assert_eq!(k.camelot, "8B");
    }

    #[test]
    fn a_minor_comes_out_as_a_minor() {
        let scale: Vec<f32> = [57.0, 59.0, 60.0, 62.0, 64.0, 65.0, 67.0].to_vec();
        let mut notes = scale.clone();
        // In minor, what rules is the tonic and the minor third.
        notes.extend_from_slice(&[57.0, 60.0, 57.0, 64.0]);
        let k = detect(&chord(&notes, 11_025, 6), 11_025).expect("there is a key");
        assert_eq!(k.name, "Am", "it came out {} ({})", k.name, k.camelot);
        assert_eq!(k.camelot, "8A");
    }

    #[test]
    fn silence_has_no_key() {
        assert!(detect(&vec![0.0; 11_025 * 4], 11_025).is_none());
    }

    #[test]
    fn too_little_material_invents_no_key() {
        assert!(detect(&[0.1; 100], 11_025).is_none());
        assert!(detect(&[0.1; 20_000], 0).is_none());
    }

    #[test]
    fn the_camelot_wheel_wraps_all_the_way_around() {
        // The twenty four slots, none repeated and none missing.
        let mut vistas: Vec<String> = Vec::new();
        for i in 0..12 {
            for mayor in [true, false] {
                vistas.push(camelot(i, mayor));
            }
        }
        vistas.sort();
        vistas.dedup();
        assert_eq!(vistas.len(), 24, "there are 24 distinct slots");
        // And the usual two anchors.
        assert_eq!(camelot(0, true), "8B", "do mayor");
        assert_eq!(camelot(9, false), "8A", "A minor");
        assert_eq!(camelot(7, true), "9B", "G major, a fifth up");
        assert_eq!(camelot(0, false), "5A", "C minor");
        assert_eq!(camelot(6, false), "11A", "F sharp minor");
        // Every major key shares a slot with its relative minor.
        for i in 0..12usize {
            let mayor = camelot(i, true);
            let relative = camelot((i + 9) % 12, false);
            assert_eq!(
                mayor[..mayor.len() - 1],
                relative[..relative.len() - 1],
                "{} and its relative minor share a number",
                NOTES[i]
            );
        }
    }

    #[test]
    fn a_flat_chromagram_picks_nothing() {
        // Every note equally strong: there is no meaningful tonic.
        let flat = [1.0f32 / 12.0; 12];
        let (_, _, confidence) = best_profile(&flat).unwrap_or((0, true, 0.0));
        assert!(confidence < 0.2, "it should not be confident: {confidence}");
    }
}
