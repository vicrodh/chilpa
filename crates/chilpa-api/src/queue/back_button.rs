//! What the back button does, and the setting that picks it.
//!
//! Most players share one habit: a press goes back to the start of the track
//! that plays, and a second press right after goes to the track before it.
//! That is the default here. The other choice always goes to the previous
//! track. The setting lives in the facade, so the bridge learns no key
//! (frontend ADR-0015).

use std::time::{Duration, Instant};

use crate::Chilpa;

pub const BACK: &str = "playback.backButton";
const RESTART: &str = "restart";
const PREVIOUS: &str = "previous";

/// The behaviours offered, value and label.
pub const CHOICES: [(&str, &str); 2] = [
    (RESTART, chilpa_i18n::mark("Restart the track, then go back")),
    (PREVIOUS, chilpa_i18n::mark("Always go to the previous track")),
];

/// A press this soon after a restart is the second of a double press.
const DOUBLE_PRESS: Duration = Duration::from_secs(2);
/// A track this near its start has nothing left to restart.
const AT_START: f64 = 1.0;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Back {
    Restart,
    Previous,
}

/// What one press does. `restarted` is how long ago the last press restarted
/// the track, if it did.
pub(crate) fn decide(mode: &str, seconds: f64, restarted: Option<Duration>) -> Back {
    if mode == PREVIOUS || seconds < AT_START || restarted.is_some_and(|ago| ago <= DOUBLE_PRESS) {
        Back::Previous
    } else {
        Back::Restart
    }
}

impl Chilpa {
    /// The back button, as the setting has it. Media keys and the tray use
    /// the same one, so every way of pressing back agrees.
    pub fn back_pressed(&mut self, device: &str, bit_perfect: bool) {
        let mode = self.preference(BACK).unwrap_or_default();
        let ago = self.desktop.back_restarted.map(|at| at.elapsed());
        // Casting and a queue that is not playing have no start to return to.
        let playing = self.now.playing && !self.casting.active();
        match if playing { decide(&mode, self.now.seconds, ago) } else { Back::Previous } {
            Back::Restart => {
                self.desktop.back_restarted = Some(Instant::now());
                self.seek(0.0);
            }
            Back::Previous => {
                self.desktop.back_restarted = None;
                self.previous_track(device, bit_perfect);
            }
        }
    }

    pub(crate) fn back_button_setting(&mut self, key: &str, value: &str) -> bool {
        if key != BACK {
            return false;
        }
        if CHOICES.iter().any(|(choice, _)| *choice == value) {
            self.remember_preference(key, value);
        }
        true
    }

    pub(crate) fn back_button_row(&self, row: &mut crate::SettingsRow) -> bool {
        if row.key != BACK {
            return false;
        }
        for (value, label) in CHOICES {
            row.values.push(value.to_string());
            row.options.push(chilpa_i18n::t(label));
        }
        row.value = self.preference(BACK).filter(|value| CHOICES.iter().any(|(choice, _)| *choice == value.as_str())).unwrap_or_else(|| RESTART.to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Option<Duration> = Some(Duration::from_millis(600));

    #[test]
    fn one_press_restarts_a_track_that_is_under_way() {
        assert_eq!(decide(RESTART, 42.0, None), Back::Restart);
    }

    #[test]
    fn a_second_press_soon_after_goes_back_and_a_late_one_restarts_again() {
        assert_eq!(decide(RESTART, 0.4, SECOND), Back::Previous);
        assert_eq!(decide(RESTART, 30.0, SECOND), Back::Previous);
        assert_eq!(decide(RESTART, 30.0, Some(Duration::from_secs(5))), Back::Restart);
    }

    #[test]
    fn a_track_at_its_start_has_nothing_to_restart() {
        assert_eq!(decide(RESTART, 0.0, None), Back::Previous);
    }

    #[test]
    fn the_other_choice_always_goes_back() {
        assert_eq!(decide(PREVIOUS, 120.0, None), Back::Previous);
    }
}
