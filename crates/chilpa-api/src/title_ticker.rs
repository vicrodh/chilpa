//! Whether a title cut short slides to show its end while the pointer rests
//! on it. On unless the listener turned it off; its tooltip stays either way.

use crate::Chilpa;

/// The toggle under Appearance.
pub const SLIDES: &str = "appearance.titleTicker";

/// The value kept for the toggle, or `None` for another key or a value
/// that is not one.
fn accepted(key: &str, value: &str) -> Option<bool> {
    if key != SLIDES {
        return None;
    }
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

impl Chilpa {
    /// Whether a cut title slides under the pointer.
    pub fn title_ticker(&self) -> bool {
        self.preference(SLIDES).is_none_or(|v| v != "false")
    }

    pub(crate) fn title_ticker_setting(&mut self, key: &str, value: &str) -> bool {
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
        assert_eq!(accepted(SLIDES, "true"), Some(true));
        assert_eq!(accepted(SLIDES, "false"), Some(false));
        assert_eq!(accepted(SLIDES, "yes"), None);
        assert_eq!(accepted("appearance.density", "false"), None);
    }
}
