//! The tray icon as a feature: its three settings, what its tooltip says,
//! and the glyph it shows. The icon itself is the desktop crate's.

use crate::{Chilpa, NowPlaying};

/// Whether the icon is on the panel at all.
pub const SHOWN: &str = "desktop.tray";
/// Closing the window keeps Chilpa playing in the tray instead of quitting.
pub const CLOSE_TO_TRAY: &str = "desktop.closeToTray";
/// Quitting from the window, its menu, Ctrl+Q or the tray asks first, as
/// QBZ's does; the question's "Don't ask again" turns it off.
pub const CONFIRM_QUIT: &str = "desktop.confirmQuit";
/// Which glyph: "auto" follows the desktop's colours, "mono-light" is the
/// light glyph for a dark panel, "mono-dark" the dark glyph for a light
/// panel, "color" the symbol in its own colours.
pub const GLYPH: &str = "desktop.trayIcon";
pub const GLYPHS: [&str; 4] = ["auto", "mono-light", "mono-dark", "color"];

/// What the tooltip says of what plays: the title over the artist, the
/// album, and what the clicks and the wheel do.
pub fn said(now: Option<&NowPlaying>, sounding: bool) -> (String, String) {
    let Some(now) = now else {
        return ("Chilpa".to_owned(), chilpa_i18n::t("Music player\nNothing playing"));
    };
    let title = if now.title.is_empty() { "Chilpa".to_owned() } else { now.title.clone() };
    let mut lines = Vec::with_capacity(4);
    if !now.artist.is_empty() {
        lines.push(chilpa_i18n::t_args("by {}", &[&now.artist]));
    }
    if !now.album.is_empty() {
        lines.push(now.album.clone());
    }
    lines.push(chilpa_i18n::t(if sounding { "Middle-click to pause" } else { "Middle-click to play" }));
    lines.push(chilpa_i18n::t("Scroll to adjust volume"));
    (title, lines.join("\n"))
}

/// The glyph a preference asks for, "auto" by the desktop's colours.
#[cfg(target_os = "linux")]
pub(crate) fn glyph_of(preference: &str, dark_desktop: bool) -> chilpa_mpris::tray::Variant {
    use chilpa_mpris::tray::Variant;
    match preference {
        "mono-light" => Variant::Light,
        "mono-dark" => Variant::Dark,
        "color" => Variant::Color,
        _ if dark_desktop => Variant::Light,
        _ => Variant::Dark,
    }
}

/// A step of the wheel over the icon: five points of volume per notch.
pub fn volume_after(percent: u8, notches: i32) -> u8 {
    (i32::from(percent) + notches * 5).clamp(0, 100) as u8
}

impl Chilpa {
    /// Whether the icon is wanted on the panel.
    pub fn tray_wanted(&self) -> bool {
        self.preference(SHOWN).is_none_or(|v| v == "true")
    }

    /// Whether closing the window should hide it to the tray instead: only
    /// while there is a tray to hide to.
    pub fn closes_to_tray(&self) -> bool {
        self.preference(CLOSE_TO_TRAY).is_none_or(|v| v == "true") && self.tray_present()
    }

    /// Whether quitting asks first.
    pub fn confirms_quit(&self) -> bool {
        self.preference(CONFIRM_QUIT).is_none_or(|v| v == "true")
    }

    /// Quitting stops asking: the question's "Don't ask again".
    pub fn stop_confirming_quit(&mut self) {
        self.remember_preference(CONFIRM_QUIT, "false");
    }

    /// The glyph preference, one of [`GLYPHS`].
    pub fn tray_glyph(&self) -> String {
        self.preference(GLYPH).filter(|v| GLYPHS.contains(&v.as_str())).unwrap_or_else(|| GLYPHS[0].to_owned())
    }

    /// Takes one of the tray's settings; `false` for a key that is not its.
    pub(crate) fn tray_setting(&mut self, key: &str, value: &str) -> bool {
        match key {
            SHOWN => {
                self.remember_preference(key, &(value == "true").to_string());
                if value == "true" { self.start_tray() } else { self.stop_tray() }
            }
            CLOSE_TO_TRAY | CONFIRM_QUIT => self.remember_preference(key, &(value == "true").to_string()),
            GLYPH => {
                let glyph = if GLYPHS.contains(&value) { value } else { GLYPHS[0] };
                self.remember_preference(key, glyph);
                self.recolor_tray();
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing(title: &str, artist: &str, album: &str) -> NowPlaying {
        NowPlaying { title: title.into(), artist: artist.into(), album: album.into(), ..NowPlaying::default() }
    }

    #[test]
    fn the_tooltip_says_what_plays_and_what_the_clicks_do() {
        let now = playing("Wasted Years", "Iron Maiden", "Somewhere in Time");
        assert_eq!(said(Some(&now), true), ("Wasted Years".into(), "by Iron Maiden\nSomewhere in Time\nMiddle-click to pause\nScroll to adjust volume".into()));
        let bare = playing("", "", "");
        assert_eq!(said(Some(&bare), false), ("Chilpa".into(), "Middle-click to play\nScroll to adjust volume".into()));
        assert_eq!(said(None, false), ("Chilpa".into(), "Music player\nNothing playing".into()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_follows_the_desktop_and_a_choice_does_not() {
        use chilpa_mpris::tray::Variant;
        assert_eq!(glyph_of("auto", true), Variant::Light);
        assert_eq!(glyph_of("auto", false), Variant::Dark);
        assert_eq!(glyph_of("mono-light", false), Variant::Light);
        assert_eq!(glyph_of("mono-dark", true), Variant::Dark);
        assert_eq!(glyph_of("color", true), Variant::Color);
        assert_eq!(glyph_of("nonsense", false), Variant::Dark);
    }

    #[test]
    fn the_wheel_moves_the_volume_five_at_a_time_and_stays_in_range() {
        assert_eq!(volume_after(50, 1), 55);
        assert_eq!(volume_after(50, -3), 35);
        assert_eq!(volume_after(98, 1), 100);
        assert_eq!(volume_after(3, -1), 0);
    }
}
