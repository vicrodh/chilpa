//! Read a file's tags, without reading more into them than they say.
//!
//! Two rules that come from the QBZ audit, where the tag editor lost
//! data:
//!
//! 1. **Multi-value fields are kept.** A record with three genres has three
//!    genres, not one with commas in it. QBZ collapsed them on save.
//! 2. **The date is kept whole.** If the file says `2019-03-15`, that is what
//!    there is; QBZ truncated it to the year even when nothing had changed.
//!
//! This crate only reads. Writing tags is another story and arrives with the
//! editor.

use chilpa_models::{ArtworkRef, Track, TrackRef};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::Accessor;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod dsf;

#[derive(Debug)]
pub enum TagError {
    Open(std::io::Error),
    Unreadable(String),
}

impl std::fmt::Display for TagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagError::Open(e) => {
                write!(f, "{}", chilpa_i18n::t_args("could not open: {}", &[&e.to_string()]))
            }
            TagError::Unreadable(s) => {
                write!(f, "{}", chilpa_i18n::t_args("the tags could not be read: {}", &[s]))
            }
        }
    }
}

/// What a file says about itself.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Tags {
    pub title: Option<String>,
    /// Every artist it declares, in order.
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    /// Every genre, without joining them into one string.
    pub genres: Vec<String>,
    /// The date as it comes: it may be `2019` or `2019-03-15`.
    pub date: Option<String>,
    pub track_number: Option<u16>,
    pub disc_number: Option<u16>,
    pub isrc: Option<String>,
    pub musicbrainz_recording_id: Option<String>,
    /// How many covers it carries inside.
    pub pictures: usize,
}

impl Tags {
    /// The year, when the date carries one. Does not touch the stored date.
    pub fn year(&self) -> Option<u32> {
        self.date.as_ref()?.get(..4)?.parse().ok()
    }

    /// The artists on one line, for views that show a single one.
    pub fn artist_line(&self) -> String {
        self.artists.join(", ")
    }
}

/// What is known about a file: its tags and how long it runs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FileInfo {
    pub tags: Tags,
    pub duration: Duration,
    pub sample_rate_hz: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
}

/// A file opened for its tags: read by lofty, or by this crate for the
/// formats lofty does not know.
pub(crate) struct Opened {
    pub(crate) tag: Option<lofty::tag::Tag>,
    pub(crate) duration: Duration,
    pub(crate) sample_rate_hz: Option<u32>,
    pub(crate) bit_depth: Option<u8>,
    pub(crate) channels: Option<u8>,
}

fn open(path: &Path) -> Result<Opened, TagError> {
    if dsf::is_dsf(path) {
        return dsf::open(path);
    }
    let mut file = Probe::open(path)
        .map_err(|e| TagError::Unreadable(e.to_string()))?
        .read()
        .map_err(|e| TagError::Unreadable(e.to_string()))?;
    let properties = file.properties();
    let (duration, sample_rate_hz, bit_depth, channels) = (
        properties.duration(),
        properties.sample_rate(),
        properties.bit_depth(),
        properties.channels(),
    );
    // Taken out of the file rather than borrowed from it, so nothing -- a
    // cover of several megabytes, say -- is copied to be looked at.
    let primary = file.primary_tag_type();
    let tag = file.remove(primary).or_else(|| {
        let first = file.first_tag()?.tag_type();
        file.remove(first)
    });
    Ok(Opened { tag, duration, sample_rate_hz, bit_depth, channels })
}

/// Reads a file's tags and properties.
pub fn read(path: &Path) -> Result<FileInfo, TagError> {
    let opened = open(path)?;
    let mut info = FileInfo {
        tags: Tags::default(),
        duration: opened.duration,
        sample_rate_hz: opened.sample_rate_hz,
        bit_depth: opened.bit_depth,
        channels: opened.channels,
    };

    let Some(tag) = opened.tag.as_ref() else {
        return Ok(info);
    };

    let several = |key: ItemKey| -> Vec<String> {
        tag.get_strings(key).map(str::to_string).collect()
    };

    info.tags = Tags {
        title: tag.title().map(|t| t.to_string()),
        artists: {
            let all = several(ItemKey::TrackArtist);
            if all.is_empty() {
                tag.artist().map(|a| vec![a.to_string()]).unwrap_or_default()
            } else {
                all
            }
        },
        album: tag.album().map(|a| a.to_string()),
        album_artist: tag.get_string(ItemKey::AlbumArtist).map(str::to_string),
        genres: {
            let all = several(ItemKey::Genre);
            if all.is_empty() {
                tag.genre().map(|g| vec![g.to_string()]).unwrap_or_default()
            } else {
                all
            }
        },
        date: tag
            .get_string(ItemKey::RecordingDate)
            .or_else(|| tag.get_string(ItemKey::ReleaseDate))
            .map(str::to_string)
            .or_else(|| tag.get_string(ItemKey::Year).map(str::to_string)),
        track_number: tag.track().map(|n| n as u16),
        disc_number: tag.disk().map(|n| n as u16),
        isrc: tag.get_string(ItemKey::Isrc).map(str::to_string),
        musicbrainz_recording_id: tag
            .get_string(ItemKey::MusicBrainzRecordingId)
            .map(str::to_string),
        pictures: tag.picture_count() as usize,
    };
    Ok(info)
}

/// Writes the embedded cover out to a file, so the interface can show it.
///
/// Written once per source file: the name comes from the path and its
/// modification time, so changing the tags produces a different name and the
/// view is never left holding the old cover.
pub fn extract_artwork(path: &Path, cache_dir: &Path) -> Result<Option<PathBuf>, TagError> {
    let opened = open(path)?;
    let Some(tag) = opened.tag.as_ref() else {
        return Ok(None);
    };
    let Some(cover) = tag.pictures().first() else {
        return Ok(None);
    };
    let extension = match cover.mime_type().map(|m| m.as_str()) {
        Some("image/png") => "png",
        _ => "jpg",
    };
    let label = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("{:x}-{label}.{extension}", path_hash(path));
    std::fs::create_dir_all(cache_dir).map_err(TagError::Open)?;
    let target = cache_dir.join(name);
    if !target.exists() {
        std::fs::write(&target, cover.data()).map_err(TagError::Open)?;
    }
    Ok(Some(target))
}

/// The names a cover goes by when it sits beside the music.
const COVERS: &[&str] = &[
    "cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.jpeg", "folder.png",
    "front.jpg", "front.png", "album.jpg", "album.png", "Cover.jpg", "Folder.jpg",
];

/// The cover sitting next to the file, if there is one.
pub fn cover_beside(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    for name in COVERS {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // Any loose image, when there is none with a known name.
    let mut loose: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| matches!(e.to_lowercase().as_str(), "jpg" | "jpeg" | "png"))
                    .unwrap_or(false)
        })
        .collect();
    loose.sort();
    loose.into_iter().next()
}

/// The cover for a track: the one inside the file, or the one beside it.
///
/// Both, because a library has both kinds and somebody looking at the window
/// does not care which. The scan knew this rule and the player did not, so an
/// album whose cover is a `cover.jpg` in its folder -- which is most of
/// them -- showed its cover in the grid and "no cover" while it played.
pub fn artwork_for(path: &Path, cache_dir: &Path) -> Option<PathBuf> {
    extract_artwork(path, cache_dir).ok().flatten().or_else(|| cover_beside(path))
}

/// A short, stable hash of the path, to name the cache file.
fn path_hash(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
}

/// Builds a local-library [`Track`] from a file.
pub fn track_from_file(path: &Path, id: TrackRef) -> Result<Track, TagError> {
    let info = read(path)?;
    let title = info.tags.title.clone().unwrap_or_else(|| {
        path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    });
    let mut track = Track::new(id, title);
    track.duration = info.duration;
    track.track_number = info.tags.track_number;
    track.isrc = info.tags.isrc.clone();
    track.recording_mbid = info.tags.musicbrainz_recording_id.clone();
    track.artists = info
        .tags
        .artists
        .iter()
        .map(|name| chilpa_models::ArtistRef::named(name))
        .collect();
    track.album = info
        .tags
        .album
        .as_ref()
        .map(|title| chilpa_models::AlbumRef::titled(title));
    track.artwork = if info.tags.pictures > 0 {
        ArtworkRef::Embedded { path: path.to_path_buf(), index: 0 }
    } else {
        ArtworkRef::None
    };
    Ok(track)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_cover_beside_the_file_counts_as_the_track_s_cover() {
        // The scan knew this and the player did not: an album whose cover is
        // a cover.jpg in its folder, which is most of them, showed its cover
        // in the grid and "no cover" while it played.
        let dir = std::env::temp_dir().join("chilpa-tags-cover");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let track = dir.join("01.flac");
        std::fs::write(&track, b"not really a flac").unwrap();
        let cache = dir.join("cache");

        assert_eq!(artwork_for(&track, &cache), None, "nothing to find yet");

        // A name nobody uses, to prove the loose-image fallback works.
        let loose = dir.join("scan0001.png");
        std::fs::write(&loose, b"not really a png").unwrap();
        assert_eq!(artwork_for(&track, &cache), Some(loose), "any image beside it");

        // And a known name wins over a loose one.
        let named = dir.join("cover.jpg");
        std::fs::write(&named, b"not really a jpeg").unwrap();
        assert_eq!(artwork_for(&track, &cache), Some(named));

        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn the_year_comes_from_the_date_without_truncating_it() {
        let tags = Tags { date: Some("2019-03-15".into()), ..Tags::default() };
        assert_eq!(tags.year(), Some(2019));
        assert_eq!(tags.date.as_deref(), Some("2019-03-15"), "the date is kept whole");
    }

    #[test]
    fn several_artists_show_on_one_line_without_getting_lost() {
        let tags = Tags { artists: vec!["One".into(), "Two".into()], ..Tags::default() };
        assert_eq!(tags.artist_line(), "One, Two");
        assert_eq!(tags.artists.len(), 2, "they are still two, not one string");
    }

    #[test]
    fn the_cover_name_changes_when_the_file_changes() {
        // Two different paths must never share a cache file.
        assert_ne!(path_hash(Path::new("/a.flac")), path_hash(Path::new("/b.flac")));
        assert_eq!(path_hash(Path::new("/a.flac")), path_hash(Path::new("/a.flac")));
    }

    #[test]
    fn a_file_that_does_not_exist_is_reported() {
        assert!(read(Path::new("/does/not/exist.flac")).is_err());
    }
}
