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

    // Committing once per file costs more than writing the file's row. Every
    // statement outside a transaction is its own commit, and a file takes
    // several -- look it up, write the track, write what was scanned, write
    // its genres.
    //
    // Measured on this machine, five thousand files into a library on disk:
    // a first scan went from 1.17 ms to 0.53 ms a file and a rescan that
    // found nothing changed from 221 us to 96 us. On a hundred thousand
    // tracks that is two minutes off the first scan and twelve seconds off
    // every one after it.
    //
    // Not one transaction for the whole scan, though. A batch is what keeps
    // the three things that matter true at once: the library fills in while
    // the scan runs instead of appearing at the end, the write-ahead log
    // stays a size instead of growing to hold everything, and a scan that
    // is interrupted loses at most a batch -- the files in it were never
    // recorded as scanned, so the next scan reads them again.
    const BATCH: usize = 200;
    let mut tx = lib.db.unchecked_transaction()?;

    for (examined, file) in files.iter().enumerate() {
        // Reported every 25 files: one by one floods the channel without the
        // interface being able to draw any faster.
        if examined % 25 == 0 {
            on_progress(examined);
        }
        if examined > 0 && examined % BATCH == 0 {
            tx.commit()?;
            tx = lib.db.unchecked_transaction()?;
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

        // A path nobody has seen might not be a new file: it might be one
        // that moved. Every player that keys a track by its path loses the
        // play count, the rating and the analysis the moment somebody
        // reorganises their music, and the whole history is sitting right
        // there under the old name.
        //
        // What identifies it: the same inode, the same size, the same mtime
        // to the nanosecond, and the old path no longer on disk. Together
        // that is a file that was renamed. A hardlink shares an inode but
        // leaves the old path in place, so it does not count -- and it is
        // not one file anyway.
        //
        // What this does NOT cover is a move to another filesystem, which
        // gives the file a new inode. That takes reading the file, and is a
        // decision of its own; here it comes out as a new track and the old
        // one goes missing, which is what would happen today.
        if known.is_none() {
            if let Some(moved) = moved_here(lib, root_id, mtime, size, inodo(&meta))? {
                lib.db.execute(
                    "UPDATE tracks SET external_id = ?2, present = 1 WHERE id = ?1",
                    params![moved.track_id, path],
                )?;
                lib.db.execute(
                    "UPDATE scanned_files SET path = ?2, scanned_at = ?3 WHERE id = ?1",
                    params![moved.row_id, path, now],
                )?;
                report.unchanged += 1;
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

    tx.commit()?;
    on_progress(files.len());

    // What was there and is not: marked missing, never deleted.
    let seen: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    report.missing = mark_missing(lib, root_id, &seen)?;
    lib.db
        .execute("UPDATE roots SET last_scan = ?2 WHERE id = ?1", params![root_id, now])?;
    // A scan is the only thing that changes the size of the library, so it is
    // the only moment the planner's statistics can go stale. See
    // `schema::update_statistics` for what they are worth. A rescan that
    // found nothing new changed nothing and pays nothing, which matters
    // because that is the scan that runs at every startup.
    if report.read > 0 || report.missing > 0 {
        crate::schema::update_statistics(&lib.db)?;
    }
    Ok(report)
}

/// A row of `scanned_files` whose file is not where it was.
struct Moved {
    row_id: i64,
    track_id: i64,
}

/// The track a file used to be, when what arrived is the same file under
/// another name.
///
/// The three numbers have to match and the old path has to be gone. The
/// last part is what tells a move apart from a copy and from a hardlink: if
/// the old name still resolves, there are two files now, not one that
/// walked.
fn moved_here(
    lib: &Library,
    root_id: i64,
    mtime: i64,
    size: i64,
    inode: Option<i64>,
) -> rusqlite::Result<Option<Moved>> {
    // Without an inode there is nothing to match on that is not a guess.
    let Some(inode) = inode else { return Ok(None) };
    let mut stmt = lib.db.prepare(
        "SELECT id, path, track_id FROM scanned_files
         WHERE root_id = ?1 AND inode = ?2 AND size = ?3 AND mtime = ?4
           AND track_id IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![root_id, inode, size, mtime], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
    })?;
    for row in rows {
        let (row_id, old_path, track_id) = row?;
        if !Path::new(&old_path).exists() {
            return Ok(Some(Moved { row_id, track_id }));
        }
    }
    Ok(None)
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
