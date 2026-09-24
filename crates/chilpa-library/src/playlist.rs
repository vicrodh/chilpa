//! M3U playlists: reading and writing them.
//!
//! The format is old and simple, which is why it is still the one everybody
//! understands. What matters when reading it:
//!
//! - **Relative paths resolve against the playlist**, not against the
//!   working directory. A playlist inside the album's folder is the normal
//!   case, and resolving it wrong leaves it empty.
//! - **An entry that no longer exists does not bring the playlist down**: it
//!   is reported separately. As with the library, what is missing is said,
//!   not deleted.

use std::io::Write;
use std::path::{Path, PathBuf};

/// What a playlist carries.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Playlist {
    /// The tracks that exist, in the playlist's order.
    pub tracks: Vec<PathBuf>,
    /// The ones the playlist names that are not on disk.
    pub missing: Vec<PathBuf>,
}

/// Reads an M3U or M3U8 playlist.
pub fn read(path: &Path) -> std::io::Result<Playlist> {
    let text = std::fs::read_to_string(path)?;
    let base = path.parent().unwrap_or(Path::new("."));
    let mut list = Playlist::default();
    for line in text.lines() {
        let line = line.trim();
        // `#EXTINF` and its companions are metadata: the source of truth is the
        // file, so they are skipped.
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // A URL is not a local file; those are not opened today.
        if line.contains("://") {
            continue;
        }
        let path = PathBuf::from(line);
        let path = if path.is_absolute() { path } else { base.join(path) };
        if path.is_file() {
            list.tracks.push(path);
        } else {
            list.missing.push(path);
        }
    }
    Ok(list)
}

/// Writes an M3U playlist with relative paths where it can.
///
/// Relative because a playlist next to the music can move with it; one full
/// of absolute paths breaks as soon as the disk changes.
pub fn write_m3u(path: &Path, tracks: &[PathBuf]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let base = path.parent().unwrap_or(Path::new("."));
    let mut f = std::fs::File::create(path)?;
    writeln!(f, "#EXTM3U")?;
    for track in tracks {
        let path = relative(base, track);
        writeln!(f, "{}", path.display())?;
    }
    Ok(())
}

/// `track`'s path relative to `base`, if they share a prefix.
fn relative(base: &Path, track: &Path) -> PathBuf {
    track.strip_prefix(base).map(PathBuf::from).unwrap_or_else(|_| track.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-playlist-{name}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn relative_paths_resolve_against_the_playlist() {
        let dir = folder("relativas");
        std::fs::write(dir.join("a.flac"), b"x").unwrap();
        std::fs::write(dir.join("b.flac"), b"x").unwrap();
        let list = dir.join("album.m3u");
        std::fs::write(&list, "#EXTM3U\n#EXTINF:120,Una\na.flac\nb.flac\n").unwrap();

        let read_back = read(&list).unwrap();
        assert_eq!(read_back.tracks, vec![dir.join("a.flac"), dir.join("b.flac")]);
        assert!(read_back.missing.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_is_gone_is_reported_separately() {
        let dir = folder("ausentes");
        std::fs::write(dir.join("a.flac"), b"x").unwrap();
        let list = dir.join("l.m3u");
        std::fs::write(&list, "a.flac\nse-fue.flac\n").unwrap();

        let read_back = read(&list).unwrap();
        assert_eq!(read_back.tracks.len(), 1);
        assert_eq!(read_back.missing, vec![dir.join("se-fue.flac")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn urls_are_skipped_for_now() {
        let dir = folder("urls");
        let list = dir.join("l.m3u");
        std::fs::write(&list, "http://ejemplo/stream\n").unwrap();
        let read_back = read(&list).unwrap();
        assert!(read_back.tracks.is_empty() && read_back.missing.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_is_written_reads_back_the_same_and_with_relative_paths() {
        let dir = folder("ida-y-vuelta");
        std::fs::create_dir_all(dir.join("disco")).unwrap();
        let tracks: Vec<PathBuf> =
            ["disco/01.flac", "disco/02.flac"].iter().map(|n| dir.join(n)).collect();
        for p in &tracks {
            std::fs::write(p, b"x").unwrap();
        }
        let list = dir.join("mia.m3u");
        write_m3u(&list, &tracks).unwrap();

        let text = std::fs::read_to_string(&list).unwrap();
        assert!(text.contains("disco/01.flac"), "se guardó relativa: {text}");
        assert!(!text.contains(dir.to_str().unwrap()), "sin rutas absolutas");

        assert_eq!(read(&list).unwrap().tracks, tracks);
        let _ = std::fs::remove_dir_all(dir);
    }
}
