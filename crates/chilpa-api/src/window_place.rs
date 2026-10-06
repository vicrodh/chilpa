//! Where the window was and how big, kept between runs.
//!
//! Only a floating window says anything worth keeping: a maximized or a
//! full-screen frame is the screen's size, not the window's, and kept as
//! the floating size the next run would open a window that fills the
//! screen without being maximized. So the floating size and place are one
//! thing and the maximized flag another, and each frame the window reports
//! changes only what it can speak for.

use crate::Chilpa;
use std::collections::BTreeMap;

/// The size the window opens at the first time.
pub const DEFAULT: (i32, i32) = (1280, 800);
/// The smallest the window gets. A frame under it is a window on its way
/// somewhere (minimizing reports nothing by nothing), not a size.
pub const SMALLEST: (i32, i32) = (960, 600);

const WIDTH: &str = "window.width";
const HEIGHT: &str = "window.height";
const LEFT: &str = "window.x";
const TOP: &str = "window.y";
const MAXIMIZED: &str = "window.maximized";

/// What the window is doing when it reports a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    Floating,
    Maximized,
    /// Full screen, minimized or hidden: it says nothing about the window.
    Away,
}

impl Frame {
    /// The frame by the name the window gives it.
    pub fn named(name: &str) -> Self {
        match name {
            "floating" => Self::Floating,
            "maximized" => Self::Maximized,
            _ => Self::Away,
        }
    }
}

/// The window's floating size and place, and whether it was maximized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub width: i32,
    pub height: i32,
    /// Its top left corner, where the system lets a window know its own.
    pub at: Option<(i32, i32)>,
    pub maximized: bool,
}

impl Default for Place {
    fn default() -> Self {
        Self { width: DEFAULT.0, height: DEFAULT.1, at: None, maximized: false }
    }
}

impl Place {
    /// The place after the window reports a frame. `at` is `None` where the
    /// system does not tell a window where it is: the place known from
    /// before stays.
    pub fn after(self, frame: Frame, width: i32, height: i32, at: Option<(i32, i32)>) -> Self {
        match frame {
            Frame::Away => self,
            Frame::Maximized => Self { maximized: true, ..self },
            Frame::Floating if width < SMALLEST.0 || height < SMALLEST.1 => self,
            Frame::Floating => Self { width, height, at: at.or(self.at), maximized: false },
        }
    }

    /// The place the window's preferences hold; what is missing or was
    /// written by hand into nonsense falls back to the default.
    pub fn read(kept: &BTreeMap<String, String>) -> Self {
        let number = |key: &str| kept.get(key).and_then(|value| value.parse::<i32>().ok());
        let size = match (number(WIDTH), number(HEIGHT)) {
            (Some(width), Some(height)) if width >= SMALLEST.0 && height >= SMALLEST.1 => (width, height),
            _ => DEFAULT,
        };
        Self {
            width: size.0,
            height: size.1,
            at: number(LEFT).zip(number(TOP)),
            maximized: kept.get(MAXIMIZED).is_some_and(|value| value == "true"),
        }
    }

    /// Writes the place into the window's preferences.
    pub fn write(self, kept: &mut BTreeMap<String, String>) {
        kept.insert(WIDTH.into(), self.width.to_string());
        kept.insert(HEIGHT.into(), self.height.to_string());
        kept.insert(MAXIMIZED.into(), self.maximized.to_string());
        if let Some((left, top)) = self.at {
            kept.insert(LEFT.into(), left.to_string());
            kept.insert(TOP.into(), top.to_string());
        }
    }
}

impl Chilpa {
    /// Where the window was when it last said.
    pub fn window_place(&self) -> Place {
        Place::read(&chilpa_core::settings::load().window)
    }

    /// Takes a frame the window reports and keeps what it changes. A frame
    /// that changes nothing writes nothing.
    pub fn keep_window_place(&mut self, frame: Frame, width: i32, height: i32, at: Option<(i32, i32)>) {
        let mut settings = chilpa_core::settings::load();
        let before = Place::read(&settings.window);
        let after = before.after(frame, width, height, at);
        if after != before {
            after.write(&mut settings.window);
            let _ = chilpa_core::settings::save(&settings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, Place, DEFAULT};
    use std::collections::BTreeMap;

    const KEPT: Place = Place { width: 1400, height: 900, at: Some((120, 80)), maximized: false };

    #[test]
    fn a_floating_frame_is_the_size_and_the_place() {
        let place = KEPT.after(Frame::Floating, 1500, 950, Some((40, 30)));
        assert_eq!(place, Place { width: 1500, height: 950, at: Some((40, 30)), maximized: false });
    }

    #[test]
    fn a_maximized_frame_keeps_the_floating_size() {
        let place = KEPT.after(Frame::Maximized, 2560, 1400, Some((0, 0)));
        assert_eq!(place, Place { maximized: true, ..KEPT });
        // Floating again, it is no longer maximized.
        assert!(!place.after(Frame::Floating, 1400, 900, None).maximized);
    }

    #[test]
    fn full_screen_and_minimized_say_nothing() {
        let maximized = Place { maximized: true, ..KEPT };
        assert_eq!(maximized.after(Frame::Away, 2560, 1440, Some((0, 0))), maximized);
        assert_eq!(KEPT.after(Frame::Away, 0, 0, None), KEPT);
        assert_eq!(Frame::named("fullscreen"), Frame::Away);
    }

    #[test]
    fn a_frame_under_the_smallest_window_is_not_a_size() {
        assert_eq!(KEPT.after(Frame::Floating, 0, 0, Some((0, 0))), KEPT);
        assert_eq!(KEPT.after(Frame::Floating, 959, 900, None), KEPT);
    }

    #[test]
    fn where_the_window_cannot_know_its_place_the_old_one_stays() {
        let place = KEPT.after(Frame::Floating, 1500, 950, None);
        assert_eq!(place.at, Some((120, 80)));
        assert_eq!((place.width, place.height), (1500, 950));
    }

    #[test]
    fn the_place_survives_its_preferences() {
        let mut kept = BTreeMap::new();
        assert_eq!(Place::read(&kept), Place::default());
        Place { maximized: true, ..KEPT }.write(&mut kept);
        assert_eq!(Place::read(&kept), Place { maximized: true, ..KEPT });
        // A size no window of mine could have is not believed.
        kept.insert("window.width".into(), "12".into());
        assert_eq!((Place::read(&kept).width, Place::read(&kept).height), DEFAULT);
    }
}
