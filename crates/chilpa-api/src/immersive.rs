//! The immersive view as the facade keeps it: which view it opens with,
//! the one it was last on, the shader scene it was last on, and what
//! Enter does in its search. Port of QBZ's `immersive_bridge.rs` and the
//! immersive half of its `search_qt.rs`, without the Qt half: the window
//! asks [`Chilpa::immersive_opening`] on the open edge and tells
//! [`Chilpa::remember_immersive_view`] when the view changes.
//!
//! The numbers are QBZ's: `view_mode` 0 focus, 1 split; `mode` 0 Album
//! Reactive, 1 Static, 2 Coverflow, 3 Spectrum, 4 Lyrics, 5 Queue, 6 Wave
//! Bed, 7 Goniometer, 8 Oscilloscope, 9 Reactive Rings; `split_panel` 0
//! Lyrics, 3 Queue, 4 Now Playing, 5 Now Playing with one line of lyrics
//! (1 and 2 were Track Info and Suggestions, which Chilpa leaves out).

use crate::ambient::{hex, hsl_to_rgb, rgb_to_hsl};
use crate::found::FoundRow;
use crate::settings_rows::SettingsRow;
use crate::{Chilpa, Found};
use chilpa_i18n as i18n;

/// The view it opens with: one of [`VIEWS`]. Its row is in Appearance.
pub const DEFAULT_VIEW: &str = "appearance.immersiveDefaultView";
/// What Enter does on a search result: one of [`ACTIONS`].
pub const SEARCH_ACTION: &str = "appearance.immersiveSearchAction";
/// The view it was last on, kept only while the default is "remember".
pub const LAST_VIEW_MODE: &str = "immersive.lastViewMode";
pub const LAST_MODE: &str = "immersive.lastMode";
pub const LAST_SPLIT_PANEL: &str = "immersive.lastSplitPanel";
/// The shader scene it was last on, 0 for none.
pub const LAST_SCENE: &str = "immersive.lastScene";

pub const VIEWS: [&str; 7] = ["remember", "reactive", "static", "coverflow", "spectrum", "lyrics", "queue"];
pub const ACTIONS: [&str; 4] = ["disabled", "replace", "next", "queue"];

/// With nothing remembered it opens on Split with the lyrics: never on a
/// focus panel.
pub const DEFAULT_VIEW_MODE: i32 = 1;
pub const DEFAULT_MODE: i32 = 0;
pub const DEFAULT_SPLIT_PANEL: i32 = 0;

/// The shipped scenes, in the order the `g` key walks them; 6 is parked.
pub const SCENES: [i32; 7] = [1, 2, 3, 4, 5, 7, 8];

/// How many of each the immersive search shows, in this order: artists,
/// albums, playlists. No songs, no top result, no way to more.
const SEARCH_CAPS: [(usize, &str, &str); 3] = [(2, "Artists", "artists"), (5, "Albums", "albums"), (2, "Playlists", "playlists")];

/// The immersive view's accent: the cover's primary color lifted so it
/// reads over the field, saturation between 0.45 and 0.85 and lightness
/// between 0.66 and 0.80; a grey primary gives white at 0.92.
pub fn lifted_accent(primary: &str) -> String {
    let Some((r, g, b)) = parse(primary) else { return "#ebffffff".into() };
    let (h, s, l) = rgb_to_hsl(r, g, b);
    if s < 0.05 {
        return "#ebffffff".into();
    }
    hex(hsl_to_rgb(h, s.clamp(0.45, 0.85), l.clamp(0.66, 0.80)))
}

/// The brightest of the cover's three colors, for a scope's trace.
pub fn brightest(colors: [&str; 3]) -> String {
    colors
        .iter()
        .filter_map(|c| parse(c).map(|(r, g, b)| (c, 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b))))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| (*c).to_string())
        .unwrap_or_else(|| "#ffffff".into())
}

/// The six colors the reactive rings cycle through: the triad, then each
/// lightened by 1.32, 1.24 and 1.30 of its lightness.
pub fn ring_palette(primary: &str, accent: &str, secondary: &str) -> [String; 6] {
    let lighter = |c: &str, by: f32| {
        parse(c).map(|(r, g, b)| {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            hex(hsl_to_rgb(h, s, (l * by).min(1.0)))
        }).unwrap_or_else(|| c.to_string())
    };
    [primary.into(), accent.into(), secondary.into(), lighter(primary, 1.32), lighter(accent, 1.24), lighter(secondary, 1.30)]
}

/// The six run into a ramp of `steps` colors between each neighbor and
/// back to the first, as a JSON list: the window picks one by position
/// instead of mixing colors itself.
pub fn ring_ramp_json(primary: &str, accent: &str, secondary: &str, steps: usize) -> String {
    let palette = ring_palette(primary, accent, secondary);
    let bytes: Vec<(u8, u8, u8)> = palette.iter().map(|c| parse(c).unwrap_or((255, 255, 255))).collect();
    let mut ramp = Vec::with_capacity(bytes.len() * steps);
    for (i, a) in bytes.iter().enumerate() {
        let b = bytes[(i + 1) % bytes.len()];
        for step in 0..steps.max(1) {
            let mix = step as f32 / steps.max(1) as f32;
            let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * mix).round() as u8;
            ramp.push(hex((channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))));
        }
    }
    serde_json::to_string(&ramp).unwrap_or_else(|_| "[]".into())
}

/// The same six, as a JSON list for the window.
pub fn ring_palette_json(primary: &str, accent: &str, secondary: &str) -> String {
    serde_json::to_string(&ring_palette(primary, accent, secondary)).unwrap_or_else(|_| "[]".into())
}

/// `#rrggbb` to its bytes; anything else is nothing.
fn parse(color: &str) -> Option<(u8, u8, u8)> {
    let digits = color.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// The view the immersive overlay opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub view_mode: i32,
    pub mode: i32,
    pub split_panel: i32,
    pub scene: i32,
}

/// A pinned default always lands on a focus panel: the matching mode.
/// `None` for "remember" or anything unknown.
pub fn pinned_view(key: &str) -> Option<(i32, i32)> {
    match key {
        "reactive" => Some((0, 0)),
        "static" => Some((0, 1)),
        "coverflow" => Some((0, 2)),
        "spectrum" => Some((0, 3)),
        "lyrics" => Some((0, 4)),
        "queue" => Some((0, 5)),
        _ => None,
    }
}

/// The two scopes are native scene-graph traces; without a GPU only their
/// guides would survive, so a remembered scope lands on Spectrum instead.
/// The remembered mode stays stored and comes back on a GPU.
pub fn focus_mode_for_tier(view_mode: i32, mode: i32, accelerated: bool) -> i32 {
    if view_mode == 0 && !accelerated && matches!(mode, 7 | 8) { 3 } else { mode }
}

/// A split panel Chilpa has: Track Info and Suggestions fall to the
/// lyrics, as does anything out of range.
pub fn split_panel_kept(panel: i32) -> i32 {
    if matches!(panel, 0 | 3 | 4 | 5) { panel } else { DEFAULT_SPLIT_PANEL }
}

/// The scene after `scene` on the `g` key: through the shipped ones and
/// back to none.
pub fn next_scene(scene: i32) -> i32 {
    match SCENES.iter().position(|s| *s == scene) {
        Some(i) if i + 1 < SCENES.len() => SCENES[i + 1],
        Some(_) => 0,
        None => SCENES[0],
    }
}

fn accepted(key: &str, value: &str) -> Option<String> {
    let in_range = |lo: i32, hi: i32| value.trim().parse::<i32>().ok().filter(|n| (lo..=hi).contains(n)).map(|n| n.to_string());
    match key {
        DEFAULT_VIEW => Some(if VIEWS.contains(&value) { value } else { VIEWS[0] }.to_string()),
        SEARCH_ACTION => Some(if ACTIONS.contains(&value) { value } else { ACTIONS[1] }.to_string()),
        LAST_VIEW_MODE => in_range(0, 1),
        LAST_MODE => in_range(0, 9),
        LAST_SPLIT_PANEL => in_range(0, 5),
        LAST_SCENE => in_range(0, 8),
        _ => None,
    }
}

impl Chilpa {
    pub fn immersive_default_view(&self) -> String {
        self.preference(DEFAULT_VIEW).filter(|v| VIEWS.contains(&v.as_str())).unwrap_or_else(|| VIEWS[0].to_string())
    }

    /// What Enter does on a result; "disabled" also keeps the search shut.
    pub fn immersive_search_action(&self) -> String {
        self.preference(SEARCH_ACTION).filter(|v| ACTIONS.contains(&v.as_str())).unwrap_or_else(|| ACTIONS[1].to_string())
    }

    fn immersive_number(&self, key: &str, fallback: i32) -> i32 {
        self.preference(key).and_then(|v| v.parse().ok()).unwrap_or(fallback)
    }

    /// The view to open on now: the one remembered when the default is
    /// "remember", the pinned panel otherwise, both brought to what this
    /// renderer can draw. A pinned default opens with no scene.
    pub fn immersive_opening(&self, accelerated: bool) -> View {
        let key = self.immersive_default_view();
        let (view_mode, mode, scene) = match pinned_view(&key) {
            Some((view_mode, mode)) => (view_mode, mode, 0),
            None => (
                self.immersive_number(LAST_VIEW_MODE, DEFAULT_VIEW_MODE).clamp(0, 1),
                self.immersive_number(LAST_MODE, DEFAULT_MODE).clamp(0, 9),
                if accelerated { self.immersive_number(LAST_SCENE, 0) } else { 0 },
            ),
        };
        let scene = if SCENES.contains(&scene) { scene } else { 0 };
        View {
            view_mode,
            mode: focus_mode_for_tier(view_mode, mode, accelerated),
            split_panel: split_panel_kept(self.immersive_number(LAST_SPLIT_PANEL, DEFAULT_SPLIT_PANEL)),
            scene,
        }
    }

    /// The view changed: kept for the next open while the default is
    /// "remember"; a pinned default wins the next open and is not
    /// written over.
    pub fn remember_immersive_view(&mut self, view_mode: i32, mode: i32, split_panel: i32) {
        if self.immersive_default_view() != "remember" {
            return;
        }
        self.immersive_setting(LAST_VIEW_MODE, &view_mode.to_string());
        self.immersive_setting(LAST_MODE, &mode.to_string());
        self.immersive_setting(LAST_SPLIT_PANEL, &split_panel_kept(split_panel).to_string());
    }

    /// The scene changed; kept under the same rule.
    pub fn remember_immersive_scene(&mut self, scene: i32) {
        if self.immersive_default_view() == "remember" {
            self.immersive_setting(LAST_SCENE, &scene.to_string());
        }
    }

    /// Takes one of the immersive view's settings; `false` for a key that
    /// is not its, or a value it cannot take.
    pub(crate) fn immersive_setting(&mut self, key: &str, value: &str) -> bool {
        let Some(kept) = accepted(key, value) else { return false };
        self.remember_preference(key, &kept);
        true
    }

    /// What the two rows say: their choices and the one kept.
    pub(crate) fn immersive_row(&self, row: &mut SettingsRow) {
        for (value, label) in crate::settings_rows::fixed(&row.key) {
            row.values.push(value.to_string());
            row.options.push(label);
        }
        row.value = match row.key.as_str() {
            DEFAULT_VIEW => self.immersive_default_view(),
            _ => self.immersive_search_action(),
        };
    }

    /// The rows of the immersive search: a few artists, albums and
    /// playlists, local and from the sources, each kind under its header.
    /// No songs, no top result and no way to more: a result is acted on,
    /// never opened.
    pub(crate) fn immersive_found_rows(&mut self, found: &Found) -> Vec<FoundRow> {
        let remote = self.source_found_rows("all");
        let mut rows = Vec::new();
        for (cap, label, kind) in SEARCH_CAPS {
            let mut section: Vec<FoundRow> = crate::found_rows(found, kind).into_iter().filter(|r| r.kind() > 0).collect();
            if kind == "playlists" && !self.search.text.trim().is_empty() {
                let words: Vec<_> = self.search.text.to_lowercase().split_whitespace().map(str::to_owned).collect();
                section.extend(self.local_playlists().into_iter().filter(|p| words.iter().all(|w| p.name.to_lowercase().contains(w))).map(FoundRow::Playlist));
            }
            let wanted = match kind { "artists" => 2, "albums" => 3, _ => 6 };
            section.extend(remote.iter().filter(|r| r.kind() == wanted).cloned());
            section.truncate(cap);
            if !section.is_empty() {
                rows.push(FoundRow::Header(i18n::t(i18n::mark(label))));
                rows.extend(section);
            }
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pinned_view_lands_on_its_focus_panel_and_remember_pins_nothing() {
        assert_eq!(pinned_view("reactive"), Some((0, 0)));
        assert_eq!(pinned_view("static"), Some((0, 1)));
        assert_eq!(pinned_view("coverflow"), Some((0, 2)));
        assert_eq!(pinned_view("spectrum"), Some((0, 3)));
        assert_eq!(pinned_view("lyrics"), Some((0, 4)));
        assert_eq!(pinned_view("queue"), Some((0, 5)));
        assert_eq!(pinned_view("remember"), None);
        assert_eq!(pinned_view("wavebed"), None, "a pinned view never lands on Wave Bed");
    }

    #[test]
    fn without_acceleration_a_scope_falls_to_spectrum_only_in_focus() {
        assert_eq!(focus_mode_for_tier(0, 7, false), 3);
        assert_eq!(focus_mode_for_tier(0, 8, false), 3);
        assert_eq!(focus_mode_for_tier(0, 7, true), 7);
        assert_eq!(focus_mode_for_tier(0, 6, false), 6);
        assert_eq!(focus_mode_for_tier(1, 8, false), 8, "split does not rewrite the stored mode");
    }

    #[test]
    fn the_panels_left_out_fall_to_the_lyrics() {
        assert_eq!(split_panel_kept(0), 0);
        assert_eq!(split_panel_kept(1), 0, "Track Info is not here");
        assert_eq!(split_panel_kept(2), 0, "nor Suggestions");
        assert_eq!(split_panel_kept(3), 3);
        assert_eq!(split_panel_kept(5), 5);
        assert_eq!(split_panel_kept(9), 0);
    }

    #[test]
    fn the_g_key_walks_the_shipped_scenes_and_comes_back_to_none() {
        let mut scene = 0;
        let mut walked = Vec::new();
        for _ in 0..8 {
            scene = next_scene(scene);
            walked.push(scene);
        }
        assert_eq!(walked, [1, 2, 3, 4, 5, 7, 8, 0]);
        assert_eq!(next_scene(6), 1, "the parked scene is not on the ring");
    }

    #[test]
    fn the_keys_are_taken_and_kept_in_range() {
        assert_eq!(accepted(DEFAULT_VIEW, "queue").as_deref(), Some("queue"));
        assert_eq!(accepted(DEFAULT_VIEW, "nonsense").as_deref(), Some("remember"));
        assert_eq!(accepted(SEARCH_ACTION, "next").as_deref(), Some("next"));
        assert_eq!(accepted(SEARCH_ACTION, "later").as_deref(), Some("replace"), "an unknown action replaces");
        assert_eq!(accepted(LAST_MODE, "9").as_deref(), Some("9"));
        assert_eq!(accepted(LAST_MODE, "10"), None);
        assert_eq!(accepted(LAST_VIEW_MODE, "2"), None);
        assert_eq!(accepted(LAST_SCENE, "7").as_deref(), Some("7"));
        assert_eq!(accepted("appearance.theme", "dark"), None, "another feature's key");
    }

    #[test]
    fn the_accent_is_lifted_into_its_band_and_gray_turns_white() {
        assert_eq!(lifted_accent("#808080"), "#ebffffff");
        assert_eq!(lifted_accent("nonsense"), "#ebffffff");
        let (r, g, b) = parse(&lifted_accent("#200000")).unwrap();
        let (_, s, l) = rgb_to_hsl(r, g, b);
        assert!((0.44..=0.86).contains(&s) && (0.65..=0.81).contains(&l), "s {s} l {l}");
        assert_eq!(brightest(["#202020", "#e0e0e0", "#808080"]), "#e0e0e0");
        let palette = ring_palette("#200000", "#002000", "#000020");
        assert_eq!(&palette[..3], ["#200000", "#002000", "#000020"]);
        let (r, ..) = parse(&palette[3]).unwrap();
        assert!(r > 0x20, "the lifted copy is lighter: {}", palette[3]);
        let ramp: Vec<String> = serde_json::from_str(&ring_ramp_json("#200000", "#002000", "#000020", 8)).unwrap();
        assert_eq!(ramp.len(), 48);
        assert_eq!(ramp[0], "#200000", "each run starts on its color");
        assert_eq!(ramp[8], "#002000");
    }

    #[test]
    fn the_defaults_are_split_with_the_lyrics_and_no_scene() {
        assert_eq!((DEFAULT_VIEW_MODE, DEFAULT_MODE, DEFAULT_SPLIT_PANEL), (1, 0, 0));
    }
}
