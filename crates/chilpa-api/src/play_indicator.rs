//! Whether the mark of the track that plays moves. Off by default, as in
//! QBZ: the moving bars wake the window every pulse while they show, and
//! the still mark costs nothing.

use crate::Chilpa;

/// The toggle under Appearance.
pub const ANIMATED: &str = "appearance.playIndicator";

/// The value kept for the toggle, or `None` for another key or a value
/// that is not one.
fn accepted(key: &str, value: &str) -> Option<bool> {
    if key != ANIMATED {
        return None;
    }
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

impl Chilpa {
    /// Whether the playing row's mark is the moving bars.
    pub fn play_indicator_animated(&self) -> bool {
        self.preference(ANIMATED).is_some_and(|v| v == "true")
    }

    pub(crate) fn play_indicator_setting(&mut self, key: &str, value: &str) -> bool {
        let Some(on) = accepted(key, value) else { return false };
        self.remember_preference(key, &on.to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_toggle_and_a_boolean_are_taken() {
        assert_eq!(accepted(ANIMATED, "true"), Some(true));
        assert_eq!(accepted(ANIMATED, "false"), Some(false));
        assert_eq!(accepted(ANIMATED, "yes"), None);
        assert_eq!(accepted("appearance.density", "true"), None);
    }
}
