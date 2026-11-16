//! Optional processing stages default to neutral for older presets.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel { #[default] All, Left, Right }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GraphicLayout { pub bands: usize, pub minimum_hz: f64, pub maximum_hz: f64 }
impl Default for GraphicLayout {
    fn default() -> Self { Self { bands: 10, minimum_hz: 31.25, maximum_hz: 16_000.0 } }
}
impl GraphicLayout {
    pub fn validate(&self) -> bool {
        (5..=32).contains(&self.bands) && super::bounded(self.minimum_hz, 20.0, 1000.0)
            && super::bounded(self.maximum_hz, 5000.0, 24_000.0) && self.minimum_hz < self.maximum_hz
            && self.q() <= 12.0
    }
    pub fn frequencies(&self) -> Vec<f64> {
        if !self.validate() { return Vec::new(); }
        (0..self.bands).map(|index| self.minimum_hz * (self.maximum_hz / self.minimum_hz).powf(index as f64 / (self.bands - 1) as f64)).collect()
    }
    pub fn q(&self) -> f64 {
        let ratio = (self.maximum_hz / self.minimum_hz).powf(1.0 / self.bands.saturating_sub(1).max(1) as f64);
        ratio.sqrt() / (ratio - 1.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tone {
    pub enabled: bool,
    pub bass_db: f64,
    pub bass_hz: f64,
    pub bass_q: f64,
    pub treble_db: f64,
    pub treble_hz: f64,
    pub treble_q: f64,
}
impl Default for Tone {
    fn default() -> Self {
        Self { enabled: true, bass_db: 0.0, bass_hz: 90.0, bass_q: 0.8,
            treble_db: 0.0, treble_hz: 10_000.0, treble_q: 0.6 }
    }
}
impl Tone {
    pub(super) fn validate(&self) -> bool {
        super::bounded(self.bass_db, -24.0, 22.5) && super::bounded(self.treble_db, -24.0, 15.0)
            && super::bounded(self.bass_hz, 20.0, 250.0) && super::bounded(self.treble_hz, 5000.0, 15_000.0)
            && super::bounded(self.bass_q, 0.1, 2.0) && super::bounded(self.treble_q, 0.1, 2.0)
    }
    pub fn is_identity(&self) -> bool { !self.enabled || (self.bass_db == 0.0 && self.treble_db == 0.0) }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dynamics {
    pub enabled: bool,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub knee_db: f64,
    pub pre_gain_db: f64,
    pub makeup_db: f64,
    pub sidechain_hpf_hz: f64,
    pub gate_enabled: bool,
    pub gate_threshold_db: f64,
    pub expander_ratio: f64,
}
impl Default for Dynamics {
    fn default() -> Self {
        Self { enabled: true, attack_ms: 5.0, release_ms: 180.0, knee_db: 0.0,
            pre_gain_db: 0.0, makeup_db: 0.0, sidechain_hpf_hz: 0.0, gate_enabled: false,
            gate_threshold_db: -90.0, expander_ratio: 1.0 }
    }
}
impl Dynamics {
    pub(super) fn validate(&self) -> bool {
        super::bounded(self.attack_ms, 1.0, 120.0) && super::bounded(self.release_ms, 1.0, 500.0)
            && super::bounded(self.knee_db, 0.0, 20.0) && super::bounded(self.pre_gain_db, 0.0, 45.0)
            && super::bounded(self.makeup_db, 0.0, 45.0) && super::bounded(self.sidechain_hpf_hz, 0.0, 20_000.0)
            && super::bounded(self.gate_threshold_db, -90.0, 0.0) && super::bounded(self.expander_ratio, 1.0, 10.0)
    }
    pub(super) fn active(&self, ratio: f64) -> bool {
        self.enabled && (ratio > 1.0 || self.expander_ratio > 1.0 || self.gate_enabled || self.pre_gain_db != 0.0 || self.makeup_db != 0.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limiter { pub enabled: bool, pub ceiling_db: f64, pub release_ms: f64 }
impl Default for Limiter {
    fn default() -> Self { Self { enabled: false, ceiling_db: -1.0, release_ms: 50.0 } }
}
impl Limiter {
    pub(super) fn validate(&self) -> bool {
        super::bounded(self.ceiling_db, -24.0, 0.0) && super::bounded(self.release_ms, 1.0, 500.0)
    }
}
