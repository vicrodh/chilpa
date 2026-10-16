//! What Chilpa remembers between sessions.
//!
//! A text file with `key = value`, in the user's configuration directory. No
//! format that forces a dependency: what is kept here is a handful of
//! preferences, and a file any editor can open is fixed by hand when
//! something goes wrong.
//!
//! Reading never fails: a missing, broken or older file gives the default
//! values. Saving can fail, and the caller decides what to do.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// The preferences remembered today.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Settings {
    /// The identifier of the last output used.
    pub output: String,
    /// The last folder opened, so as not to start at home every time.
    pub last_folder: String,
    /// The window layout, by name.
    pub layout: String,
    /// The quick view the library is looked at with.
    pub album_sort: String,
    /// Which visualizer is drawn, by name.
    pub visualizer: String,
    /// The language the window speaks, as a two letter code. Empty means
    /// whatever the environment asks for, which is right until someone
    /// chooses otherwise.
    pub language: String,
    /// The window's own preferences, by their dotted key:
    /// `appearance.theme`, `ui.sidebar.width`. The engine reads none of
    /// them; they are kept in the same file so there is one place to look.
    pub window: BTreeMap<String, String>,
}

/// The visualizer someone who has chosen none starts with.
pub const DEFAULT_VISUALIZER: &str = "wave";

/// The layout someone who has chosen none starts with.
pub const DEFAULT_LAYOUT: &str = "Classic";

impl Default for Settings {
    fn default() -> Self {
        Self {
            output: String::new(),
            last_folder: String::new(),
            layout: DEFAULT_LAYOUT.to_string(),
            album_sort: String::new(),
            visualizer: DEFAULT_VISUALIZER.to_string(),
            language: String::new(),
            window: BTreeMap::new(),
        }
    }
}

/// Where the file lives.
pub fn path() -> PathBuf {
    chilpa_dirs::config().join("settings.conf")
}

/// Reads the preferences. Whatever is missing or not understood takes its
/// default value.
pub fn load() -> Settings {
    load_from(&path())
}

/// An output identifier as it is written today, out of one that may have
/// been written before stacks existed.
///
/// Until there were stacks an identifier was whatever ALSA called the
/// device, and there is only one thing it could have meant: a `hw:` name was
/// the card and `capture` was the file. Reading it that way costs one line
/// and saves everybody who already has a settings file from being silently
/// moved to a different output the first time they open a new Chilpa.
fn carried_over(id: &str) -> String {
    if chilpa_audio::backend::BackendOutput::split(id).is_some() {
        return id.to_string();
    }
    let kind = if id == chilpa_audio::CAPTURE_ID {
        chilpa_audio::backend::BackendKind::File
    } else if id.starts_with("hw:") {
        chilpa_audio::backend::BackendKind::Alsa
    } else if id.is_empty() {
        return String::new();
    } else {
        chilpa_audio::backend::BackendKind::System
    };
    chilpa_audio::backend::BackendOutput::full_id(kind, id)
}

pub fn load_from(file: &std::path::Path) -> Settings {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let pairs = parse(&text);
    let mut s = Settings::default();
    if let Some(v) = pairs.get("output") {
        s.output = carried_over(v);
    }
    if let Some(v) = pairs.get("last_folder") {
        s.last_folder = v.clone();
    }
    // An empty layout in the file does not leave the window with nothing to load.
    if let Some(v) = pairs.get("layout") {
        if !v.is_empty() {
            s.layout = renamed(v, OLD_LAYOUTS);
        }
    }
    if let Some(v) = pairs.get("album_sort") {
        s.album_sort = v.clone();
    }
    if let Some(v) = pairs.get("visualizer") {
        if !v.is_empty() {
            s.visualizer = renamed(v, OLD_VISUALIZERS);
        }
    }
    if let Some(v) = pairs.get("language") {
        s.language = v.clone();
    }
    // The engine's keys are plain words; a dotted key is the window's.
    s.window = pairs.into_iter().filter(|(k, _)| k.contains('.')).collect();
    s
}

/// The names these values had before the code moved to English. An old file
/// still opens the same window.
const OLD_LAYOUTS: &[(&str, &str)] = &[("Clasico", "Classic"), ("Compacto", "Compact")];
const OLD_VISUALIZERS: &[(&str, &str)] =
    &[("onda", "wave"), ("espectro", "spectrum"), ("goniometro", "goniometer")];

fn renamed(value: &str, old: &[(&str, &str)]) -> String {
    old.iter()
        .find(|(from, _)| *from == value)
        .map(|(_, to)| (*to).to_string())
        .unwrap_or_else(|| value.to_string())
}

/// Saves the preferences, creating the directory if needed.
pub fn save(settings: &Settings) -> std::io::Result<()> {
    save_to(&path(), settings)
}

pub fn save_to(file: &std::path::Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = format!(
        "# Chilpa's preferences. Editing this by hand is fine.\noutput = {}\nlast_folder = {}\nlayout = {}\nalbum_sort = {}\nvisualizer = {}\nlanguage = {}\n",
        settings.output,
        settings.last_folder,
        settings.layout,
        settings.album_sort,
        settings.visualizer,
        settings.language
    );
    for (key, value) in &settings.window {
        text.push_str(&format!("{key} = {value}\n"));
    }
    std::fs::write(file, text)
}

fn parse(text: &str) -> BTreeMap<String, String> {
    text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_settings_file_written_before_stacks_still_names_its_output() {
        // Until there were stacks an identifier was whatever ALSA called the
        // device. Reading one as "no stack, start over" would quietly move
        // everybody who already has a settings file onto a different output,
        // and the one they would land on is the one that resamples.
        assert_eq!(carried_over("hw:1,0"), "alsa:hw:1,0");
        assert_eq!(carried_over("capture"), "file:capture");
        assert_eq!(carried_over("default"), "system:default");
        assert_eq!(carried_over(""), "", "nothing remembered stays nothing");
        // And one that already names its stack is left alone.
        assert_eq!(carried_over("pulse:alsa_output.x"), "pulse:alsa_output.x");
    }

    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-settings-{name}.conf"));
        p
    }

    #[test]
    fn with_no_file_the_defaults_are_used() {
        let s = load_from(&temporary("missing"));
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn what_was_saved_reads_back_the_same() {
        let file = temporary("round-trip");
        let s = Settings {
            output: "alsa:hw:1,0".into(),
            last_folder: "/music/albums".into(),
            layout: "Column".into(),
            album_sort: "played".into(),
            visualizer: "spectrum".into(),
            language: "es".into(),
            window: [("appearance.theme", "dark"), ("ui.sidebar.width", "240")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        save_to(&file, &s).unwrap();
        assert_eq!(load_from(&file), s);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn the_old_spanish_names_still_open_the_same_window() {
        // A settings.conf written before the code moved to English cannot
        // leave the window without a layout or a visualizer.
        let file = temporary("old-names");
        std::fs::write(&file, "layout = Clasico\nvisualizer = espectro\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.layout, "Classic");
        assert_eq!(s.visualizer, "spectrum");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_broken_file_brings_nothing_down() {
        let file = temporary("broken");
        std::fs::write(&file, "this is not\na valid file }{\noutput = hw:0,0\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.output, "alsa:hw:0,0", "what does parse is salvaged");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn with_no_saved_layout_the_window_still_has_something_to_start_with() {
        let file = temporary("no-layout");
        std::fs::write(&file, "layout =\n").unwrap();
        assert_eq!(load_from(&file).layout, DEFAULT_LAYOUT);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn the_window_keeps_its_preferences_without_touching_the_engines() {
        let file = temporary("window");
        std::fs::write(&file, "output = hw:1,0\nappearance.density = rich\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.window.get("appearance.density").map(String::as_str), Some("rich"));
        assert!(!s.window.contains_key("output"), "a plain key is the engine's");
        save_to(&file, &s).unwrap();
        assert_eq!(load_from(&file), s, "and it survives a save");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn comments_and_whitespace_do_not_get_in_the_way() {
        let file = temporary("comments");
        std::fs::write(&file, "# a comment\n  output   =   hw:2,0  \nbit_perfect = false\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.output, "alsa:hw:2,0");
        assert!(!s.window.contains_key("bit_perfect"), "the retired setting is ignored");
        let _ = std::fs::remove_file(file);
    }
}
