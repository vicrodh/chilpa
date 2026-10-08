//! The mini player as the facade keeps it: which of its five surfaces was
//! shown last, whether the card carries the blurred cover behind it, and
//! which surface it opens with. The window draws the mini player; this is
//! what it is drawn from, as QBZ's shell keeps it.

use crate::settings_rows::SettingsRow;
use crate::Chilpa;

/// The surface shown last, 0 to 4: micro, compact, artwork, queue, lyrics.
pub const SURFACE: &str = "mini.surface";
/// Whether the card carries its own blurred cover behind it.
pub const BLUR: &str = "mini.backgroundBlur";
/// The surface the mini player opens with: one of [`VIEWS`]. Its row is in
/// Appearance, so the key carries that section, as every row does.
pub const DEFAULT_VIEW: &str = "appearance.miniDefaultView";
/// The views to open with: the surface shown last, or one of the five.
pub const VIEWS: [&str; 6] = ["remember", "micro", "compact", "artwork", "queue", "lyrics"];
pub const SURFACES: u8 = 5;
/// Artwork, until the listener shows another.
pub const DEFAULT_SURFACE: u8 = 2;

/// The surface the mini player opens with: the view chosen, or the
/// surface shown last when the view is "remember" or unknown.
pub fn opening_surface(default_view: &str, last: u8) -> u8 {
    match VIEWS.iter().position(|v| *v == default_view) {
        Some(view) if view > 0 => (view - 1) as u8,
        _ => last.min(SURFACES - 1),
    }
}

/// What a setting's new value is kept as, or `None` for a key that is not
/// the mini player's, or a value it cannot take.
fn accepted(key: &str, value: &str) -> Option<String> {
    match key {
        SURFACE => value.trim().parse::<u8>().ok().filter(|n| *n < SURFACES).map(|n| n.to_string()),
        BLUR => matches!(value, "true" | "false").then(|| value.to_string()),
        DEFAULT_VIEW => Some(if VIEWS.contains(&value) { value } else { VIEWS[0] }.to_string()),
        _ => None,
    }
}

impl Chilpa {
    /// The surface shown last.
    pub fn mini_surface(&self) -> u8 {
        self.preference(SURFACE).and_then(|v| v.parse::<u8>().ok()).filter(|n| *n < SURFACES).unwrap_or(DEFAULT_SURFACE)
    }

    pub fn mini_background_blur(&self) -> bool {
        self.preference(BLUR).is_some_and(|v| v == "true")
    }

    pub fn mini_default_view(&self) -> String {
        self.preference(DEFAULT_VIEW).filter(|v| VIEWS.contains(&v.as_str())).unwrap_or_else(|| VIEWS[0].to_string())
    }

    /// The surface to open with now.
    pub fn mini_opening_surface(&self) -> u8 {
        opening_surface(&self.mini_default_view(), self.mini_surface())
    }

    /// Whether play has something to start on: a queue with tracks in it.
    pub fn queue_has_target(&self) -> bool {
        !self.queue_order().is_empty()
    }

    /// Takes one of the mini player's settings; `false` for a key that is
    /// not its, or a value it cannot take.
    pub(crate) fn mini_setting(&mut self, key: &str, value: &str) -> bool {
        let Some(kept) = accepted(key, value) else { return false };
        self.remember_preference(key, &kept);
        true
    }

    /// What the mini player's row says: the views to choose from and the
    /// one kept.
    pub(crate) fn mini_row(&self, row: &mut SettingsRow) {
        if row.key == DEFAULT_VIEW {
            for (value, label) in crate::settings_rows::fixed(&row.key) {
                row.values.push(value.to_string());
                row.options.push(label);
            }
            row.value = self.mini_default_view();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_opening_surface_follows_the_view_and_falls_back_to_the_last() {
        assert_eq!(opening_surface("micro", 3), 0);
        assert_eq!(opening_surface("compact", 3), 1);
        assert_eq!(opening_surface("artwork", 3), 2);
        assert_eq!(opening_surface("queue", 0), 3);
        assert_eq!(opening_surface("lyrics", 0), 4);
        assert_eq!(opening_surface("remember", 1), 1);
        assert_eq!(opening_surface("", 4), 4);
        assert_eq!(opening_surface("remember", 9), 4, "a surface that does not exist is the last one");
    }

    #[test]
    fn the_three_keys_are_taken_and_their_values_kept_in_range() {
        assert_eq!(accepted(SURFACE, "3").as_deref(), Some("3"));
        assert_eq!(accepted(SURFACE, "5"), None, "there are five surfaces");
        assert_eq!(accepted(SURFACE, "x"), None);
        assert_eq!(accepted(BLUR, "true").as_deref(), Some("true"));
        assert_eq!(accepted(BLUR, "yes"), None);
        assert_eq!(accepted(DEFAULT_VIEW, "queue").as_deref(), Some("queue"));
        assert_eq!(accepted(DEFAULT_VIEW, "nonsense").as_deref(), Some("remember"), "an unknown view remembers");
        assert_eq!(accepted("appearance.theme", "dark"), None, "another feature's key");
    }
}
