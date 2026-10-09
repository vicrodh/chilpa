//! A theme file, read into colors.
//!
//! A theme is data: one JSON file per theme with every semantic token
//! (`surface.window`, `text.secondary`). Some tokens are not written but
//! derived from another one, and the file says how: `derive:accent@0.10` is
//! the accent at 10 % alpha, `derive:accent+L0.06` is the accent 6 % lighter.
//! They are computed here, once, so QML never builds a color (the QML
//! literal check fails on `Qt.rgba`, `Qt.lighter` and friends).
//!
//! Overriding the accent re-derives everything that hangs off it.

use std::collections::HashMap;

/// A color with every channel between 0 and 1.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

/// One layer of a shadow: offset, blur radius and color.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shadow {
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub color: Rgba,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub id: String,
    pub dark: bool,
    /// As written in the file: a color or a `derive:` rule.
    specs: HashMap<String, String>,
    placeholders: Vec<Rgba>,
    shadows: HashMap<String, Vec<Shadow>>,
    accent: Option<Rgba>,
}

/// Every token a theme has to define. A theme that lacks one is refused at
/// load, not discovered as a transparent patch in the window.
pub const TOKENS: &[&str] = &[
    "surface.window", "surface.sidebar", "surface.raised", "surface.panel", "surface.pane",
    "surface.player", "surface.lcd", "text.primary", "text.secondary", "text.tertiary",
    "text.onAccent", "text.onAccentMuted", "stroke.hairline", "stroke.frame", "state.hover", "state.selected",
    "state.stripe", "fill.field", "row.playing", "row.selected", "accent", "accent.hover",
    "accent.pressed", "status.positive", "status.warning", "status.negative", "overlay.scrim", "lightbox.scrim", "stats.hero", "stats.artists", "stats.albums", "stats.tracks", "stats.previous",
    "overlay.button", "overlay.ink", "casting",
    "albumHeader.base", "albumHeader.dim", "albumHeader.top", "albumHeader.bottom",
    "ambient.sidebar", "ambient.playerBase", "ambient.player", "ambient.content", "ambient.raised", "ambient.frost", "ambient.veil", "ambient.bar", "ambient.panel",
    "mini.edge", "mini.capsuleEdge", "mini.separator", "mini.hover", "mini.capsuleHover", "mini.coverFill",
    "immersive.stage", "immersive.black", "immersive.glass", "immersive.glassEdge", "immersive.glassHover",
    "immersive.menu", "immersive.rowActive", "immersive.rowHover", "immersive.text", "immersive.textStrong",
    "immersive.textBody", "immersive.textDim", "immersive.textMuted", "immersive.textFaint", "immersive.textGhost",
    "immersive.textQuiet", "immersive.lyricInactive", "immersive.fill", "immersive.nav", "immersive.seekTrack",
    "immersive.volumeTrack", "immersive.volumeTrackLocked", "immersive.volumeFill", "immersive.volumeFillLocked", "immersive.bubble",
    "immersive.shadow", "immersive.shadowDeep", "immersive.shadowArt", "immersive.placeholder", "immersive.scrim",
    "immersive.scopeVeil", "immersive.textShadow", "immersive.playDisc", "immersive.playDiscHover", "immersive.playGlyph",
    "immersive.cardGlass", "immersive.cardVeil", "immersive.cardGradientTop", "immersive.cardGradientMid", "immersive.cardGradientBottom",
    "immersive.cardEdge", "immersive.queueRow", "immersive.queueRowEdge", "immersive.queueRowHover", "immersive.queueCover",
    "immersive.queueCoverEdge", "immersive.queueChip", "immersive.queueChipText", "immersive.queuePlay", "immersive.queuePlayHover",
    "immersive.queueLine", "immersive.clear", "immersive.clearHover", "immersive.clearEdge", "immersive.clearEdgeHover",
    "immersive.searchPanel", "immersive.searchEdge", "immersive.axis", "immersive.axisBox", "immersive.ribbonBase",
    "immersive.glowDefault", "immersive.separator",
    "mini.rowActive", "mini.danger", "mini.onBackdrop", "mini.glyphOnBackdrop", "mini.seekOnBackdrop", "mini.footer",
];

/// How far the two weaker text tokens move toward `text.primary` while the
/// dynamic background shows through a light theme: the field sits at mid
/// luminance, where a light theme's grey ramp loses its contrast floor. A
/// dark theme keeps its values.
const AMBIENT_HARDENING: [(&str, f64); 2] = [("text.secondary", 0.40), ("text.tertiary", 0.50)];

/// The themes that ship inside the binary.
pub const BUILT_IN: &[(&str, &str)] = &[
    ("breeze-light", include_str!("../themes/breeze-light.json")),
    ("breeze-dark", include_str!("../themes/breeze-dark.json")),
    ("dark-oled", include_str!("../themes/dark-oled.json")),
];

/// The theme a choice in Settings stands for. "system" follows the desktop,
/// and so does a choice nobody knows.
pub fn for_choice(choice: &str, system_dark: bool) -> &'static str {
    match choice {
        "light" => "breeze-light",
        "dark" => "breeze-dark",
        "oled" => "dark-oled",
        _ if system_dark => "breeze-dark",
        _ => "breeze-light",
    }
}

impl Theme {
    pub fn parse(json: &str) -> Result<Theme, String> {
        let root: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let id = root["id"].as_str().ok_or("the theme has no id")?.to_string();
        let colors = root["color"].as_object().ok_or("the theme has no colors")?;
        let mut specs = HashMap::new();
        let mut placeholders = Vec::new();
        for (key, value) in colors {
            match value {
                serde_json::Value::String(s) => {
                    specs.insert(key.clone(), s.clone());
                }
                serde_json::Value::Array(list) if key == "art.placeholder" => {
                    for item in list {
                        let text = item.as_str().ok_or("a placeholder is not a color")?;
                        placeholders.push(parse_color(text).ok_or(format!("{text} is not a color"))?);
                    }
                }
                _ => return Err(format!("{key} is neither a color nor a derive rule")),
            }
        }
        specs.entry("text.onAccentMuted".into()).or_insert_with(|| "derive:text.onAccent@0.82".into());
        let mut shadows = HashMap::new();
        if let Some(all) = root["shadow"].as_object() {
            for (name, layers) in all {
                let mut out = Vec::new();
                for layer in layers.as_array().into_iter().flatten() {
                    let n = |i: usize| layer[i].as_f64().unwrap_or(0.0);
                    let color = layer[3].as_str().and_then(parse_color).ok_or("a shadow has no color")?;
                    out.push(Shadow { x: n(0), y: n(1), blur: n(2), color });
                }
                shadows.insert(name.clone(), out);
            }
        }
        let theme = Theme {
            id,
            dark: root["base"].as_str() == Some("dark"),
            specs,
            placeholders,
            shadows,
            accent: None,
        };
        for token in TOKENS {
            theme.color(token).ok_or(format!("{} does not resolve {token}", theme.id))?;
        }
        if theme.placeholders.is_empty() {
            return Err(format!("{} has no art.placeholder colors", theme.id));
        }
        Ok(theme)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// A built-in theme by id.
    pub fn built_in(id: &str) -> Theme {
        let (_, json) = BUILT_IN.iter().find(|(i, _)| *i == id).unwrap_or(&BUILT_IN[0]);
        Theme::parse(json).expect("a built-in theme parses")
    }

    /// Replaces the accent; everything derived from it follows.
    pub fn set_accent(&mut self, accent: Option<Rgba>) {
        self.accent = accent;
    }

    pub fn color(&self, token: &str) -> Option<Rgba> {
        self.resolve(token, 0)
    }

    /// The token as it reads with the dynamic background on or off: under
    /// it, a light theme hardens its weaker text toward `text.primary`.
    pub fn color_under(&self, token: &str, ambient: bool) -> Option<Rgba> {
        let color = self.color(token)?;
        if !ambient || self.dark {
            return Some(color);
        }
        let Some((_, amount)) = AMBIENT_HARDENING.iter().find(|(t, _)| *t == token) else { return Some(color) };
        Some(toward(color, self.color("text.primary")?, *amount))
    }

    fn resolve(&self, token: &str, depth: u8) -> Option<Rgba> {
        if token == "accent" {
            if let Some(accent) = self.accent {
                return Some(accent);
            }
        }
        // The theme's own accent, whatever replaced it: what "follow the
        // theme" looks like among the accents to choose from.
        let token = if token == "accent.base" { "accent" } else { token };
        // A rule that points at itself, directly or not, is a broken file.
        if depth > 4 {
            return None;
        }
        let spec = self.specs.get(token)?;
        let Some(rule) = spec.strip_prefix("derive:") else {
            return parse_color(spec);
        };
        if let Some((base, alpha)) = rule.split_once('@') {
            let c = self.resolve(base, depth + 1)?;
            return Some(Rgba { a: alpha.parse().ok()?, ..c });
        }
        let at = rule.find(['+', '-'])?;
        let (base, change) = rule.split_at(at);
        let amount: f64 = change[2..].parse().ok()?;
        let sign = if change.starts_with('-') { -1.0 } else { 1.0 };
        if &change[1..2] != "L" {
            return None;
        }
        Some(lighten(self.resolve(base, depth + 1)?, sign * amount))
    }

    pub fn placeholder(&self, index: usize) -> Rgba {
        self.placeholders[index % self.placeholders.len()]
    }

    pub fn shadow(&self, name: &str, layer: usize) -> Option<Shadow> {
        self.shadows.get(name)?.get(layer).copied()
    }
}

/// `#RGB`, `#RRGGBB`, `#RRGGBBAA` or `rgba(r, g, b, a)`.
pub fn parse_color(text: &str) -> Option<Rgba> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let digit = |i: usize, n: usize| u8::from_str_radix(hex.get(i..i + n)?, 16).ok();
        let (r, g, b, a) = match hex.len() {
            3 => (digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17, 255),
            6 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, 255),
            8 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, digit(6, 2)?),
            _ => return None,
        };
        let f = |v: u8| f64::from(v) / 255.0;
        return Some(Rgba { r: f(r), g: f(g), b: f(b), a: f(a) });
    }
    let inner = text.strip_prefix("rgba(")?.strip_suffix(')')?;
    let parts: Vec<f64> = inner.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let [r, g, b, a] = parts[..] else { return None };
    Some(Rgba { r: r / 255.0, g: g / 255.0, b: b / 255.0, a })
}

/// Mixes `c` toward `to` by `amount` on each channel, keeping `c`'s alpha.
fn toward(c: Rgba, to: Rgba, amount: f64) -> Rgba {
    let mix = |from: f64, target: f64| from + (target - from) * amount;
    Rgba { r: mix(c.r, to.r), g: mix(c.g, to.g), b: mix(c.b, to.b), a: c.a }
}

/// Moves the HSL lightness by `amount`, keeping hue, saturation and alpha.
fn lighten(c: Rgba, amount: f64) -> Rgba {
    let max = c.r.max(c.g).max(c.b);
    let min = c.r.min(c.g).min(c.b);
    let l = (max + min) / 2.0;
    let d = max - min;
    let (h, s) = if d == 0.0 {
        (0.0, 0.0)
    } else {
        let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
        let h = if max == c.r {
            (c.g - c.b) / d + if c.g < c.b { 6.0 } else { 0.0 }
        } else if max == c.g {
            (c.b - c.r) / d + 2.0
        } else {
            (c.r - c.g) / d + 4.0
        };
        (h / 6.0, s)
    };
    let l = (l + amount).clamp(0.0, 1.0);
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let channel = |t: f64| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    Rgba { r: channel(h + 1.0 / 3.0), g: channel(h), b: channel(h - 1.0 / 3.0), a: c.a }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.002
    }

    #[test]
    fn a_choice_stands_for_its_theme_and_system_follows_the_desktop() {
        assert_eq!(for_choice("light", true), "breeze-light");
        assert_eq!(for_choice("dark", false), "breeze-dark");
        assert_eq!(for_choice("oled", false), "dark-oled");
        assert_eq!(for_choice("system", true), "breeze-dark");
        assert_eq!(for_choice("system", false), "breeze-light");
        assert_eq!(for_choice("gone", false), "breeze-light");
        assert_eq!(Theme::built_in(for_choice("oled", false)).id(), "dark-oled");
    }

    #[test]
    fn the_oled_theme_is_black_where_the_window_is() {
        let theme = Theme::built_in("dark-oled");
        assert!(theme.dark);
        let window = theme.color("surface.window").unwrap();
        assert!(window.r == 0.0 && window.g == 0.0 && window.b == 0.0 && close(window.a, 1.0));
    }

    #[test]
    fn every_built_in_theme_resolves_every_token() {
        for (id, json) in BUILT_IN {
            let theme = Theme::parse(json).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(theme.id, *id);
        }
        assert!(Theme::built_in("breeze-dark").dark);
        assert!(!Theme::built_in("breeze-light").dark);
    }

    #[test]
    fn colors_read_the_way_the_design_writes_them() {
        let c = parse_color("#A3161F").unwrap();
        assert!(close(c.r, 163.0 / 255.0) && close(c.a, 1.0));
        let c = parse_color("rgba(35,38,41,0.10)").unwrap();
        assert!(close(c.g, 38.0 / 255.0) && close(c.a, 0.10));
        assert!(parse_color("#12345").is_none());
        assert!(parse_color("crimson").is_none(), "named colors are not tokens");
    }

    #[test]
    fn under_the_background_a_light_theme_hardens_its_weak_text_and_a_dark_one_does_not() {
        let light = Theme::built_in("breeze-light");
        let (secondary, primary) = (light.color("text.secondary").unwrap(), light.color("text.primary").unwrap());
        let hardened = light.color_under("text.secondary", true).unwrap();
        assert!(close(hardened.r, secondary.r + (primary.r - secondary.r) * 0.40));
        let tertiary = light.color("text.tertiary").unwrap();
        let hardened = light.color_under("text.tertiary", true).unwrap();
        assert!(close(hardened.g, tertiary.g + (primary.g - tertiary.g) * 0.50));
        assert_eq!(light.color_under("text.secondary", false), Some(secondary), "off, nothing moves");
        assert_eq!(light.color_under("accent", true), light.color("accent"), "only the two weak text tokens");
        let dark = Theme::built_in("breeze-dark");
        assert_eq!(dark.color_under("text.secondary", true), dark.color("text.secondary"));
    }

    #[test]
    fn the_ambient_tokens_are_the_surfaces_at_their_alpha() {
        for id in ["breeze-light", "breeze-dark", "dark-oled"] {
            let theme = Theme::built_in(id);
            let (sidebar, ambient) = (theme.color("surface.sidebar").unwrap(), theme.color("ambient.sidebar").unwrap());
            assert_eq!((ambient.r, ambient.g, ambient.b), (sidebar.r, sidebar.g, sidebar.b), "{id}");
            assert!(close(ambient.a, 0.50), "{id}");
            let (player, base) = (theme.color("ambient.player").unwrap(), theme.color("surface.player").unwrap());
            assert!(close(player.a, 0.82), "{id}");
            assert!(player.r + player.g + player.b < base.r + base.g + base.b, "{id}: darker than the plain bar");
            assert!(close(theme.color("ambient.content").unwrap().a, 0.22), "{id}");
            assert!(close(theme.color("ambient.raised").unwrap().a, 0.50), "{id}");
            assert!(close(theme.color("ambient.frost").unwrap().a, 0.10), "{id}: the plane's hairline");
            assert!(close(theme.color("ambient.bar").unwrap().a, 0.60), "{id}: a bar that sticks over scrolling rows");
            assert!(close(theme.color("ambient.panel").unwrap().a, 0.50), "{id}");
            let veil = theme.color("ambient.veil").unwrap();
            assert!(veil.r == 0.0 && veil.g == 0.0 && veil.b == 0.0, "{id}");
        }
    }

    #[test]
    fn a_selected_row_is_the_accent_at_the_themes_alpha() {
        let light = Theme::built_in("breeze-light");
        let accent = light.color("accent").unwrap();
        let row = light.color("row.selected").unwrap();
        assert_eq!((row.r, row.g, row.b), (accent.r, accent.g, accent.b));
        assert!(close(row.a, 0.10));
        assert!(close(Theme::built_in("breeze-dark").color("row.selected").unwrap().a, 0.18));
    }

    #[test]
    fn hover_is_lighter_and_pressed_darker_than_the_accent() {
        let theme = Theme::built_in("breeze-light");
        let sum = |c: Rgba| c.r + c.g + c.b;
        let accent = sum(theme.color("accent").unwrap());
        assert!(sum(theme.color("accent.hover").unwrap()) > accent);
        assert!(sum(theme.color("accent.pressed").unwrap()) < accent);
    }

    #[test]
    fn overriding_the_accent_carries_its_derived_tokens_along() {
        let mut theme = Theme::built_in("breeze-light");
        let blue = parse_color("#3DAEE9").unwrap();
        theme.set_accent(Some(blue));
        let row = theme.color("row.selected").unwrap();
        assert_eq!((row.r, row.g, row.b), (blue.r, blue.g, blue.b));
        assert!(close(row.a, 0.10));
    }

    #[test]
    fn a_theme_missing_a_token_is_refused() {
        let broken = r##"{"id":"x","color":{"accent":"#FFFFFF","art.placeholder":["#000"]}}"##;
        let err = Theme::parse(broken).unwrap_err();
        assert!(err.contains("surface.window"), "{err}");
    }

    #[test]
    fn the_theme_s_own_accent_is_still_there_under_a_chosen_one() {
        let mut theme = Theme::built_in("breeze-light");
        let own = theme.color("accent");
        theme.set_accent(parse_color("#3DAEE9"));
        assert_ne!(theme.color("accent"), own);
        assert_eq!(theme.color("accent.base"), own);
    }
}
