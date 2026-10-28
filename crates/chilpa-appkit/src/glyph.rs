//! What the menu-bar item shows and how a click on it is read. The choices
//! are apart from AppKit, so they are tested wherever the workspace is.

/// Which of the three bundled pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picture {
    /// The symbol in its own colours.
    Color,
    /// The light glyph, for a dark menu bar.
    White,
    /// The dark glyph, for a light menu bar.
    Black,
}

/// The picture a glyph preference asks for, and whether the menu bar should
/// tint it itself. "auto" is the dark glyph as a template image, which the
/// system recolours for a light or a dark bar; the others are drawn as they
/// are.
pub fn picture_for(preference: &str) -> (Picture, bool) {
    match preference {
        "color" => (Picture::Color, false),
        "mono-light" => (Picture::White, false),
        "mono-dark" => (Picture::Black, false),
        _ => (Picture::Black, true),
    }
}

/// What a click on the item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// The window shows if hidden and hides if in sight.
    Toggle,
    /// The menu opens.
    Menu,
}

/// A right click, or a click with Control held, opens the menu; a plain left
/// click toggles the window.
pub fn click(right_button: bool, control_held: bool) -> Click {
    if right_button || control_held { Click::Menu } else { Click::Toggle }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_is_a_template_and_the_rest_are_drawn_as_they_are() {
        assert_eq!(picture_for("auto"), (Picture::Black, true));
        assert_eq!(picture_for("anything else"), (Picture::Black, true));
        assert_eq!(picture_for("mono-light"), (Picture::White, false));
        assert_eq!(picture_for("mono-dark"), (Picture::Black, false));
        assert_eq!(picture_for("color"), (Picture::Color, false));
    }

    #[test]
    fn only_a_plain_left_click_toggles() {
        assert_eq!(click(false, false), Click::Toggle);
        assert_eq!(click(true, false), Click::Menu);
        assert_eq!(click(false, true), Click::Menu);
    }
}
