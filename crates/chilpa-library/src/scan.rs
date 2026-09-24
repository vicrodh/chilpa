//! Scanning a folder into the library.
//!
//! Two rules that come from the QBZ audit:
//!
//! 1. **What did not change is not touched.** A file whose time and size are
//!    the same is neither reread nor has its row rewritten. In QBZ a rescan
//!    with no changes did three passes and rewrote every row.
//! 2. **A root that cannot be read deletes nothing.** If the folder is not
//!    there or cannot be opened, it is marked unreachable and we leave. In
//!    QBZ an unmounted mountpoint looked empty and pruned all its tracks.

use crate::store::Library;
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// What happened in a scan.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScanReport {
    pub root: PathBuf,
    /// Audio files seen.
    pub seen: usize,
    /// The ones read because they are new or changed.
    pub read: usize,
    /// The ones skipped because nothing changed.
    pub unchanged: usize,
    /// The ones that are gone and were marked missing.
    pub missing: usize,
    /// True when the root could not be read; then nothing was touched.
    pub unreachable: bool,
}

const EXTENSIONS: &[&str] =
    &["flac", "wav", "aiff", "aif", "m4a", "mp4", "alac", "mp3", "ogg", "oga", "dsf"];

/// Scans a folder and its subfolders, leaving covers in `cache`.
pub fn scan_root_with_cache(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
) -> rusqlite::Result<ScanReport> {
    scan(lib, root, cache, &|_| {})
}

/// Scans a folder and its subfolders.
pub fn scan_root(lib: &Library, root: &Path) -> rusqlite::Result<ScanReport> {
    scan(lib, root, None, &|_| {})
}

/// The same, reporting how many files it has looked at.
pub fn scan_root_reporting(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
) -> rusqlite::Result<ScanReport> {
    scan(lib, root, cache, on_progress)
}

/// How many audio files live under a folder. It does not touch the database.
pub fn count_audio_files(root: &Path) -> usize {
    walk(root).len()
}

fn scan(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
) -> rusqlite::Result<ScanReport> {
    let mut report = ScanReport { root: root.to_path_buf(), ..ScanReport::default() };
    let source = lib.source_id("local", "default")?;
    let root_id = lib.root_id(source, root)?;

    if !root.is_dir() {
        lib.set_root_reachable(root_id, false)?;
        report.unreachable = true;
        return Ok(report);
    }
    lib.set_root_reachable(root_id, true)?;

    let now = now_unix();
    let files = walk(root);
    report.seen = files.len();

    for (examined, file) in files.iter().enumerate() {
        // Reported every 25 files: one by one floods the channel without the
        // interface being able to draw any faster.
        if examined % 25 == 0 {
            on_progress(examined);
        }
        let Ok(meta) = std::fs::metadata(file) else { continue };
        // In nanoseconds, not seconds: two different versions of a file
        // written within the same second would look identical, and the scan
        // would skip the change.
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let size = meta.len() as i64;
        let path = file.display().to_string();

        let known: Option<(i64, i64, Option<i64>)> = lib
            .db
            .query_row(
                "SELECT mtime, size, track_id FROM scanned_files WHERE path = ?1",
                params![path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();

        if let Some((m, s, track)) = &known {
            if *m == mtime && *s == size && track.is_some() {
                // Same as last time: neither read nor rewritten.
                report.unchanged += 1;
                lib.db.execute(
                    "UPDATE tracks SET present = 1 WHERE id = ?1",
                    params![track.unwrap()],
                )?;
                continue;
            }
        }

        let track_id = save_file(lib, source, file, now, cache)?;
        lib.db.execute(
            "INSERT INTO scanned_files (root_id, path, mtime, size, inode, track_id, scanned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET
                mtime = excluded.mtime, size = excluded.size,
                track_id = excluded.track_id, scanned_at = excluded.scanned_at",
            params![root_id, path, mtime, size, inodo(&meta), track_id, now],
        )?;
        report.read += 1;
    }

    on_progress(files.len());

    // What was there and is not: marked missing, never deleted.
    let seen: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    report.missing = mark_missing(lib, root_id, &seen)?;
    lib.db
        .execute("UPDATE roots SET last_scan = ?2 WHERE id = ?1", params![root_id, now])?;
    Ok(report)
}

fn save_file(
    lib: &Library,
    source: i64,
    path: &Path,
    now: i64,
    cache: Option<&Path>,
) -> rusqlite::Result<i64> {
    let info = chilpa_tags::read(path).ok();
    let tags = info.as_ref().map(|i| i.tags.clone()).unwrap_or_default();
    let title = tags.title.clone().unwrap_or_else(|| {
        path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    });
    let artist = tags.artist_line();
    let album_id = match &tags.album {
        Some(album_title) => {
            let album_artist = tags.album_artist.clone().unwrap_or_else(|| artist.clone());
            let id = album_id(lib, album_title, &album_artist, tags.year())?;
            // The album's cover comes from the first file that carries one.
            if let Some(cache) = cache {
                save_cover(lib, id, path, cache)?;
            }
            Some(id)
        }
        None => None,
    };
    let duration = info.as_ref().map(|i| i.duration.as_millis() as i64).unwrap_or(0);

    lib.db.execute(
        "INSERT INTO tracks
           (source_id, external_id, title, artist, album_id, track_number, disc_number,
            duration_ms, sample_rate_hz, bit_depth, channels, isrc, recording_mbid,
            present, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14)
         ON CONFLICT(source_id, external_id) DO UPDATE SET
           title = excluded.title, artist = excluded.artist, album_id = excluded.album_id,
           track_number = excluded.track_number, disc_number = excluded.disc_number,
           duration_ms = excluded.duration_ms, sample_rate_hz = excluded.sample_rate_hz,
           bit_depth = excluded.bit_depth, channels = excluded.channels,
           isrc = excluded.isrc, recording_mbid = excluded.recording_mbid, present = 1",
        params![
            source,
            path.display().to_string(),
            title,
            artist,
            album_id,
            tags.track_number,
            tags.disc_number,
            duration,
            info.as_ref().and_then(|i| i.sample_rate_hz),
            info.as_ref().and_then(|i| i.bit_depth),
            info.as_ref().and_then(|i| i.channels),
            tags.isrc,
            tags.musicbrainz_recording_id,
            now
        ],
    )?;
    let track_id: i64 = lib.db.query_row(
        "SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2",
        params![source, path.display().to_string()],
        |r| r.get(0),
    )?;
    // Genres go in their own table: a file declares several, and joining
    // them into one string makes none of them usable for filtering.
    lib.set_track_genres(track_id, &tags.genres)?;
    Ok(track_id)
}

/// Cover file names that turn up in music folders.
const COVERS: &[&str] = &[
    "cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.jpeg", "folder.png",
    "front.jpg", "front.png", "album.jpg", "album.png", "Cover.jpg", "Folder.jpg",
];

/// Gives the album a cover if it does not have one yet.
///
/// The file's own first; failing that, the one next to it in the folder,
/// which is how half the libraries in the world keep their covers.
fn save_cover(lib: &Library, album: i64, path: &Path, cache: &Path) -> rusqlite::Result<()> {
    let has: Option<String> = lib
        .db
        .query_row("SELECT artwork_path FROM albums WHERE id = ?1", params![album], |r| r.get(0))
        .unwrap_or(None);
    if has.is_some() {
        return Ok(());
    }
    let cover = chilpa_tags::extract_artwork(path, cache)
        .ok()
        .flatten()
        .or_else(|| cover_in_folder(path));
    if let Some(cover) = cover {
        lib.db.execute(
            "UPDATE albums SET artwork_path = ?2 WHERE id = ?1",
            params![album, cover.display().to_string()],
        )?;
    }
    Ok(())
}

/// The cover sitting next to the file, if there is one.
fn cover_in_folder(path: &Path) -> Option<PathBuf> {
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

fn album_id(lib: &Library, title: &str, artist: &str, año: Option<u32>) -> rusqlite::Result<i64> {
    lib.db.execute(
        "INSERT OR IGNORE INTO albums (title, album_artist, year) VALUES (?1, ?2, ?3)",
        params![title, artist, año],
    )?;
    lib.db.query_row(
        "SELECT id FROM albums WHERE title = ?1 AND album_artist = ?2",
        params![title, artist],
        |r| r.get(0),
    )
}

/// Marks the root's files that did not turn up as missing.
fn mark_missing(lib: &Library, root_id: i64, seen: &[String]) -> rusqlite::Result<usize> {
    let mut stmt = lib
        .db
        .prepare("SELECT path, track_id FROM scanned_files WHERE root_id = ?1")?;
    let rows: Vec<(String, Option<i64>)> = stmt
        .query_map(params![root_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut missing = 0;
    for (path, track) in rows {
        if seen.iter().any(|v| v == &path) {
            continue;
        }
        if let Some(id) = track {
            lib.db.execute("UPDATE tracks SET present = 0 WHERE id = ?1", params![id])?;
        }
        lib.db.execute("DELETE FROM scanned_files WHERE path = ?1", params![path])?;
        missing += 1;
    }
    Ok(missing)
}

/// Every audio file under a folder.
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut output = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            // An unreadable subfolder neither cancels the scan nor disables the
            // prune: it is skipped and that is all. In QBZ a lost+found disabled
            // it forever.
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if is_audio(&path) {
                output.push(path);
            }
        }
    }
    output.sort();
    output
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(target_family = "unix")]
fn inodo(meta: &std::fs::Metadata) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.ino() as i64)
}

#[cfg(not(target_family = "unix"))]
fn inodo(_: &std::fs::Metadata) -> Option<i64> {
    None
}

fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
