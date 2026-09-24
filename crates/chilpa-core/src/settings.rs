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
    /// Whether bit-perfect is asked for.
    pub bit_perfect: bool,
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
}

/// The visualizer someone who has chosen none starts with.
pub const DEFAULT_VISUALIZER: &str = "wave";

/// The layout someone who has chosen none starts with.
pub const DEFAULT_LAYOUT: &str = "Classic";

impl Default for Settings {
    fn default() -> Self {
        Self {
            output: String::new(),
            bit_perfect: true,
            last_folder: String::new(),
            layout: DEFAULT_LAYOUT.to_string(),
            album_sort: String::new(),
            visualizer: DEFAULT_VISUALIZER.to_string(),
            language: String::new(),
        }
    }
}

/// Where the file lives.
pub fn path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("chilpa").join("settings.conf")
}

/// Reads the preferences. Whatever is missing or not understood takes its
/// default value.
pub fn load() -> Settings {
    load_from(&path())
}

pub fn load_from(file: &std::path::Path) -> Settings {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let pares = parse(&text);
    let mut s = Settings::default();
    if let Some(v) = pares.get("output") {
        s.output = v.clone();
    }
    if let Some(v) = pares.get("bit_perfect") {
        s.bit_perfect = v != "false";
    }
    if let Some(v) = pares.get("last_folder") {
        s.last_folder = v.clone();
    }
    // An empty layout in the file does not leave the window with nothing to load.
    if let Some(v) = pares.get("layout") {
        if !v.is_empty() {
            s.layout = renamed(v, OLD_LAYOUTS);
        }
    }
    if let Some(v) = pares.get("album_sort") {
        s.album_sort = v.clone();
    }
    if let Some(v) = pares.get("visualizer") {
        if !v.is_empty() {
            s.visualizer = renamed(v, OLD_VISUALIZERS);
        }
    }
    if let Some(v) = pares.get("language") {
        s.language = v.clone();
    }
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
    let text = format!(
        "# Chilpa's preferences. Editing this by hand is fine.\noutput = {}\nbit_perfect = {}\nlast_folder = {}\nlayout = {}\nalbum_sort = {}\nvisualizer = {}\nlanguage = {}\n",
        settings.output,
        settings.bit_perfect,
        settings.last_folder,
        settings.layout,
        settings.album_sort,
        settings.visualizer,
        settings.language
    );
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
    use super::*;

    fn temporal(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-settings-{name}.conf"));
        p
    }

    #[test]
    fn with_no_file_the_defaults_are_used() {
        let s = load_from(&temporal("no-existe"));
        assert_eq!(s, Settings::default());
        assert!(s.bit_perfect, "bit-perfect viene encendido");
    }

    #[test]
    fn what_was_saved_reads_back_the_same() {
        let file = temporal("ida-y-vuelta");
        let s = Settings {
            output: "hw:1,0".into(),
            bit_perfect: false,
            last_folder: "/musica/albumes".into(),
            layout: "Columna".into(),
            album_sort: "played".into(),
            visualizer: "spectrum".into(),
        };
        save_to(&file, &s).unwrap();
        assert_eq!(load_from(&file), s);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn the_old_spanish_names_still_open_the_same_window() {
        // A settings.conf written before the code moved to English cannot
        // leave the window without a layout or a visualizer.
        let file = temporal("nombres-viejos");
        std::fs::write(&file, "layout = Clasico\nvisualizer = espectro\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.layout, "Classic");
        assert_eq!(s.visualizer, "spectrum");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_broken_file_brings_nothing_down() {
        let file = temporal("roto");
        std::fs::write(&file, "esto no es\nun archivo válido }{\noutput = hw:0,0\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.output, "hw:0,0", "se rescata lo que sí se entiende");
        assert!(s.bit_perfect, "lo demás toma su valor por omisión");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn with_no_saved_layout_the_window_still_has_something_to_start_with() {
        let file = temporal("sin-layout");
        std::fs::write(&file, "layout =\n").unwrap();
        assert_eq!(load_from(&file).layout, DEFAULT_LAYOUT);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn comments_and_whitespace_do_not_get_in_the_way() {
        let file = temporal("comentarios");
        std::fs::write(&file, "# un comentario\n  output   =   hw:2,0  \nbit_perfect = false\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.output, "hw:2,0");
        assert!(!s.bit_perfect);
        let _ = std::fs::remove_file(file);
    }
}
