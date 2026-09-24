//! The lyrics that already come with the music.
//!
//! This crate talks to nobody over the network: it looks for what is on
//! disk, which is where the lyrics you actually have come from. Two places,
//! in this order:
//!
//! 1. An `.lrc` next to the file, with the same name. It is the one that
//!    usually carries timings, which is why it goes first.
//! 2. The lyrics stored inside the file, in its tags.
//!
//! Timed lyrics can be followed while the track plays; untimed ones are
//! shown whole. Both are fine, and which is which is stated, not guessed.


pub mod lrc;

use std::path::{Path, PathBuf};

/// One line of the lyrics, with the moment it starts.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Line {
    /// Milliseconds from the start of the track. Zero in unsynced lyrics.
    
    pub at_ms: u64,
    pub text: String,
}

/// A track's lyrics.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Lyrics {
    /// What the lyrics say about themselves, when they say anything.
    pub title: Option<String>,
    pub artist: Option<String>,
    pub lines: Vec<Line>,
    /// True when they carry timings and can be followed while playing.
    pub synced: bool,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Which line is current at that moment, by position in `lines`.
    ///
    /// It is the last one that has started. Before the first there is none:
    /// showing the first line from second zero, when it starts at twenty, is
    /// worse than showing nothing.
    pub fn line_at(&self, ms: u64) -> Option<usize> {
        if !self.synced || self.lines.is_empty() {
            return None;
        }
        // `partition_point` gives how many lines have started.
        let ya = self.lines.partition_point(|l| l.at_ms <= ms);
        ya.checked_sub(1)
    }

    /// The lyrics as plain text, one line each.
    pub fn plain(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

/// Where the lyrics came from. Used to say so in the view.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Source {
    /// An `.lrc` file next to the track.
    Sidecar(PathBuf),
    /// The file's tags.
    Embedded,
}

/// Lyrics that were found, with their origin.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Found {
    pub lyrics: Lyrics,
    pub source: Source,
}

/// Looks for a track's lyrics.
///
/// Returns `None` when there are none, which is not the same as empty
/// lyrics: empty lyrics in the tags must not hide the `.lrc` next door.
pub fn find(track: &Path) -> Option<Found> {
    if let Some(path) = sidecar(track) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let lyrics = lrc::parse(&text);
            if !lyrics.is_empty() {
                return Some(Found { lyrics, source: Source::Sidecar(path) });
            }
        }
    }
    let text = embedded(track)?;
    let lyrics = lrc::parse(&text);
    (!lyrics.is_empty()).then_some(Found { lyrics, source: Source::Embedded })
}

/// The `.lrc` that belongs to a track, if it is there.
///
/// Both `.lrc` and `.LRC` are tried: on a disc that came from Windows the
/// extension may be uppercase, and on Linux that matters.
pub fn sidecar(track: &Path) -> Option<PathBuf> {
    for ext in ["lrc", "LRC", "Lrc"] {
        let candidate = track.with_extension(ext);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The lyrics stored inside the file.
fn embedded(track: &Path) -> Option<String> {
    use lofty::file::TaggedFileExt;
    use lofty::prelude::ItemKey;

    let file = lofty::probe::Probe::open(track).ok()?.read().ok()?;
    let tag = file.primary_tag().or_else(|| file.first_tag())?;
    let text = tag.get_string(ItemKey::Lyrics)?;
    (!text.trim().is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-lyrics-{name}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn the_current_line_is_the_last_one_that_started() {
        let l = lrc::parse("[00:05.00]Uno\n[00:10.00]Dos\n[00:20.00]Tres\n");
        assert_eq!(l.line_at(0), None, "antes del primero no hay ninguno");
        assert_eq!(l.line_at(4_999), None);
        assert_eq!(l.line_at(5_000), Some(0), "justo cuando entra");
        assert_eq!(l.line_at(9_999), Some(0));
        assert_eq!(l.line_at(10_000), Some(1));
        assert_eq!(l.line_at(999_999), Some(2), "y el último se queda");
    }

    #[test]
    fn lyrics_without_timings_are_not_followed() {
        let l = lrc::parse("Nomás el texto\n");
        assert_eq!(l.line_at(5_000), None);
        assert_eq!(l.plain(), "Nomás el texto");
    }

    #[test]
    fn the_lrc_next_to_the_file_wins() {
        let dir = folder("sidecar");
        let track = dir.join("cancion.flac");
        std::fs::write(&track, b"no importa el contenido").unwrap();
        std::fs::write(dir.join("cancion.lrc"), "[00:01.00]De al lado\n").unwrap();

        let found = find(&track).expect("se encontró");
        assert_eq!(found.source, Source::Sidecar(dir.join("cancion.lrc")));
        assert!(found.lyrics.synced);
        assert_eq!(found.lyrics.lines[0].text, "De al lado");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn with_no_lyrics_nothing_is_invented() {
        let dir = folder("vacio");
        let track = dir.join("cancion.flac");
        std::fs::write(&track, b"tampoco importa").unwrap();
        assert!(find(&track).is_none());

        // An empty .lrc does not count as lyrics either.
        std::fs::write(dir.join("cancion.lrc"), "\n\n").unwrap();
        assert!(find(&track).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_uppercase_extension_is_found_too() {
        let dir = folder("mayusculas");
        let track = dir.join("cancion.flac");
        std::fs::write(&track, b"x").unwrap();
        std::fs::write(dir.join("cancion.LRC"), "[00:01.00]Arriba\n").unwrap();
        assert!(find(&track).is_some(), "en Linux la extensión distingue");
        let _ = std::fs::remove_dir_all(dir);
    }
}
