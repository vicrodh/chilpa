use chilpa_core::dsp::{Preset, Filter, Shape};

/// I accept the parametric subset shared by common text EQ exporters.
/// Unknown active filters fail the whole import instead of sounding wrong.
pub(super) fn parse(text: &str, name: &str) -> Result<Preset, String> {
    if text.len() > 64*1024 { return Err(chilpa_i18n::t("The EQ profile is too large.")); }
    if text.trim_start().starts_with('{') {
        let preset: Preset = serde_json::from_str(text).map_err(|_| chilpa_i18n::t("Invalid DSP preset JSON."))?;
        preset.validate()?; return Ok(preset);
    }
    let mut preset = Preset { name: name.trim().into(), category: "Importados".into(), ..Preset::default() };
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let tokens: Vec<_> = line.split_whitespace().collect();
        if tokens.first() == Some(&"Preamp:") {
            if tokens.len() != 3 || tokens[2] != "dB" { return Err(chilpa_i18n::t("Invalid EQ preamp line.")); }
            preset.preamp_db = number(tokens.get(1).copied())?;
            continue;
        }
        if !line.starts_with("Filter ") {
            if ["Filter Settings file", "Room EQ ", "Dated:", "Notes:", "Equaliser:"].iter().any(|prefix| line.starts_with(prefix)) { continue; }
            return Err(chilpa_i18n::t("Unsupported EQ directive. Paste parametric filters without channel routing or other commands."));
        }
        let position = tokens.iter().position(|s| *s == "ON" || *s == "OFF").ok_or_else(|| chilpa_i18n::t("The filter has no ON/OFF state."))?;
        if tokens[position] == "OFF" { continue; }
        if tokens.get(position+2).copied() != Some("Fc") || tokens.get(position+4).copied() != Some("Hz") {
            return Err(chilpa_i18n::t("Unsupported EQ filter syntax or frequency unit."));
        }
        let shape = match tokens.get(position+1).copied() {
            Some("PK" | "PEQ") => Shape::Peak, Some("LS" | "LSC") => Shape::LowShelf,
            Some("HS" | "HSC") => Shape::HighShelf, Some("LP" | "LPQ") => Shape::LowPass,
            Some("HP" | "HPQ") => Shape::HighPass, Some("NO") => Shape::Notch,
            _ => return Err(chilpa_i18n::t("Unsupported active EQ filter. Use PK, LS, HS, LP, HP or NO.")),
        };
        let field = |key: &str| tokens.iter().position(|t| *t == key).and_then(|i| tokens.get(i+1).copied());
        let frequency = number(field("Fc"))?;
        let gain_db = if matches!(shape, Shape::Peak | Shape::LowShelf | Shape::HighShelf) { number(field("Gain"))? } else { 0.0 };
        let q = match field("Q") { Some(q) => number(Some(q))?, None if matches!(shape, Shape::LowShelf | Shape::HighShelf | Shape::LowPass | Shape::HighPass) => std::f64::consts::FRAC_1_SQRT_2,
            None => return Err(chilpa_i18n::t("An active EQ filter is missing Q.")) };
        preset.filters.push(Filter { shape, frequency, gain_db, q });
    }
    if preset.filters.is_empty() { return Err(chilpa_i18n::t("No active parametric EQ filters were found.")); }
    preset.validate()?; Ok(preset)
}
fn number(text: Option<&str>) -> Result<f64, String> {
    text.and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite()).ok_or_else(|| chilpa_i18n::t("An EQ value is missing or invalid."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_filters_preserve_frequency_gain_and_q() {
        let p = parse("Preamp: -6.2 dB\nFilter 1: ON PK Fc 105 Hz Gain 3.2 dB Q 0.71\nFilter 2: ON HSC Fc 5000 Hz Gain -2 dB Q 0.707\nFilter 3: OFF PK Fc 400 Hz Gain 99 dB Q 0", "Measured profile").unwrap();
        assert_eq!(p.filters.len(), 2); assert_eq!(p.preamp_db, -6.2);
        assert_eq!(p.filters[0].frequency, 105.0); assert_eq!(p.filters[0].gain_db, 3.2); assert_eq!(p.filters[1].shape, Shape::HighShelf);
    }
    #[test]
    fn malformed_unsupported_and_unbounded_profiles_fail_whole() {
        for text in ["", "no EQ", "Filter 1: ON PK Fc NaN Hz Gain 2 dB Q 1", "Filter 1: ON AP Fc 200 Hz Q 1",
            "Channel: L\nFilter 1: ON PK Fc 100 Hz Gain 3 dB Q 1", "Filter 1: ON LS 12dB Fc 100 Hz Gain 3 dB",
            "Filter 1: ON PK Fc 100 Hz Gain 3 dB", "Filter 1: ON PK Fc 100 Hz Gain 30 dB Q 1",
            "Preamp: +8 dB\nFilter 1: ON PK Fc 100 Hz Gain 3 dB Q 1"] { assert!(parse(text, "test").is_err(), "{text}"); }
    }
}
