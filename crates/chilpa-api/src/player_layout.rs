//! How the player sits in the window: floating over the content, as it
//! always did, or a bar the window's width at its bottom or at its top, with
//! the sidebar, the content and the right panel beside it instead of under
//! it. Chosen from the player's View menu and kept for the next run.

use crate::Chilpa;

const KEY: &str = "ui.player.layout";

/// The layouts, as the window names them; the first is the one a profile
/// that never chose starts with.
pub const LAYOUTS: [&str; 3] = ["floating", "bottom", "top"];

/// The layout a kept value names, or the first for anything else.
pub fn layout_of(kept: Option<&str>) -> &'static str {
    LAYOUTS.iter().copied().find(|layout| Some(*layout) == kept).unwrap_or(LAYOUTS[0])
}

impl Chilpa {
    pub fn player_layout(&self) -> &'static str {
        layout_of(self.preference(KEY).as_deref())
    }

    /// Keeps `layout` and says the one kept: anything unknown is floating.
    pub fn remember_player_layout(&mut self, layout: &str) -> &'static str {
        let kept = layout_of(Some(layout));
        self.remember_preference(KEY, kept);
        kept
    }
}

#[cfg(test)]
mod tests {
    use super::layout_of;

    #[test]
    fn a_layout_is_one_of_the_three_or_floating() {
        assert_eq!(layout_of(Some("top")), "top");
        assert_eq!(layout_of(Some("bottom")), "bottom");
        assert_eq!(layout_of(Some("sideways")), "floating");
        assert_eq!(layout_of(None), "floating", "a profile that never chose");
    }
}
