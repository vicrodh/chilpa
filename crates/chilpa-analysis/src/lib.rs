//! What can be learned about a track by listening to it.
//!
//! Today: tempo, key and loudness. Whatever comes next is added here and
//! goes out the same door, because the library stores features with the
//! analyzer's version and can recompute without disturbing the old rows.
//!
//! None of this touches the playback path. Analysis reads files and writes
//! numbers; the audio that plays never finds out.

pub mod key;
pub mod loudness;
pub mod tempo;

pub use key::{detect as detect_key, Key};
pub use loudness::Meter as LoudnessMeter;
pub use tempo::{detect as detect_tempo, Tempo};

/// This analyzer's version.
///
/// It goes up when the result changes for the same input, and also when
/// something new is extracted: a version 1 row carries no key and no
/// loudness, and leaving it as good would mean half a lame library. The
/// library stores this number next to every feature, so raising it is what
/// makes recomputing possible without deleting what came before.
///
/// - 1: tempo.
/// - 2: tempo, key and loudness.
pub const VERSION: i64 = 2;

/// What this analyzer is called in the database.
pub const NAME: &str = "chilpa";

/// What analysis pulls out of a track.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Analysis {
    pub bpm: Option<f32>,
    /// From 0 to 1: how pronounced the beat is.
    pub tempo_confidence: Option<f32>,
    /// The key, when one could be decided.
    pub key: Option<Key>,
    /// How loud it sounds, in LUFS. Measured, never corrected.
    pub loudness_lufs: Option<f32>,
}

/// The least confidence needed to claim a key.
///
/// Below this the chromagram fits two keys almost equally well, and naming
/// one would be inventing it.
pub const MIN_KEY_CONFIDENCE: f32 = 0.05;

/// Analyzes an already decoded mono signal.
pub fn analyze(samples: &[f32], sample_rate_hz: u32) -> Analysis {
    let t = tempo::detect(samples, sample_rate_hz);
    // The key is looked for on the resampled signal, which is where the
    // bins land at the size the chromagram wants.
    let bajada = tempo::resample_for_analysis(samples, sample_rate_hz);
    let k = key::detect(&bajada, tempo::ANALYSIS_HZ)
        .filter(|k| k.confidence >= MIN_KEY_CONFIDENCE);
    Analysis {
        bpm: t.map(|t| t.bpm),
        tempo_confidence: t.map(|t| t.confidence),
        key: k,
        // Loudness is measured on the signal as it comes, not on the one
        // resampled to count beats; the caller supplies it.
        loudness_lufs: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_beat_and_no_tone_reports_nothing() {
        let f = analyze(&vec![0.0; 44_100 * 10], 44_100);
        assert_eq!(f, Analysis::default(), "zeros, not invented numbers");
        assert!(f.bpm.is_none() && f.key.is_none());
    }
}
