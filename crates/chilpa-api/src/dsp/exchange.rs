//! Bounded interchange keeps whole imports atomic and rejects ambiguous transfer curves.
use super::*;
use chilpa_core::dsp::{Channel, Filter, Shape};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle { format: String, version: u32, presets: Vec<Preset> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct External {
    #[serde(default)] name: String,
    preamp: f64,
    parametric: bool,
    bands: Vec<ExternalBand>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalBand {
    #[serde(rename = "type")] kind: u32,
    channels: u32, frequency: f64, q: f64, gain: f64,
    #[serde(default)] color: i64,
}
pub(super) fn parse_many(text: &str, name: &str) -> Result<Vec<Preset>, String> {
    if text.len() > 2 * 1024 * 1024 { return Err(chilpa_i18n::t("DSP import is too large.")); }
    let text = text.trim();
    let presets = if text.starts_with(['{', '[']) {
        let value: Value = serde_json::from_str(text).map_err(|_| chilpa_i18n::t("Invalid DSP preset JSON."))?;
        if value.get("format").is_some() {
            let bundle: Bundle = serde_json::from_value(value).map_err(|_| chilpa_i18n::t("Invalid DSP preset bundle."))?;
            if bundle.format != "chilpa-dsp" || bundle.version != 2 { return Err(chilpa_i18n::t("Unsupported DSP preset bundle.")); }
            bundle.presets
        } else {
            let values = if let Value::Array(values) = value { values } else { vec![value] };
            if values.len() > 64 { return Err(chilpa_i18n::t("Import at most 64 presets at once.")); }
            values.into_iter().map(|value| if value.get("bands").is_some() { external(value, name) }
                else { serde_json::from_value(value).map_err(|_| chilpa_i18n::t("Invalid DSP preset JSON.")) }).collect::<Result<_, _>>()?
        }
    } else { vec![import::parse(text, name)?] };
    if presets.is_empty() || presets.len() > 64 { return Err(chilpa_i18n::t("Import between 1 and 64 presets.")); }
    for preset in &presets { preset.validate()?; }
    Ok(presets)
}
fn external(value: Value, fallback: &str) -> Result<Preset, String> {
    let source: External = serde_json::from_value(value).map_err(|_| chilpa_i18n::t("Invalid external EQ JSON."))?;
    if !source.parametric || !(2..=66).contains(&source.bands.len()) {
        return Err(chilpa_i18n::t("Use a parametric export with two tone bands and at most 64 EQ bands."));
    }
    let mut preset = Preset { name: if source.name.trim().is_empty() { fallback.into() } else { source.name },
        preamp_db: source.preamp, category: "Imported".into(), ..Preset::default() };
    for (index, band) in source.bands.into_iter().enumerate() {
        if !band.gain.is_finite() || !(-15.0..=15.0).contains(&band.gain) { return Err(chilpa_i18n::t("Invalid external EQ gain.")); }
        let channel = match band.channels { 0 => Channel::All, 1 => Channel::Left, 2 => Channel::Right,
            _ => return Err(chilpa_i18n::t("Unsupported external EQ channel.")) };
        if index < 2 {
            if band.kind != index as u32 || channel != Channel::All || band.gain < 0.0 {
                return Err(chilpa_i18n::t("Unsupported external tone routing or type."));
            }
            if index == 0 { preset.tone.bass_db = band.gain * 1.5; preset.tone.bass_hz = band.frequency; preset.tone.bass_q = band.q; }
            else { preset.tone.treble_db = band.gain; preset.tone.treble_hz = band.frequency; preset.tone.treble_q = band.q; }
        } else {
            let shape = match band.kind { 0 => Shape::LowShelf, 1 => Shape::HighShelf, 2 => Shape::Peak,
                _ => return Err(chilpa_i18n::t("External JSON supports peak and shelf EQ bands. Pass-filter gain semantics are not transferable.")) };
            preset.filters.push(Filter { shape, channel, frequency: band.frequency, q: band.q,
                gain_db: if band.gain < 0.0 { band.gain * 0.65 } else { band.gain },
                color: band.color.unsigned_abs().wrapping_rem(16) as u8, ..Filter::default() });
        }
    }
    preset.validate()?;
    Ok(preset)
}

impl Chilpa {
    pub fn dsp_export(&mut self, input: &str) -> String {
        let result = (|| {
            if input.len() > 16 * 1024 { return Err(chilpa_i18n::t("DSP export selection is too large.")); }
            let ids: Vec<String> = serde_json::from_str(input).map_err(|_| chilpa_i18n::t("Invalid DSP export selection."))?;
            if ids.len() > 64 { return Err(chilpa_i18n::t("Export at most 64 presets at once.")); }
            let selected = self.dsp.selection(&self.chosen_output().0);
            let presets = if ids.is_empty() { vec![self.dsp.preset(&selected).ok_or_else(|| chilpa_i18n::t("Choose a DSP preset."))?] }
            else { ids.iter().map(|id| self.dsp.saved.custom.iter().find(|preset| &preset.id == id).cloned()
                .or_else(|| self.dsp.browser.selected(id, &selected.plugin, &self.dsp.signature))
                .or_else(|| self.dsp.providers.iter().flat_map(|provider| &provider.catalog.presets).find(|preset| &preset.id == id).cloned())
                .ok_or_else(|| chilpa_i18n::t("DSP preset is unavailable."))).collect::<Result<Vec<_>, _>>()? };
            serde_json::to_string_pretty(&json!({"format":"chilpa-dsp","version":2,"presets":presets})).map_err(|error| error.to_string())
        })();
        match result { Ok(text) => text, Err(error) => { self.dsp.error = error; String::new() } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_roundtrip_preserves_tone_channels_and_rejects_partial_imports() {
        let mut first = Preset::default(); first.tone.bass_db = 4.0;
        first.filters.push(Filter { channel: Channel::Left, locked: true, ..Filter::default() });
        let text = json!({"format":"chilpa-dsp","version":2,"presets":[first, Preset::default()]}).to_string();
        let roundtrip = parse_many(&text, "Imported").unwrap();
        assert_eq!(roundtrip[0], first); assert_eq!(roundtrip.len(), 2);
        assert!(parse_many(r#"[{"name":"Good"},{"name":"Bad","tone":{"bass_db":99}}]"#, "Imported").is_err());
    }
    #[test]
    fn external_transfer_uses_normalized_gain_and_keeps_tone_separate() {
        let text = r#"{"name":"Transfer","preamp":-6,"parametric":true,"bands":[{"type":0,"channels":0,"frequency":90,"q":0.8,"gain":10,"color":0},{"type":1,"channels":0,"frequency":10000,"q":0.6,"gain":3,"color":0},{"type":2,"channels":1,"frequency":1000,"q":1,"gain":-10,"color":0}]}"#;
        let preset = parse_many(text, "Imported").unwrap().remove(0);
        assert_eq!(preset.tone.bass_db, 15.0); assert_eq!(preset.tone.treble_db, 3.0);
        assert_eq!(preset.filters[0].gain_db, -6.5); assert_eq!(preset.filters[0].channel, Channel::Left);
        assert!(parse_many(&text.replace("\"type\":2", "\"type\":3"), "Imported").is_err());
    }
}
