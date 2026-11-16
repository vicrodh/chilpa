//! Presentation changes preserve live filter and dynamics histories.
use super::{Filter, Preset, Shape};
impl Filter {
    pub(super) fn affects_audio(&self) -> bool {
        self.enabled && !(matches!(self.shape, Shape::Peak | Shape::LowShelf | Shape::HighShelf) && self.gain_db == 0.0)
    }
    fn processing_eq(&self, other: &Self) -> bool {
        self.shape == other.shape && self.frequency == other.frequency && self.gain_db == other.gain_db
            && self.q == other.q && self.channel == other.channel
    }
}
impl Preset {
    pub(super) fn processing_eq(&self, other: &Self) -> bool {
        if self.is_identity() && other.is_identity() { return true; }
        let active = |preset: &Preset, filter: &Filter| preset.eq_enabled && filter.affects_audio();
        let count = self.filters.iter().filter(|filter| active(self, filter)).count();
        let same_filters = count == other.filters.iter().filter(|filter| active(other, filter)).count()
            && self.filters.iter().filter(|filter| active(self, filter)).all(|filter| {
                self.filters.iter().filter(|candidate| active(self, candidate) && filter.processing_eq(candidate)).count()
                    == other.filters.iter().filter(|candidate| active(other, candidate) && filter.processing_eq(candidate)).count()
            });
        let tone = (&self.tone, &other.tone);
        let tone_band = |bass: bool| {
            let left = if tone.0.enabled { if bass { tone.0.bass_db } else { tone.0.treble_db } } else { 0.0 };
            let right = if tone.1.enabled { if bass { tone.1.bass_db } else { tone.1.treble_db } } else { 0.0 };
            left == right && (left == 0.0 || if bass { tone.0.bass_hz == tone.1.bass_hz && tone.0.bass_q == tone.1.bass_q }
                else { tone.0.treble_hz == tone.1.treble_hz && tone.0.treble_q == tone.1.treble_q })
        };
        let dynamics = if !self.dynamics.active(self.compressor_ratio) && !other.dynamics.active(other.compressor_ratio) { true }
            else { self.dynamics == other.dynamics && self.compressor_ratio == other.compressor_ratio && self.compressor_threshold_db == other.compressor_threshold_db };
        let limiter = !self.limiter.enabled && !other.limiter.enabled || self.limiter == other.limiter;
        same_filters && tone_band(true) && tone_band(false) && dynamics && limiter
            && self.preamp_db == other.preamp_db && self.auto_headroom == other.auto_headroom
            && self.crossfeed == other.crossfeed && self.width == other.width && self.balance == other.balance
            && self.delay_left_ms == other.delay_left_ms && self.delay_right_ms == other.delay_right_ms
            && self.invert_left == other.invert_left && self.invert_right == other.invert_right
    }
}
