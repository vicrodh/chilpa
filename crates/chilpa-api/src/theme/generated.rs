//! A palette from QBZ's registry, its auto theme or the custom one, turned
//! into the tokens the window reads.
//!
//! The tokens fall in three kinds. Those QBZ paints from its palette take
//! the palette's value at the place QBZ's own component takes it (QBZ 2.1.2,
//! `qbz-qt/qml`): the content pane is `surfaceMain` (`AppShell.qml:416`),
//! the sidebar, the side panel and the player bar are `surfaceCard`
//! (`Sidebar.qml:495`, `AppShell.qml:272`, `NowPlayingBar.qml:26`), what is
//! raised is `surfaceElevated`, hovers are `surfaceHover`, and each overlay
//! is the alpha tier its component names (the mini player's at
//! `MiniShell.qml:93`, `MiniWindowControls.qml:93,179`, `TBtn.qml:54`,
//! `CapBtn.qml:55`, `MiniCoverArt.qml:39`; the frosted hairline is
//! `frostBorder`, the 10 % tier). Those Chilpa has and QBZ does not (the
//! listening stats, the casting tint, the selected row, the ambient chrome
//! derived from the surfaces) come from Chilpa's own dark or light file,
//! whichever matches the palette's polarity. And those that do not follow
//! the theme at all (the immersive view, the mini player over its backdrop)
//! are the files' constants in both.

use super::{Rgba, Theme};
use chilpa_theme::auto::PaletteColor;
use chilpa_theme::{base_from_theme, relative_luminance, CustomThemeBase, ThemeColors};

type Rgba8 = chilpa_theme::Rgba;

/// The theme file whose Chilpa-only tokens a palette of each polarity takes.
const DARK_TEMPLATE: &str = "dark-oled";
const LIGHT_TEMPLATE: &str = "breeze-light";

/// The accent's hover and pressed while another accent replaces the
/// palette's: the theme files' rules, since the palette's own pair belongs
/// to its own accent.
const OVERRIDDEN: [(&str, &str); 2] = [("accent.hover", "derive:accent+L0.06"), ("accent.pressed", "derive:accent-L0.08")];

/// How far the six cover placeholders step from `surface_elevated` in HSL
/// lightness, so a grid of them does not read as one block (the theme
/// files' spread). QBZ paints the one colour (`AlbumCard.qml:479`).
const PLACEHOLDER_STEPS: [f64; 6] = [0.0, -0.024, 0.024, -0.04, 0.008, 0.04];

/// QBZ's non-text contrast floor for a glyph on an accent fill (WCAG 1.4.11,
/// `QbzTheme.qml` `accentFloor`).
const ACCENT_FLOOR: f64 = 3.0;

fn hex(c: Rgba8) -> String {
    format!("#{:02X}{:02X}{:02X}{:02X}", c.r, c.g, c.b, c.a)
}

fn wide(c: Rgba8) -> Rgba {
    let f = |v: u8| f64::from(v) / 255.0;
    Rgba { r: f(c.r), g: f(c.g), b: f(c.b), a: f(c.a) }
}

fn narrow(c: Rgba) -> Rgba8 {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgba8::rgba(b(c.r), b(c.g), b(c.b), b(c.a))
}

/// `fg` laid over an opaque `bg`, opaque.
fn over(fg: Rgba, bg: Rgba) -> Rgba {
    let mix = |f: f64, b: f64| f * fg.a + b * (1.0 - fg.a);
    Rgba { r: mix(fg.r, bg.r), g: mix(fg.g, bg.g), b: mix(fg.b, bg.b), a: 1.0 }
}

/// WCAG 2.x relative luminance, spelled as `QbzTheme.qml`'s `_relLum`.
fn luminance(c: Rgba) -> f64 {
    let f = |v: f64| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
}

/// QBZ's colour for a glyph or a label on an accent fill (`QbzTheme.qml`,
/// `accentGlyphTint` and `accentGlyphColor`): the theme's own on-accent
/// colour while it clears the floor against the accent; else white or
/// black, whichever holds on both the accent and its hover; and when
/// neither does, whichever holds on the accent alone.
fn on_accent(accent: Rgba, text: Rgba, hover: Rgba) -> Rgba {
    let la = luminance(accent) + 0.05;
    let lt = luminance(text) + 0.05;
    if (if la > lt { la / lt } else { lt / la }) >= ACCENT_FLOOR {
        return text;
    }
    let lh = luminance(hover) + 0.05;
    let white = (1.05 / la).min(1.05 / lh);
    let black = la.min(lh) / 0.05;
    let pick_white = if white.max(black) >= ACCENT_FLOOR { white >= black } else { 1.05 / la >= la / 0.05 };
    let v = if pick_white { 1.0 } else { 0.0 };
    Rgba { r: v, g: v, b: v, a: 1.0 }
}

impl Theme {
    /// A theme built from a palette, under `id`.
    pub fn from_palette(id: &str, colors: &ThemeColors) -> Theme {
        let dark = relative_luminance(colors.surface_main) < 0.5;
        let mut theme = Theme::built_in(if dark { DARK_TEMPLATE } else { LIGHT_TEMPLATE });
        theme.id = id.to_string();
        theme.dark = dark;
        theme.palette = Some(*colors);
        let tier = |pct: u8| colors.alpha_pct(pct);
        // White on a dark palette, black on a light one, as the alpha ramp.
        let polar = |a: u8| {
            let v = if dark { 255 } else { 0 };
            Rgba8::rgba(v, v, v, a)
        };
        let set = [
            ("surface.window", colors.surface_main),
            ("surface.pane", colors.surface_main),
            ("surface.sidebar", colors.surface_card),
            ("surface.panel", colors.surface_card),
            ("surface.player", colors.surface_card),
            ("surface.raised", colors.surface_elevated),
            ("surface.lcd", tier(5)),
            ("text.primary", colors.text_primary),
            ("text.secondary", colors.text_secondary),
            ("text.tertiary", colors.text_muted),
            // The selector replaces it; the palette's own stays underneath.
            ("text.onAccent", colors.accent_text),
            // The toggle's knob, on the accent and off it (`QbzToggle.qml:50`).
            ("toggle.knob", colors.text_primary),
            ("stroke.hairline", colors.border_subtle),
            // The wizard's steps (`DacWizardModal.qml:342`).
            ("stroke.frame", colors.border_muted),
            ("state.hover", colors.surface_hover),
            ("state.selected", tier(10)),
            // A track row's stripe (`TrackRow.qml:480`, `#07ffffff`).
            ("state.stripe", polar(0x07)),
            ("fill.field", tier(8)),
            // The segment's count chip (`QbzTabBar.qml:92`).
            ("fill.count", tier(8)),
            ("fill.countActive", tier(15)),
            // The playing row (`TrackRow.qml:467`).
            ("row.playing", tier(10)),
            ("accent", colors.accent),
            ("accent.hover", colors.accent_hover),
            ("accent.pressed", colors.accent_pressed),
            ("favorite", colors.favorite),
            ("status.positive", colors.success),
            ("status.warning", colors.warning),
            ("status.negative", colors.danger),
            ("overlay.scrim", Rgba8::rgba(0, 0, 0, 0x80)),
            // The black glyph on the white overlay disc (`CardOverlayButton.qml:37`).
            ("overlay.ink", Rgba8::rgb(0x0a, 0x0a, 0x0a)),
            // The album header's floor (`HeaderGradient.qml:162`).
            ("albumHeader.base", Rgba8::rgb(0x0a, 0x0a, 0x0b)),
            ("ambient.frost", tier(10)),
            ("mini.edge", tier(10)),
            ("mini.capsuleEdge", tier(20)),
            ("mini.separator", tier(15)),
            ("mini.hover", tier(8)),
            ("mini.capsuleHover", tier(12)),
            ("mini.coverFill", tier(8)),
        ];
        for (token, color) in set {
            theme.specs.insert(token.to_string(), hex(color));
        }
        let raised = colors.surface_elevated;
        let raised = PaletteColor::new(raised.r, raised.g, raised.b);
        theme.placeholders = PLACEHOLDER_STEPS
            .iter()
            .map(|step| {
                let c = raised.shift_lightness(*step);
                wide(Rgba8::rgb(c.r, c.g, c.b))
            })
            .collect();
        theme
    }

    /// The palette this theme was built from, for a generated one.
    pub fn palette(&self) -> Option<&ThemeColors> {
        self.palette.as_ref()
    }

    /// What a generated theme answers apart from its tokens: the text on the
    /// accent goes through QBZ's selector, and a replaced accent takes the
    /// files' hover and pressed rules.
    pub(super) fn resolve_generated(&self, token: &str, depth: u8) -> Option<Rgba> {
        let palette = self.palette.as_ref()?;
        if self.accent.is_some() {
            if let Some((_, rule)) = OVERRIDDEN.iter().find(|(t, _)| *t == token) {
                return self.resolve_spec(rule, depth);
            }
        }
        if token == "text.onAccent" {
            let accent = self.resolve("accent", depth + 1)?;
            let hover = self.resolve("accent.hover", depth + 1)?;
            return Some(on_accent(accent, wide(palette.accent_text), hover));
        }
        None
    }

    /// The custom theme's base for what this theme shows, the accent as it
    /// is drawn: QBZ's "Start from current theme" and the seed the first
    /// choice of Custom takes (`custom_theme_qt.rs`, `seed_from_current`).
    /// A file theme maps its tokens onto the same eleven colours.
    pub fn seed_base(&self) -> CustomThemeBase {
        let black = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
        let color = |token: &str| self.color(token).unwrap_or(black);
        let opaque = |token: &str| narrow(over(color(token), color("surface.window"))).to_hex();
        let mut base = match &self.palette {
            Some(colors) => base_from_theme(colors, self.dark),
            None => CustomThemeBase {
                is_dark: self.dark,
                surface_main: opaque("surface.window"),
                surface_card: opaque("surface.panel"),
                surface_elevated: opaque("surface.raised"),
                text_primary: opaque("text.primary"),
                text_secondary: opaque("text.secondary"),
                accent: String::new(),
                danger: opaque("status.negative"),
                warning: opaque("status.warning"),
                success: opaque("status.positive"),
                border: opaque("stroke.hairline"),
                favorite: opaque("favorite"),
            },
        };
        base.accent = opaque("accent");
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_theme::{palette, ThemeId};

    fn close(a: Rgba, b: Rgba) -> bool {
        (a.r - b.r).abs() < 0.002 && (a.g - b.g).abs() < 0.002 && (a.b - b.b).abs() < 0.002 && (a.a - b.a).abs() < 0.002
    }

    #[test]
    fn every_registry_theme_resolves_every_token() {
        for id in chilpa_theme::ALL {
            let theme = Theme::from_palette(id.slug(), &palette(*id));
            for token in super::super::TOKENS {
                assert!(theme.color(token).is_some(), "{}: {token}", id.slug());
            }
            assert_eq!(theme.id(), id.slug());
        }
    }

    #[test]
    fn the_surfaces_sit_where_qbz_paints_them() {
        let nord = palette(ThemeId::Nord);
        let theme = Theme::from_palette("nord", &nord);
        assert!(!theme.dark == chilpa_theme::is_light(ThemeId::Nord));
        assert!(close(theme.color("surface.window").unwrap(), wide(nord.surface_main)));
        assert!(close(theme.color("surface.sidebar").unwrap(), wide(nord.surface_card)));
        assert!(close(theme.color("surface.player").unwrap(), wide(nord.surface_card)));
        assert!(close(theme.color("surface.raised").unwrap(), wide(nord.surface_elevated)));
        assert!(close(theme.color("text.tertiary").unwrap(), wide(nord.text_muted)));
        assert!(close(theme.color("favorite").unwrap(), wide(nord.favorite)));
        assert!(close(theme.color("state.hover").unwrap(), wide(nord.surface_hover)));
    }

    #[test]
    fn a_light_palette_takes_the_light_file_and_black_overlays() {
        let theme = Theme::from_palette("alucard", &palette(ThemeId::Alucard));
        assert!(!theme.dark);
        let edge = theme.color("mini.edge").unwrap();
        assert!(edge.r == 0.0 && (edge.a - 0.10).abs() < 0.01, "black at the 10 % tier");
        let light_file = Theme::built_in("breeze-light");
        assert_eq!(theme.color("stats.artists"), light_file.color("stats.artists"));
    }

    #[test]
    fn the_text_on_the_accent_is_the_themes_own_when_it_holds() {
        // Tokyo Night's accent-text is its own dark navy and clears 3:1.
        let tokyo = palette(ThemeId::TokyoNight);
        let theme = Theme::from_palette("tokyo-night", &tokyo);
        assert!(close(theme.color("text.onAccent").unwrap(), wide(tokyo.accent_text)));
    }

    #[test]
    fn the_text_on_the_accent_falls_back_where_qbz_measured_it_failing() {
        // Rose Pine Dawn: accent #d7827e, accent-text #575279 is 2.56:1,
        // so QBZ paints black (7.38:1).
        let theme = Theme::from_palette("rose-pine-dawn", &palette(ThemeId::RosePineDawn));
        let on = theme.color("text.onAccent").unwrap();
        assert!(on.r == 0.0 && on.g == 0.0 && on.b == 0.0);
    }

    #[test]
    fn the_ink_on_a_dark_backdrop_is_white_whatever_the_accent() {
        // Tokyo Night's text on the accent is its dark navy: the album
        // header and the cover overlays must not take it (QBZ paints them
        // #ffffff, AlbumView.qml:299).
        let white = Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
        for theme in [Theme::from_palette("tokyo-night", &palette(ThemeId::TokyoNight)), Theme::built_in("breeze-light"), Theme::built_in("dark-oled")] {
            assert!(close(theme.color("backdrop.text").unwrap(), white), "{}", theme.id());
            assert!((theme.color("backdrop.textMuted").unwrap().a - 0.82).abs() < 0.002, "{}", theme.id());
        }
        let tokyo = palette(ThemeId::TokyoNight);
        let theme = Theme::from_palette("tokyo-night", &tokyo);
        assert!(!close(theme.color("text.onAccent").unwrap(), white));
        assert!(close(theme.color("toggle.knob").unwrap(), wide(tokyo.text_primary)), "QbzToggle.qml:50");
        assert!(close(Theme::built_in("breeze-light").color("toggle.knob").unwrap(), white), "Chilpa's own stays white");
    }

    #[test]
    fn a_chosen_accent_carries_its_own_hover_on_a_palette_theme() {
        let mut theme = Theme::from_palette("nord", &palette(ThemeId::Nord));
        let own = theme.color("accent.hover");
        theme.set_accent(super::super::parse_color("#A3161F"));
        let hover = theme.color("accent.hover").unwrap();
        assert_ne!(Some(hover), own);
        assert!(hover.r > 163.0 / 255.0, "lighter than the chosen accent");
    }

    #[test]
    fn a_registry_theme_seeds_the_custom_one_with_its_base() {
        let theme = Theme::from_palette("dracula", &palette(ThemeId::Dracula));
        let base = theme.seed_base();
        assert_eq!(base, base_from_theme(&palette(ThemeId::Dracula), true));
    }

    #[test]
    fn a_file_theme_seeds_opaque_colors() {
        let base = Theme::built_in("breeze-light").seed_base();
        assert!(!base.is_dark);
        assert_eq!(base.surface_main, "#fcfcfc");
        for hex in [&base.border, &base.surface_card, &base.favorite] {
            assert!(chilpa_theme::Rgba::from_hex(hex).is_some_and(|c| c.a == 255), "{hex}");
        }
    }
}
