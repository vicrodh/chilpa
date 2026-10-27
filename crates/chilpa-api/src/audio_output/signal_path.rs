//! I read the current engine report without opening equipment inspection.
use super::*;

impl crate::Chilpa {
    pub fn audio_signal_path(&self) -> String {
        let dsp = self.now.playing && !self.casting.active() && self.player.as_ref().is_some_and(|p| p.dsp().active());
        let gain = self.now.playing && !self.casting.active() && self.player.as_ref().is_some_and(|p| p.gain_active());
        let current = self.audio_now(dsp, gain);
        if !self.now.playing {
            return json!({"status":"idle","summary":chilpa_i18n::t("Nothing playing"),"stages":[]}).to_string();
        }
        if current["available"] != true {
            return json!({"status":"waiting","summary":chilpa_i18n::t("Waiting for output"),"stages":[]}).to_string();
        }
        let changed = current["samplesChanged"] == true;
        let cast = self.casting.active();
        let (status, summary) = if cast { ("cast", chilpa_i18n::t("Original audio")) }
            else if changed { ("processed", chilpa_i18n::t("Processing active")) }
            else if self.now.bit_perfect { ("unchanged", chilpa_i18n::t("Unchanged samples")) }
            else { ("unverified", chilpa_i18n::t("Path unverified")) };
        let processing = match (cast, dsp, gain, changed) {
            (true, _, _, _) => chilpa_i18n::t("Receiver"),
            (_, true, true, _) => chilpa_i18n::t("DSP and gain"),
            (_, true, _, _) => "DSP".into(),
            (_, _, true, _) => chilpa_i18n::t("Gain"),
            (_, _, _, true) => chilpa_i18n::t("Format conversion"),
            _ => chilpa_i18n::t("Unchanged"),
        };
        let route = self.audio_output.route.as_ref();
        let mode = if cast { chilpa_i18n::t("Physical output unreported") }
            else { match route.and_then(|r| r.observation.exclusive) {
                Some(true) => chilpa_i18n::t("Exclusive"),
                Some(false) => chilpa_i18n::t("Shared"),
                None => chilpa_i18n::t("Mode not reported"),
            } };
        let destination = current["destination"].as_str().unwrap_or_default();
        let name = self.audio_output.outputs.iter().find(|o| o["id"] == destination)
            .and_then(|o| o["label"].as_str()).unwrap_or(destination);
        json!({"status":status,"summary":summary,"detail":current["summary"],"stages":[
            {"label":chilpa_i18n::t("Source"),"value":self.now.quality.long(),"detail":current["source"]},
            {"label":chilpa_i18n::t("Processing"),"value":processing,"detail":current["processing"]},
            {"label":chilpa_i18n::t("Output"),"value":current["output"],"detail":current["readback"]},
            {"label":chilpa_i18n::t("Device"),"value":name,"mode":mode,"detail":current["readbackScope"]}
        ]}).to_string()
    }
}
