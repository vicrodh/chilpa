//! Explicit PCM channel policy, rendered by the existing Settings controls.
use crate::Chilpa;
pub(crate) const CHANNEL_MODE: &str = "audio.channelMode";
const CHOICES: [(&str, &str); 2] = [
    ("original", chilpa_i18n::mark("Original channels")),
    ("stereo", chilpa_i18n::mark("Convert to stereo")),
];
impl Chilpa {
    pub(crate) fn stereo_downmix_enabled(&self) -> bool {
        self.preference(CHANNEL_MODE).as_deref() == Some("stereo")
    }
    pub(crate) fn channel_mode_setting(&mut self, key: &str, value: &str) -> bool {
        if key != CHANNEL_MODE {
            return false;
        }
        if !CHOICES.iter().any(|(choice, _)| *choice == value) {
            return true;
        }
        let saved = self.preference(CHANNEL_MODE);
        if saved.as_deref() == Some(value) || (saved.is_none() && value == "original") {
            return true;
        }
        if let Err(error) = self.remember_preference_checked(key, value) {
            self.now.error = error;
            return true;
        }
        if let Some(player) = &self.player {
            player.set_stereo_downmix(value == "stereo");
        }
        let (output, _) = self.chosen_output();
        self.move_output(&output, false);
        true
    }
    pub(crate) fn channel_mode_row(&self, row: &mut crate::SettingsRow) -> bool {
        if row.key != CHANNEL_MODE {
            return false;
        }
        for (value, label) in CHOICES {
            row.values.push(value.into());
            row.options.push(chilpa_i18n::t(label));
        }
        row.value = if self.stereo_downmix_enabled() {
            "stereo"
        } else {
            "original"
        }
        .into();
        true
    }
}
