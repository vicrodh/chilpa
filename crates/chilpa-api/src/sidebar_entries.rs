//! The sidebar's own destinations: their order and which are shown
//! (Settings > Navigation). A hidden one leaves the sidebar and the Menu
//! button only; its screen still opens from links, shortcuts and search.

use crate::navigation::Screen;
use crate::settings_rows::SettingsRow;
use crate::Chilpa;

/// The preference: ids in order, separated by commas, with a leading `-`
/// on the ones turned off.
pub(crate) const KEY: &str = "navigation.sidebar";
/// The single switch Devices had before this list.
const LEGACY_DEVICES: &str = "navigation.devices";

/// Every destination, in the order a new profile shows them. The ids are
/// the screens' names.
pub(crate) const ENTRIES: [&str; 11] = [
    "home", "library", "artists", "albums", "tracks", "genres",
    "composers", "compositions", "folders", "openMedia", "deviceSync",
];

/// Every known entry exactly once, as `(id, shown)`: unknown and repeated
/// ids are dropped, missing ones are appended shown, and when none is
/// shown the first one is.
pub(crate) fn normalized(raw: &str) -> Vec<(&'static str, bool)> {
    let mut out: Vec<(&'static str, bool)> = Vec::new();
    for part in raw.split(',').map(str::trim) {
        let (shown, id) = match part.strip_prefix('-') {
            Some(id) => (false, id),
            None => (true, part),
        };
        if let Some(known) = ENTRIES.iter().find(|entry| **entry == id) {
            if !out.iter().any(|(seen, _)| seen == known) {
                out.push((known, shown));
            }
        }
    }
    for id in ENTRIES {
        if !out.iter().any(|(seen, _)| *seen == id) {
            out.push((id, true));
        }
    }
    if !out.iter().any(|(_, shown)| *shown) {
        out[0].1 = true;
    }
    out
}

pub(crate) fn encoded(entries: &[(&str, bool)]) -> String {
    entries.iter().map(|(id, shown)| if *shown { id.to_string() } else { format!("-{id}") }).collect::<Vec<_>>().join(",")
}

/// The layout as kept. Before it was ever kept: every entry in order, with
/// Devices off when its old switch was.
fn kept() -> Vec<(&'static str, bool)> {
    let window = chilpa_core::settings::load().window;
    if let Some(raw) = window.get(KEY) {
        return normalized(raw);
    }
    let devices_off = window.get(LEGACY_DEVICES).is_some_and(|value| value == "false");
    ENTRIES.iter().map(|id| (*id, !(devices_off && *id == "deviceSync"))).collect()
}

/// Where Chilpa opens when no tabs are brought back: the first entry shown.
pub(crate) fn first_shown() -> Screen {
    kept().into_iter().find(|(_, shown)| *shown).and_then(|(id, _)| Screen::from_name(id)).unwrap_or(Screen::Home)
}

impl Chilpa {
    /// The sidebar's destinations for the window, normalized.
    pub fn sidebar_layout(&self) -> String {
        encoded(&kept())
    }

    /// Takes only a whole, normalized layout: the window always sends
    /// every entry, so anything else (none shown, an unknown id, a missing
    /// one) is refused rather than repaired.
    pub(crate) fn sidebar_entries_setting(&mut self, key: &str, value: &str) -> bool {
        if key != KEY || encoded(&normalized(value)) != value {
            return false;
        }
        self.remember_preference_checked(key, value).is_ok()
    }

    pub(crate) fn sidebar_entries_row(&self, row: &mut SettingsRow) -> bool {
        if row.key != KEY {
            return false;
        }
        row.value = self.sidebar_layout();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(entries: &[(&str, bool)]) -> Vec<String> {
        entries.iter().map(|(id, shown)| if *shown { id.to_string() } else { format!("-{id}") }).collect()
    }

    #[test]
    fn every_entry_is_a_screen_the_navigation_knows() {
        for id in ENTRIES {
            assert!(crate::navigation::Screen::from_name(id).is_some(), "{id} is not a navigation screen");
        }
    }

    #[test]
    fn nothing_kept_is_every_entry_in_order_and_shown() {
        assert_eq!(normalized(""), ENTRIES.iter().map(|id| (*id, true)).collect::<Vec<_>>());
    }

    #[test]
    fn unknown_and_repeated_ids_are_dropped_and_missing_ones_appended_shown() {
        let got = normalized("tracks,nowhere,-home,tracks,-genres");
        assert_eq!(&ids(&got)[..3], ["tracks", "-home", "-genres"]);
        assert_eq!(got.len(), ENTRIES.len(), "every entry exactly once");
        assert!(got[3..].iter().all(|(_, shown)| *shown), "a missing entry comes back shown: {got:?}");
    }

    #[test]
    fn at_least_one_entry_stays_shown() {
        let all_off = ENTRIES.iter().map(|id| format!("-{id}")).collect::<Vec<_>>().join(",");
        let got = normalized(&all_off);
        assert_eq!(got.iter().filter(|(_, shown)| *shown).count(), 1);
        assert_eq!(got[0], ("home", true));
    }

    #[test]
    fn encoding_round_trips() {
        let raw = "albums,-home,library,artists,tracks,genres,composers,compositions,folders,openMedia,-deviceSync";
        assert_eq!(encoded(&normalized(raw)), raw);
    }
}
