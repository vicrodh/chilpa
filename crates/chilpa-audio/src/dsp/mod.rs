//! Bounded PCM processing. Guests supply parameters; the host owns the math.
mod biquad;
mod engine;
mod wire;
#[cfg(test)]
mod tests;

pub use engine::{Control, Processor};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: u32 = 1;
pub const MAX_FILTERS: usize = 10;
pub const MAX_PRESETS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape { Peak, LowShelf, HighShelf, LowPass, HighPass, Notch }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub shape: Shape,
    pub frequency: f64,
    pub gain_db: f64,
    pub q: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub source: String,
    pub source_url: String,
    pub preamp_db: f64,
    pub filters: Vec<Filter>,
    pub crossfeed: f64,
    pub width: f64,
    pub balance: f64,
    pub delay_left_ms: f64,
    pub delay_right_ms: f64,
    pub invert_left: bool,
    pub invert_right: bool,
    pub compressor_threshold_db: f64,
    pub compressor_ratio: f64,
}

impl Default for Preset {
    fn default() -> Self {
        Self { id: "custom".into(), name: "Personalizado".into(), category: "Personal".into(),
            description: String::new(), source: String::new(), source_url: String::new(), preamp_db: 0.0, filters: Vec::new(), crossfeed: 0.0,
            width: 1.0, balance: 0.0, delay_left_ms: 0.0, delay_right_ms: 0.0,
            invert_left: false, invert_right: false, compressor_threshold_db: -20.0, compressor_ratio: 1.0 }
    }
}

fn bounded(value: f64, low: f64, high: f64) -> bool { value.is_finite() && (low..=high).contains(&value) }

impl Preset {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.len() > 80 || !self.id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            || self.name.trim().is_empty() || self.name.len() > 160 || self.category.len() > 80 || self.description.len() > 800
            || self.source.len() > 256 || self.source_url.len() > 2048 || (!self.source_url.is_empty() && !self.source_url.starts_with("https://")) {
            return Err(chilpa_i18n::t("Invalid DSP preset identity."));
        }
        if self.filters.len() > MAX_FILTERS { return Err(chilpa_i18n::t("Use at most 10 DSP filters.")); }
        for f in &self.filters {
            if !bounded(f.frequency, 10.0, 24_000.0) || !bounded(f.gain_db, -18.0, 12.0) || !bounded(f.q, 0.2, 12.0) {
                return Err(chilpa_i18n::t("DSP filters need 10–24000 Hz, −18–12 dB and Q 0.2–12."));
            }
        }
        if !bounded(self.preamp_db, -60.0, 0.0) || !bounded(self.crossfeed, 0.0, 0.5)
            || !bounded(self.width, 0.0, 1.0) || !bounded(self.balance, -1.0, 1.0)
            || !bounded(self.delay_left_ms, 0.0, 30.0) || !bounded(self.delay_right_ms, 0.0, 30.0)
            || !bounded(self.compressor_threshold_db, -60.0, 0.0) || !bounded(self.compressor_ratio, 1.0, 6.0) {
            return Err(chilpa_i18n::t("Invalid DSP gain, stereo, delay or dynamics settings."));
        }
        Ok(())
    }

    pub fn is_identity(&self) -> bool {
        self.preamp_db == 0.0 && self.filters.iter().all(|f| matches!(f.shape, Shape::Peak | Shape::LowShelf | Shape::HighShelf) && f.gain_db == 0.0)
            && self.crossfeed == 0.0 && self.width == 1.0 && self.balance == 0.0
            && self.delay_left_ms == 0.0 && self.delay_right_ms == 0.0
            && !self.invert_left && !self.invert_right && self.compressor_ratio == 1.0
    }

    /// The EQ's combined response reserves headroom without summing boosts
    /// at unrelated frequencies. A final sample guard catches transients.
    pub fn effective_preamp_db(&self) -> f64 {
        self.preamp_for_rate(48_000)
    }

    pub fn preamp_for_rate(&self, rate: u32) -> f64 {
        if self.is_identity() { return 0.0; }
        let rate = rate.max(1000);
        let filters: Vec<_> = self.filters.iter().map(|f| biquad::Biquad::new(f, rate)).collect();
        let top = f64::from(rate) * 0.49;
        let response = |hz| filters.iter().map(|f| f.magnitude_db(hz, rate)).sum::<f64>();
        let grid = (0..=2048).map(|i| 1.0 * top.powf(i as f64 / 2048.0));
        let centers = self.filters.iter().map(|f| f.frequency.min(top));
        let boost = grid.chain(centers).map(response).fold(0.0, f64::max);
        self.preamp_db.min(-1.0 - boost)
    }

    /// Display-only EQ response, before the automatic preamp and stereo tools.
    pub fn curve(&self, rate: u32) -> Vec<f64> {
        let filters: Vec<_> = self.filters.iter().map(|f| biquad::Biquad::new(f, rate.max(1000))).collect();
        (0..=128).map(|i| {
            let hz = 20.0 * 1000_f64.powf(i as f64/128.0);
            if hz >= f64::from(rate)*0.49 { return 0.0; }
            filters.iter().map(|f| f.magnitude_db(hz, rate)).sum::<f64>()
        }).collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog { pub protocol: u32, pub presets: Vec<Preset> }
impl Catalog {
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol != PROTOCOL || self.presets.is_empty() || self.presets.len() > MAX_PRESETS {
            return Err(chilpa_i18n::t("Unsupported or empty DSP preset catalog."));
        }
        let mut ids = std::collections::BTreeSet::new();
        for preset in &self.presets {
            preset.validate()?;
            if !ids.insert(&preset.id) { return Err(chilpa_i18n::t("Duplicate DSP preset ID.")); }
        }
        Ok(())
    }
}
