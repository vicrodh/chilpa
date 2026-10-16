//! The lyrics that came with the music.
//!
//! Three places, all on disk: a `.ttml` or an `.lrc` next to the file with
//! the same name, and the lyrics stored in the file's tags. When more than
//! one is there, the most finely timed wins; on a tie, the file next to the
//! track wins over the tags, because somebody put it there on purpose.

use crate::cache::Origin;
use crate::model::Document;
use crate::{lrc, ttml};
use std::path::{Path, PathBuf};

/// The largest file next to a track that is read as lyrics.
const MAX_FILE: u64 = 2 * 1024 * 1024;

/// Lyrics found with a track.
#[derive(Clone, PartialEq, Debug)]
pub struct Found {
    pub document: Document,
    pub origin: Origin,
    /// The file they came from, when they did not come from the tags.
    pub file: Option<PathBuf>,
}

/// Looks for a track's own lyrics.
///
/// `None` when there are none, which is not the same as empty lyrics: empty
/// lyrics in the tags must not hide the file next door.
pub fn find(track: &Path) -> Option<Found> {
    let mut best: Option<Found> = None;
    let mut consider = |found: Found| {
        if found.document.is_empty() {
            return;
        }
        if best.as_ref().is_none_or(|b| found.document.timing > b.document.timing) {
            best = Some(found);
        }
    };
    for (path, is_ttml) in sidecars(track) {
        let Some(text) = read_small(&path) else { continue };
        let document = if is_ttml { ttml::parse(&text) } else { Some(lrc::parse(&text)) };
        if let Some(document) = document {
            consider(Found { document, origin: origin("file"), file: Some(path) });
        }
    }
    if let Some(text) = embedded(track) {
        consider(Found { document: lrc::parse(&text), origin: origin("tags"), file: None });
    }
    best
}

/// The files next to a track that may hold its lyrics, TTML first.
///
/// Every common case of the extension is tried: on a disc that came from
/// Windows it may be uppercase, and on Linux that matters.
pub fn sidecars(track: &Path) -> Vec<(PathBuf, bool)> {
    let mut out = Vec::new();
    for (ext, is_ttml) in [("ttml", true), ("TTML", true), ("lrc", false), ("LRC", false), ("Lrc", false)] {
        let candidate = track.with_extension(ext);
        if candidate.is_file() && !out.iter().any(|(_, t)| *t == is_ttml) {
            out.push((candidate, is_ttml));
        }
    }
    out
}

fn origin(kind: &str) -> Origin {
    Origin { provider: kind.to_string(), name: String::new() }
}

fn read_small(path: &Path) -> Option<String> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > MAX_FILE {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    // Old LRC files are often in a one-byte encoding. Reading them lossy keeps the lines
    // and replaces only what cannot be read.
    Some(String::from_utf8_lossy(&bytes).into_owned())
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
    use crate::model::Timing;

    fn folder(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("chilpa-lyrics-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn the_lrc_next_to_the_file_is_found() {
        let dir = folder("sidecar");
        let track = dir.join("song.flac");
        std::fs::write(&track, b"the content does not matter").unwrap();
        std::fs::write(dir.join("song.lrc"), "[00:01.00]From next door\n").unwrap();

        let found = find(&track).expect("it was found");
        assert_eq!(found.origin.provider, "file");
        assert_eq!(found.file, Some(dir.join("song.lrc")));
        assert_eq!(found.document.timing, Timing::Line);
        assert_eq!(found.document.lines[0].text, "From next door");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_finer_file_wins() {
        let dir = folder("finer");
        let track = dir.join("song.flac");
        std::fs::write(&track, b"x").unwrap();
        std::fs::write(dir.join("song.lrc"), "[00:01.00]Lines only\n").unwrap();
        std::fs::write(
            dir.join("song.ttml"),
            r#"<tt xmlns="http://www.w3.org/ns/ttml"><body><div><p begin="1" end="2"><span begin="1" end="1.5">Word</span> <span begin="1.5" end="2">timed</span></p></div></body></tt>"#,
        )
        .unwrap();
        let found = find(&track).unwrap();
        assert_eq!(found.document.timing, Timing::Word);
        assert_eq!(found.file, Some(dir.join("song.ttml")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn with_no_lyrics_nothing_is_invented() {
        let dir = folder("empty");
        let track = dir.join("song.flac");
        std::fs::write(&track, b"it does not matter either").unwrap();
        assert!(find(&track).is_none());

        // An empty .lrc does not count as lyrics either.
        std::fs::write(dir.join("song.lrc"), "\n\n").unwrap();
        assert!(find(&track).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_uppercase_extension_is_found_too() {
        let dir = folder("uppercase");
        let track = dir.join("song.flac");
        std::fs::write(&track, b"x").unwrap();
        std::fs::write(dir.join("song.LRC"), "[00:01.00]Up there\n").unwrap();
        assert!(find(&track).is_some(), "on Linux the extension is case sensitive");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn files_in_old_encodings_are_still_read() {
        let dir = folder("old-encoding");
        let track = dir.join("song.flac");
        std::fs::write(&track, b"x").unwrap();
        std::fs::write(dir.join("song.lrc"), b"[00:01.00]Caf\xe9\n").unwrap();
        let found = find(&track).unwrap();
        assert!(found.document.lines[0].text.starts_with("Caf"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
