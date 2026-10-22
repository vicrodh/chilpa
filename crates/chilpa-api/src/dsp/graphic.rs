//! I keep the previous EQ while the graphic editor changes real peak filters.
use super::{Preset, Selection};
use chilpa_core::dsp::{Filter, Shape};
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
    preset.response_db(&FREQUENCIES, rate)
}

pub(super) fn matches(preset: &Preset) -> bool {
    preset.filters.len() == FREQUENCIES.len()
        && preset.filters.iter().zip(FREQUENCIES).all(|(filter, frequency)| {
            filter.shape == Shape::Peak && (filter.frequency - frequency).abs() < 0.001
                && (filter.q - std::f64::consts::SQRT_2).abs() < 0.001
        })
}

impl Selection {
    pub(super) fn begin_graphic(&mut self, mut preset: Preset) {
        if self.graphic_origin.is_none() {
            self.graphic_origin = Some(Origin {
                preset: self.preset.clone(), edited: self.edited.clone(), enabled: self.enabled,
            });
        }
        preset.name = chilpa_i18n::t("Graphic equalizer");
        preset.filters = FREQUENCIES.into_iter().map(|frequency| Filter {
            shape: Shape::Peak, frequency, gain_db: 0.0, q: std::f64::consts::SQRT_2,
        }).collect();
        self.edited = Some(preset);
        self.enabled = true;
    }

    pub(super) fn restore_graphic(&mut self) -> Result<(), String> {
        let origin = self.graphic_origin.take().ok_or_else(|| chilpa_i18n::t("There is no previous EQ to restore."))?;
        self.preset = origin.preset;
        self.edited = origin.edited;
        self.enabled = origin.enabled;
        Ok(())
    }
}
