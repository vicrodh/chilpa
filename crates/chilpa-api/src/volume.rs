//! The player bar's volume (design/02 7, design/04).
//!
//! Shared and converting routes scale samples and report a processed result.
//! A direct ALSA route cannot scale without ceasing to be direct, so it uses
//! a selected hardware mixer or leaves the player slider fixed.

use crate::Chilpa;

const VOLUME: &str = "playback.volume";
const MUTED: &str = "playback.muted";
pub(crate) const HARDWARE: &str = "advanced.hwVolume";

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
        if self.hardware_volume() {
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
                chilpa_i18n::t("Direct ALSA does not scale the samples. Use the DAC or amplifier, or enable hardware volume in Settings › Advanced.")
            };
            return VolumeView { percent: 100, muted: false, locked: true, why, ..VolumeView::default() };
        }
        VolumeView { percent, muted, ..VolumeView::default() }
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
        self.preference(HARDWARE).as_deref() == Some("true")
    }

    pub fn set_hardware_volume(&mut self, on: bool) {
        self.remember_preference(HARDWARE, &on.to_string());
        self.apply_volume();
    }

    /// What the samples are multiplied by. One when the device's knob does
    /// the work or the route is direct.
    pub(crate) fn software_gain(&mut self) -> f32 {
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
        if gain != 1.0 {
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
    fn the_slider_is_heard_on_a_curve() {
        assert_eq!(gain_of(100), 1.0);
        assert_eq!(gain_of(0), 0.0);
        assert!((gain_of(50) - 0.125).abs() < 1e-6);
    }
}
