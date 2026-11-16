//! Bounded PCM processing. Guests supply parameters; the host owns the math.
mod biquad;
mod engine;
mod wire;
mod settings;
mod dynamics;
mod control;
mod response;
mod equality;
pub mod volume;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod extended_tests;

pub use engine::Processor;
pub use control::Control;
pub use settings::{Channel, Dynamics, GraphicLayout, Limiter, Tone};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: u32 = 1;
pub const MAX_FILTERS: usize = 64;
pub const MAX_PRESETS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape { Peak, LowShelf, HighShelf, LowPass, HighPass, Notch, BandPass }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Filter {
    pub shape: Shape,
    pub frequency: f64,
    pub gain_db: f64,
    pub q: f64,
    pub enabled: bool,
    pub channel: Channel,
    pub locked: bool,
    pub color: u8,
}
impl Default for Filter {
    fn default() -> Self { Self { shape: Shape::Peak, frequency: 1000.0, gain_db: 0.0,
        q: std::f64::consts::FRAC_1_SQRT_2, enabled: true, channel: Channel::All, locked: false, color: 0 } }
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
    pub graphic_layout: Option<GraphicLayout>,
    pub eq_enabled: bool,
    pub auto_headroom: bool,
    pub tone: Tone,
    pub dynamics: Dynamics,
    pub limiter: Limiter,
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
        Self { id: "custom".into(), name: "Custom".into(), category: "Personal".into(),
            description: String::new(), source: String::new(), source_url: String::new(), preamp_db: 0.0, filters: Vec::new(), crossfeed: 0.0,
            graphic_layout: None, eq_enabled: true, auto_headroom: true, tone: Tone::default(), dynamics: Dynamics::default(), limiter: Limiter::default(),
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
        if self.filters.len() > MAX_FILTERS { return Err(chilpa_i18n::t("Use at most 64 DSP filters.")); }
        for f in &self.filters {
            if !bounded(f.frequency, 10.0, 24_000.0) || !bounded(f.gain_db, -24.0, 18.0) || !bounded(f.q, 0.1, 12.0) || f.color > 15 {
                return Err(chilpa_i18n::t("DSP filters need 10–24000 Hz, −24–18 dB and Q 0.1–12."));
            }
        }
        if !bounded(self.preamp_db, -60.0, 24.0) || !bounded(self.crossfeed, 0.0, 0.5)
            || !bounded(self.width, 0.0, 1.0) || !bounded(self.balance, -1.0, 1.0)
            || !bounded(self.delay_left_ms, 0.0, 30.0) || !bounded(self.delay_right_ms, 0.0, 30.0)
            || !bounded(self.compressor_threshold_db, -90.0, 0.0) || !bounded(self.compressor_ratio, 1.0, 10.0)
            || !self.tone.validate() || !self.dynamics.validate() || !self.limiter.validate()
            || self.graphic_layout.as_ref().is_some_and(|layout| !layout.validate()) {
            return Err(chilpa_i18n::t("Invalid DSP gain, stereo, delay or dynamics settings."));
        }
        Ok(())
    }

    pub fn is_identity(&self) -> bool {
        self.preamp_db == 0.0 && (!self.eq_enabled || self.filters.iter().all(|f| !f.enabled || matches!(f.shape, Shape::Peak | Shape::LowShelf | Shape::HighShelf) && f.gain_db == 0.0))
            && self.crossfeed == 0.0 && self.width == 1.0 && self.balance == 0.0
            && self.delay_left_ms == 0.0 && self.delay_right_ms == 0.0
            && !self.invert_left && !self.invert_right && !self.dynamics.active(self.compressor_ratio)
            && self.tone.is_identity() && !self.limiter.enabled
    }


}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Labels { pub name: String, pub category: String, pub description: String, pub source: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub protocol: u32,
    pub presets: Vec<Preset>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub localizations: std::collections::BTreeMap<String, std::collections::BTreeMap<String, Labels>>,
}
impl Catalog {
    /// I localize a display copy; the processing preset keeps its identity and parameters.
    pub fn display(&self, preset: &Preset, language: &str) -> Preset {
        let primary = language.split(['-', '_']).next().unwrap_or(language);
        let mut display = preset.clone();
        if let Some(labels) = self.localizations.get(language).or_else(|| self.localizations.get(primary)).and_then(|rows| rows.get(&preset.id)) {
            if !labels.name.is_empty() { display.name = labels.name.clone(); }
            if !labels.category.is_empty() { display.category = labels.category.clone(); }
            if !labels.description.is_empty() { display.description = labels.description.clone(); }
            if !labels.source.is_empty() { display.source = labels.source.clone(); }
        }
        display
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.protocol != PROTOCOL || self.presets.is_empty() || self.presets.len() > MAX_PRESETS {
            return Err(chilpa_i18n::t("Unsupported or empty DSP preset catalog."));
        }
        let mut ids = std::collections::BTreeSet::new();
        for preset in &self.presets {
            preset.validate()?;
            if !ids.insert(&preset.id) { return Err(chilpa_i18n::t("Duplicate DSP preset ID.")); }
        }
        if self.localizations.len() > 16 { return Err(chilpa_i18n::t("Invalid DSP preset identity.")); }
        for (language, labels) in &self.localizations {
            if language.len() > 24 || language.len() < 2 || !language.bytes().all(|b| b.is_ascii_alphabetic() || matches!(b, b'-' | b'_'))
                || labels.len() > self.presets.len() { return Err(chilpa_i18n::t("Invalid DSP preset identity.")); }
            for id in labels.keys() {
                let preset = self.presets.iter().find(|preset| &preset.id == id).ok_or_else(|| chilpa_i18n::t("Invalid DSP preset identity."))?;
                self.display(preset, language).validate()?;
            }
        }
        Ok(())
    }
}
