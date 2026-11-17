//! Explicit PCM channel policy, rendered by the existing Settings controls.
use crate::Chilpa;
use sha2::{Digest,Sha256};
pub(crate) const CHANNEL_FALLBACK: &str = "audio.channelFallback";
const SAVED: &str = "audio.channelFallbackOutputs";
const MAX_OUTPUTS: usize = 64;
pub(crate) fn identity(output: &str) -> String { format!("{:x}",Sha256::digest(output.as_bytes())) }
fn saved(value: Option<&str>) -> Result<Vec<String>,String> {
    let Some(value)=value else { return Ok(Vec::new()) };
    let invalid=||chilpa_i18n::t("Stored stereo conversion choices could not be read.");
    if value.len()>MAX_OUTPUTS*68+2 { return Err(invalid()); }
    let outputs:Vec<String>=serde_json::from_str(value).map_err(|_|invalid())?;
    if outputs.len()>MAX_OUTPUTS || outputs.iter().any(|id|id.len()!=64 || !id.bytes().all(|c|c.is_ascii_hexdigit())) { return Err(invalid()); }
    Ok(outputs)
}
pub(crate) const CHANNEL_MODE: &str = "audio.channelMode";
const CHOICES: [(&str, &str); 2] = [
    ("original", chilpa_i18n::mark("Original channels")),
    ("stereo", chilpa_i18n::mark("Convert to stereo")),
];
impl Chilpa {
    pub(crate) fn remembered_stereo_fallback(&self, output: &str) -> bool {
        saved(self.preference(SAVED).as_deref()).is_ok_and(|outputs|outputs.contains(&identity(output)))
    }
    pub(crate) fn keep_stereo_fallback(&mut self, output: &str, enabled: bool) -> Result<(),String> {
        if output.is_empty() || output.len()>2048 { return Err(chilpa_i18n::t("Choose an audio output before remembering stereo conversion.")); }
        let id=identity(output);
        chilpa_core::settings::update_value(SAVED,|previous| {
            let mut outputs=saved(previous)?;
            if enabled && !outputs.contains(&id) {
                if outputs.len()==MAX_OUTPUTS { return Err(chilpa_i18n::t("Too many outputs have remembered stereo conversion. Forget one in Audio settings.")); }
                outputs.push(id);
            } else if !enabled { outputs.retain(|value|value!=&id); }
            Ok((serde_json::to_string(&outputs).map_err(|error|error.to_string())?,()))
        })
    }
    pub(crate) fn stereo_downmix_enabled(&self) -> bool {
        self.preference(CHANNEL_MODE).as_deref() == Some("stereo")
    }
    pub(crate) fn channel_mode_setting(&mut self, key: &str, value: &str) -> bool {
        if key == CHANNEL_FALLBACK {
            if !["true","false"].contains(&value) { return true; }
            let (output,_)=self.chosen_output();
            if self.remembered_stereo_fallback(&output)==(value=="true") { return true; }
            if let Err(error)=self.keep_stereo_fallback(&output,value=="true") { self.now.error=error;return true; }
            if value=="true" && self.accept_pending_channel_offer() { return true; }
            self.reset_channel_consent();
            self.apply_playback_options(false);
            self.move_output(&output,false);
            return true;
        }
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
        if value=="stereo" && self.accept_pending_channel_offer() { return true; }
        self.reset_channel_consent();
        if let Some(player) = &self.player {
            player.set_stereo_downmix(value == "stereo");
        }
        let (output, _) = self.chosen_output();
        self.move_output(&output, false);
        true
    }
    pub(crate) fn channel_mode_row(&self, row: &mut crate::SettingsRow) -> bool {
        if row.key == CHANNEL_FALLBACK {
            row.value=self.remembered_stereo_fallback(&self.chosen_output().0).to_string();
            return true;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_output_permissions_are_strict_and_bounded() {
        assert!(saved(None).unwrap().is_empty());
        let valid:Vec<_>=(0..64).map(|index|format!("{index:064x}")).collect();
        assert_eq!(saved(Some(&serde_json::to_string(&valid).unwrap())).unwrap(),valid);
        for invalid in ["{}".into(),"null".into(),"[\"not-a-hash\"]".into(),serde_json::to_string(&vec!["a".repeat(64);65]).unwrap()," ".repeat(5000)] {
            assert!(saved(Some(&invalid)).is_err(),"invalid permission list was accepted");
        }
    }
}
