//! Volume normalization's own settings, QBZ's (`settings_qt.rs`,
//! `NORMALIZATION_TARGETS`): the loudness every track is aligned to and the
//! clipping guard. They live here and reach the player through the facade,
//! so the bridge learns no key (frontend ADR-0015). The switch itself is
//! `playback.normalize`, in the queue.

use crate::Chilpa;

pub const TARGET: &str = "playback.normalizeTarget";
pub const PREVENT_CLIPPING: &str = "playback.preventClipping";

/// The targets offered, value and label, in QBZ's order. A label names a
/// use, never a service (ADR-004).
pub const TARGET_CHOICES: [(&str, &str); 5] = [
    ("-11", chilpa_i18n::mark("-11 LUFS · Loud")),
    ("-14", chilpa_i18n::mark("-14 LUFS · Streaming (default)")),
    ("-16", chilpa_i18n::mark("-16 LUFS")),
    ("-18", chilpa_i18n::mark("-18 LUFS · ReplayGain")),
    ("-23", chilpa_i18n::mark("-23 LUFS · Broadcast (EBU R128)")),
];

impl Chilpa {
    /// The loudness tracks are aligned to, in LUFS.
    pub fn normalize_target(&self) -> f32 {
        self.preference(TARGET)
            .filter(|value| TARGET_CHOICES.iter().any(|(choice, _)| choice == value))
            .and_then(|value| value.parse().ok())
            .unwrap_or(chilpa_core::loudness::DEFAULT_TARGET)
    }

    /// Whether a boost stops where the loudest peak would pass full scale.
    pub fn prevent_clipping(&self) -> bool {
        self.preference(PREVENT_CLIPPING).as_deref() != Some("false")
    }

    pub(crate) fn normalization_setting(&mut self, key: &str, value: &str) -> bool {
        let valid = match key {
            TARGET => TARGET_CHOICES.iter().any(|(choice, _)| *choice == value),
            PREVENT_CLIPPING => matches!(value, "true" | "false"),
            _ => return false,
        };
        if valid {
            self.remember_preference(key, value);
            self.apply_playback_options(false);
        }
        true
    }

    /// The two rows, shown only while normalization is on, as in QBZ.
    pub(crate) fn normalization_row(&self, row: &mut crate::SettingsRow) -> bool {
        if row.key != TARGET && row.key != PREVENT_CLIPPING {
            return false;
        }
        if !self.normalize() {
            row.control = String::new();
            return true;
        }
        if row.key == TARGET {
            for (value, label) in TARGET_CHOICES {
                row.values.push(value.to_string());
                row.options.push(chilpa_i18n::t(label));
            }
            row.value = format!("{}", self.normalize_target());
        } else {
            row.value = self.prevent_clipping().to_string();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_offered_is_one_the_player_knows() {
        for (value, _) in TARGET_CHOICES {
            let lufs: f32 = value.parse().unwrap();
            assert!(chilpa_core::loudness::TARGETS.contains(&lufs), "{value}");
            assert_eq!(format!("{lufs}"), value, "the row reads back what it offered");
        }
    }
}
