//! I keep the previous EQ while the graphic editor changes real peak filters.
use super::{Preset, Selection};
use chilpa_core::dsp::{Filter, Shape, GraphicLayout, Channel};
use serde::{Deserialize, Serialize};

const FREQUENCIES: [f64; 10] = [31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Origin {
    preset: String,
    edited: Option<Preset>,
    enabled: bool,
}
impl Origin {
    pub(super) fn valid(&self) -> bool {
        !self.preset.is_empty() && self.preset.len() <= 80
            && self.preset.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            && self.edited.as_ref().is_none_or(|preset| preset.validate().is_ok())
    }
}

/// What each band of the graphic editor reads for any preset: its combined
/// response at the band's frequency, so a preset of precise filters shows its
/// shape there too.
pub(super) fn levels(preset: &Preset, rate: u32) -> Vec<f64> {
    preset.response_db(&frequencies(preset), rate)
}

pub(super) fn frequencies(preset: &Preset) -> Vec<f64> {
    preset.graphic_layout.as_ref().map_or_else(|| FREQUENCIES.to_vec(), GraphicLayout::frequencies)
}

pub(super) fn matches(preset: &Preset) -> bool {
    if preset.graphic_layout.as_ref().is_some_and(|layout| !layout.validate()) { return false; }
    let frequencies = frequencies(preset);
    let q = preset.graphic_layout.as_ref().map_or(std::f64::consts::SQRT_2, GraphicLayout::q);
    preset.filters.len() == frequencies.len()
        && preset.filters.iter().zip(frequencies).all(|(filter, frequency)| {
            filter.shape == Shape::Peak && filter.enabled && filter.channel == Channel::All && !filter.locked
                && (filter.frequency - frequency).abs() < 0.001 && (filter.q - q).abs() < 0.001
        })
}

impl Selection {
    pub(super) fn begin_graphic(&mut self, preset: Preset) {
        self.configure_graphic(preset, GraphicLayout::default());
    }
    pub(super) fn configure_graphic(&mut self, mut preset: Preset, layout: GraphicLayout) {
        if self.graphic_origin.is_none() {
            self.graphic_origin = Some(Origin {
                preset: self.preset.clone(), edited: self.edited.clone(), enabled: self.enabled,
            });
        }
        preset.name = chilpa_i18n::t("Graphic equalizer");
        preset.filters = layout.frequencies().into_iter().map(|frequency| Filter {
            shape: Shape::Peak, frequency, gain_db: 0.0, q: layout.q(), ..Default::default() }).collect();
        preset.graphic_layout = Some(layout);
        self.edited = Some(preset);
        self.enabled = true;
    }

    pub(super) fn restore_graphic(&mut self) -> Result<(), String> {
        let origin = self.graphic_origin.take().ok_or_else(|| chilpa_i18n::t("There is no previous EQ to restore."))?;
        if !origin.valid() { return Err(chilpa_i18n::t("The previous EQ parameters are invalid.")); }
        self.preset = origin.preset;
        self.edited = origin.edited;
        self.enabled = origin.enabled;
        Ok(())
    }
}
