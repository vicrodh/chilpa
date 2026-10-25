//! The player bar's volume (design/02 7, design/04).
//!
//! Shared and converting routes scale samples and report a processed result.
//! A direct ALSA route cannot scale without ceasing to be direct, so it uses
//! a selected hardware mixer or leaves the player slider fixed.

use crate::Chilpa;

const VOLUME: &str = "playback.volume";
const MUTED: &str = "playback.muted";
pub(crate) const HARDWARE: &str = "audio.hwVolume";
const LEGACY_HARDWARE: &str = "advanced.hwVolume";

/// What the player bar shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VolumeView {
    pub percent: u8,
    pub muted: bool,
    /// It moves the device's own knob.
    pub hardware: bool,
    /// It cannot be moved from here; `why` says why.
    pub locked: bool,
    pub why: String,
}

impl Chilpa {
    /// Where the volume is, and whether and how it can be moved.
    pub fn player_volume(&mut self) -> VolumeView {
        if self.casting.active() { return self.cast_volume(); }
        let (output, _) = self.chosen_output();
        // On a Mac with the output alone, the slider moves the device's own
        // volume and the samples stay untouched. A DAC with no volume of its
        // own falls back to scaling.
        if self.hardware_volume() || (self.stack_of(&output) == "coreaudio" && self.exclusive()) {
            let knob = chilpa_core::volume_of(&output);
            if knob.available {
                return VolumeView { percent: knob.percent, muted: knob.muted, hardware: true, ..VolumeView::default() };
            }
        }
        let (percent, muted) = (self.software_percent(), self.preference(MUTED).as_deref() == Some("true"));
        if self.stack_of(&output) == "alsa" && output.starts_with("alsa:") {
            let why = if self.hardware_volume() {
                chilpa_i18n::t("This direct output has no hardware volume control. Use the DAC or amplifier.")
            } else {
                chilpa_i18n::t("Direct ALSA does not scale the samples. Use the DAC or amplifier, or enable hardware volume in Settings › Audio.")
            };
            return VolumeView { percent: 100, muted: false, locked: true, why, ..VolumeView::default() };
        }
        if self.pipewire_one_bit() {
            return VolumeView {
                percent: 100, muted: false, locked: true,
                why: chilpa_i18n::t("Native DSD bypasses software volume. Use the DAC or amplifier; your PCM volume is kept."),
                ..VolumeView::default()
            };
        }
        VolumeView { percent, muted, ..VolumeView::default() }
    }

    fn pipewire_one_bit(&self) -> bool {
        self.now.playing && self.dsp.passthrough && self.stack_of(&self.chosen_output().0) == "pipewire"
    }

    pub fn set_player_volume(&mut self, percent: u8) {
        if self.casting.active() { self.cast_set_volume(percent); return; }
        let percent = percent.min(100);
        let view = self.player_volume();
        if view.locked {
            return;
        }
        if view.hardware {
            let (output, _) = self.chosen_output();
            if let Err(e) = chilpa_core::set_volume(&output, percent) {
                self.now.error = e;
            }
            return;
        }
        self.remember_preference(VOLUME, &percent.to_string());
        self.apply_volume();
    }

    pub fn set_player_muted(&mut self, muted: bool) {
        if self.casting.active() { self.cast_set_muted(muted); return; }
        let view = self.player_volume();
        if view.locked {
            return;
        }
        if view.hardware {
            let (output, _) = self.chosen_output();
            if let Err(e) = chilpa_core::set_muted(&output, muted) {
                self.now.error = e;
            }
            return;
        }
        self.remember_preference(MUTED, &muted.to_string());
        self.apply_volume();
    }

    pub fn hardware_volume(&self) -> bool {
        let (output, _) = self.chosen_output();
        self.stack_of(&output) == "alsa" && self.alsa_plugin() == "hw"
            && self.preference(HARDWARE).or_else(|| self.preference(LEGACY_HARDWARE)).as_deref() == Some("true")
    }

    pub fn set_hardware_volume(&mut self, on: bool) {
        let (output, _) = self.chosen_output();
        #[cfg(target_os = "linux")]
        let eligible = on && self.stack_of(&output) == "alsa" && self.alsa_plugin() == "hw";
        #[cfg(target_os = "linux")]
        let enabled = eligible && match chilpa_core::activate_alsa_hardware_volume(&output) {
            Ok(()) => true,
            Err(error) => {
                self.now.error = error;
                false
            }
        };
        #[cfg(not(target_os = "linux"))]
        let enabled = {
            let _ = (on, output);
            false
        };
        self.remember_preference(HARDWARE, &enabled.to_string());
        self.apply_volume();
    }

    /// What the samples are multiplied by. One when the device's knob does
    /// the work or the route is direct.
    pub(crate) fn software_gain(&mut self) -> f32 {
        // Keep the PCM preference ready for the next track or output. The
        // queue bypasses it on one-bit streams; the locked slider is a view,
        // not a replacement for the saved PCM volume or mute setting.
        if self.pipewire_one_bit() {
            return if self.preference(MUTED).as_deref() == Some("true") { 0.0 } else { gain_of(self.software_percent()) };
        }
        let view = self.player_volume();
        if view.hardware || view.locked {
            return 1.0;
        }
        if view.muted {
            return 0.0;
        }
        gain_of(view.percent)
    }

    /// Hands the player the volume as it stands, and says at once that the
    /// samples are no longer untouched when it scales them.
    pub(crate) fn apply_volume(&mut self) {
        let gain = self.software_gain();
        if let Some(p) = &self.player {
            p.set_volume(gain);
        }
        self.mark_scaled(gain);
    }

    /// A track that plays at a scaled volume is not bit-perfect, from the
    /// moment it is scaled; the verdict at its start could not know.
    pub(crate) fn mark_scaled(&mut self, gain: f32) {
        if gain != 1.0 && !self.dsp.passthrough {
            self.now.bit_perfect = false;
        }
    }

    fn software_percent(&self) -> u8 {
        self.preference(VOLUME).and_then(|v| v.parse().ok()).unwrap_or(100).min(100)
    }
}

/// The factor a slider position stands for. Cubed, because loudness is
/// heard on a curve: halfway down is about 18 dB down, not 6.
pub fn gain_of(percent: u8) -> f32 {
    let p = f32::from(percent.min(100)) / 100.0;
    p * p * p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipewire_one_bit_volume_keeps_pcm_preferences_across_route_changes() {
        const CHILD: &str = "CHILPA_TEST_PIPEWIRE_VOLUME";
        if std::env::var(CHILD).as_deref() != Ok("1") {
            let profile = tempfile::tempdir().unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "volume::tests::pipewire_one_bit_volume_keeps_pcm_preferences_across_route_changes", "--nocapture"])
                .env(CHILD, "1").envs(chilpa_dirs::profile_environment(profile.path())).output().unwrap();
            assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
            assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"));
            return;
        }
        let mut api = Chilpa::new();
        api.remember("pipewire:volume-fixture", false);
        api.set_player_volume(40);
        let gain = gain_of(40);
        api.now.playing = true;
        api.now.bit_perfect = true;
        api.dsp.passthrough = true;
        let view = api.player_volume();
        assert!(view.locked && !view.why.is_empty());
        assert_eq!((view.percent, view.muted), (100, false));
        assert_eq!(api.software_gain(), gain, "PCM gain stays ready for the next track");
        api.set_player_volume(75);
        api.set_player_muted(true);
        api.apply_volume();
        assert!(api.now.bit_perfect, "a bypassed gain does not change the one-bit verdict");
        api.dsp.passthrough = false;
        let pcm = api.player_volume();
        assert_eq!((pcm.percent, pcm.muted, pcm.locked), (40, false, false));
        assert_eq!(api.software_gain(), gain);
        api.set_player_muted(true);
        assert_eq!(api.software_gain(), 0.0);
        api.dsp.passthrough = true;
        assert!(!api.player_volume().muted, "software mute is bypassed on native DSD");
        assert_eq!(api.software_gain(), 0.0, "the saved PCM mute is retained");
        api.dsp.passthrough = false;
        assert!(api.player_volume().muted);
        api.now.playing = false;
        api.dsp.passthrough = true;
        assert!(!api.player_volume().locked, "an old stream cannot lock a stopped player");
    }

    #[test]
    fn the_slider_is_heard_on_a_curve() {
        assert_eq!(gain_of(100), 1.0);
        assert_eq!(gain_of(0), 0.0);
        assert!((gain_of(50) - 0.125).abs() < 1e-6);
    }
}
