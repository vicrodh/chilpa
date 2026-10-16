//! Loudness normalization, ported from QBZ: `qbz-audio`'s `loudness.rs`,
//! `loudness_cache.rs`, `loudness_analyzer.rs` and `dynamic_amplify.rs`,
//! and `qbz-player`'s `normalization.rs`.
//!
//! A track starts at a gain resolved before its first sample: the cache of
//! measured loudness, then the ReplayGain the source or the file gives
//! (kept in the cache as such), else unity. While it plays, the analyzer
//! measures it (BS.1770 integrated loudness and true peak, `ebur128`): when
//! nothing was known, the first measurement after ten seconds sets the gain
//! once, and the ramp glides to it in 400 ms; every later window only
//! refines the cache. The gain is a plain multiplication: no limiter, no
//! compressor. It changes the samples, so a normalized track is processed,
//! never bit-perfect.

pub mod analyzer;
pub mod cache;
pub mod level;
pub mod ramp;

/// ReplayGain 2.0's reference: a track gain of `g` dB means the track sits
/// at `-18 - g` LUFS.
pub const REPLAYGAIN_REFERENCE_LUFS: f32 = -18.0;
/// The most a track is raised. Lowering is never capped.
pub const MAX_BOOST_DB: f32 = 6.0;
/// What is kept under full scale when a peak is known and clipping is
/// guarded.
const PEAK_HEADROOM_DB: f32 = -1.0;

/// The targets the setting offers, QBZ's.
pub const TARGETS: [f32; 5] = [-11.0, -14.0, -16.0, -18.0, -23.0];
/// The target until one is chosen: what streaming services aim at.
pub const DEFAULT_TARGET: f32 = -14.0;

/// A gain in dB as a factor: 0 dB is 1.0, -6 dB about 0.5.
pub fn db_to_linear(db: f32) -> f32 {
    10_f32.powf(db / 20.0)
}

pub fn lufs_from_replaygain(gain_db: f32) -> f32 {
    REPLAYGAIN_REFERENCE_LUFS - gain_db
}

/// The factor that moves a track at `lufs` to `target`: a boost capped at
/// [`MAX_BOOST_DB`], and with `prevent_clipping` a boost further capped at
/// what keeps the known peak [`PEAK_HEADROOM_DB`] under full scale. The guard
/// only limits a boost: a track that needs none keeps unity (or its cut)
/// even when its own peaks already sit above the headroom.
pub fn gain_for(lufs: f32, peak: Option<f32>, target: f32, prevent_clipping: bool) -> f32 {
    let db = (target - lufs).min(MAX_BOOST_DB);
    let mut gain = db_to_linear(db);
    if prevent_clipping {
        if let Some(peak) = peak.filter(|p| *p > 0.0) {
            let ceiling = (db_to_linear(PEAK_HEADROOM_DB) / peak).max(1.0);
            if gain > ceiling {
                gain = ceiling;
            }
        }
    }
    gain
}

/// A track's gain before its first sample, and whether it was known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartGain {
    pub gain: f32,
    pub known: bool,
}

/// The start gain (QBZ, `resolve_start_gain`): the cache first, then the
/// ReplayGain the source gives or the file's tags, which are kept in the
/// cache as a full-track figure; unity when nothing is known, and the
/// analyzer measures it.
pub fn start_gain(
    cache: &cache::LoudnessCache,
    key: &str,
    said: Option<(f32, Option<f32>)>,
    tags: impl FnOnce() -> (Option<f32>, Option<f32>),
    target: f32,
    prevent_clipping: bool,
) -> StartGain {
    if let Some((gain, _)) = cache.resolve_gain(key, target, prevent_clipping) {
        return StartGain { gain, known: true };
    }
    let replaygain = said.or_else(|| {
        let (gain, peak) = tags();
        gain.map(|gain| (gain, peak))
    });
    if let Some((gain_db, peak)) = replaygain.filter(|(gain, _)| gain.is_finite()) {
        let peak = peak.filter(|p| p.is_finite() && *p > 0.0);
        let lufs = lufs_from_replaygain(gain_db);
        cache.store(key, lufs, peak, cache::Rank::ReplayGain);
        return StartGain { gain: gain_for(lufs, peak, target, prevent_clipping), known: true };
    }
    StartGain { gain: 1.0, known: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibels_are_factors() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 0.001);
        assert!((db_to_linear(-6.0) - 0.501).abs() < 0.01);
        assert!((db_to_linear(6.0) - 1.995).abs() < 0.01);
        assert!((db_to_linear(-20.0) - 0.1).abs() < 0.001);
    }

    #[test]
    fn the_gain_follows_the_target_and_its_caps() {
        // -12 LUFS to -14: two dB down; to -18: six down.
        assert!((gain_for(-12.0, None, -14.0, true) - db_to_linear(-2.0)).abs() < 0.001);
        assert!((gain_for(-12.0, None, -18.0, true) - db_to_linear(-6.0)).abs() < 0.001);
        // A quiet track: never more than +6 dB without a peak.
        assert!((gain_for(-30.0, None, -14.0, true) - db_to_linear(6.0)).abs() < 0.001);
        // With a peak at 0.5 the ceiling is -1 dBFS over it.
        assert!((gain_for(-30.0, Some(0.5), -14.0, true) - db_to_linear(-1.0) / 0.5).abs() < 0.001);
        // A peak already at 0.9 leaves no room to boost, and is not cut.
        assert_eq!(gain_for(-20.0, Some(0.9), -14.0, true), 1.0);
        // Without the guard the boost is the full six.
        assert!((gain_for(-30.0, Some(0.9), -14.0, false) - db_to_linear(6.0)).abs() < 0.001);
        // A cut is never capped.
        assert!((gain_for(-5.0, Some(1.0), -14.0, true) - db_to_linear(-9.0)).abs() < 0.001);
        assert_eq!(lufs_from_replaygain(-4.0), -14.0);
    }

    #[test]
    fn the_start_gain_prefers_the_cache_then_replaygain_then_unity() {
        let cache = cache::LoudnessCache::in_memory();
        let unknown = start_gain(&cache, "a", None, || (None, None), -14.0, true);
        assert_eq!(unknown, StartGain { gain: 1.0, known: false });
        // ReplayGain -4 dB is -14 LUFS: at a -14 target, unity, and it is kept.
        let said = start_gain(&cache, "b", Some((-4.0, None)), || panic!("the source's gain wins over the tags"), -14.0, true);
        assert_eq!(said, StartGain { gain: 1.0, known: true });
        assert_eq!(cache.lookup("b").map(|r| r.rank), Some(cache::Rank::ReplayGain));
        let tagged = start_gain(&cache, "c", None, || (Some(-4.0), Some(0.5)), -18.0, true);
        assert!(tagged.known && (tagged.gain - db_to_linear(-4.0)).abs() < 0.001);
        // A measured row wins over everything.
        cache.store("d", -20.0, None, cache::Rank::Measured);
        let kept = start_gain(&cache, "d", Some((-10.0, None)), || (None, None), -14.0, true);
        assert!((kept.gain - db_to_linear(6.0)).abs() < 0.001);
    }

    #[test]
    fn a_source_gain_of_zero_still_replaces_the_tags() {
        let cache = cache::LoudnessCache::in_memory();
        let said = start_gain(&cache, "z", Some((0.0, None)), || panic!("the tags are not read"), -18.0, true);
        assert_eq!(said, StartGain { gain: 1.0, known: true });
        let nan = start_gain(&cache, "n", Some((f32::NAN, None)), || (None, None), -18.0, true);
        assert_eq!(nan, StartGain { gain: 1.0, known: false }, "a gain that is no number says nothing");
    }
}
