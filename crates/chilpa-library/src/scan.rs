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
use crate::scan_sidecar::{overlaid, Homes, Seen};
use crate::sidecar::Home;
use rusqlite::params;
use std::collections::HashSet;
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
    /// Audio files found that could not be read: a link that points
    /// nowhere, a file that vanished between the listing and the read.
    pub unreadable: usize,
    /// Sidecars that are there and cannot be read. Their folders were
    /// scanned as if they had none.
    pub sidecars_unreadable: usize,
    /// True when the root could not be read; then nothing was touched.
    pub unreachable: bool,
}

/// A track that lives inside another file, with a path of its own and what
/// its container says about it.
pub struct ContainedTrack {
    pub path: String,
    pub info: chilpa_tags::FileInfo,
}

/// Files that hold several tracks -- a disc image -- read by whoever knows
/// how.
///
/// The library does not know what is inside a SACD image and may not depend
/// on what does (ADR-001). The core, which assembles both, hands the reader
/// in.
pub trait Containers {
    /// Whether a file may hold tracks, by its name. Nothing is opened.
    fn may_hold(&self, path: &Path) -> bool;

    /// The tracks a file holds, or `None` when it holds none: a DVD image
    /// beside the music is neither an error nor audio.
    fn tracks(&self, path: &Path) -> Option<Vec<ContainedTrack>>;
}

/// Every file is one track.
pub struct NoContainers;

impl Containers for NoContainers {
    fn may_hold(&self, _: &Path) -> bool {
        false
    }

    fn tracks(&self, _: &Path) -> Option<Vec<ContainedTrack>> {
        None
    }
}

const EXTENSIONS: &[&str] =
    &["flac", "wav", "aiff", "aif", "m4a", "mp4", "alac", "mp3", "ogg", "oga", "dsf", "dff"];

/// Scans a folder and its subfolders, leaving covers in `cache`.
pub fn scan_root_with_cache(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
) -> rusqlite::Result<ScanReport> {
    scan(lib, root, cache, &|_| {}, &NoContainers)
}

/// Scans a folder and its subfolders.
pub fn scan_root(lib: &Library, root: &Path) -> rusqlite::Result<ScanReport> {
    scan(lib, root, None, &|_| {}, &NoContainers)
}

/// The same, reporting how many files it has looked at.
pub fn scan_root_reporting(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
) -> rusqlite::Result<ScanReport> {
    scan(lib, root, cache, on_progress, &NoContainers)
}

/// The same, with files that hold several tracks read by `containers`.
pub fn scan_root_with(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
    containers: &dyn Containers,
) -> rusqlite::Result<ScanReport> {
    scan(lib, root, cache, on_progress, containers)
}

/// How many audio files live under a folder. It does not touch the database.
pub fn count_audio_files(root: &Path) -> usize {
    walk(root, &NoContainers).len()
}

/// The same, counting a file that may hold tracks as one.
pub fn count_files(root: &Path, containers: &dyn Containers) -> usize {
    walk(root, containers).len()
}

fn scan(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
    containers: &dyn Containers,
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
    let files = walk(root, containers);

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
    // What could be read. Only these are present at the end: a path that is
    // listed but cannot be opened is not a track anybody can play.
    let mut readable: HashSet<String> = HashSet::with_capacity(files.len());
    let mut homes = Homes::new();

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
        if containers.may_hold(file) {
            let found = Found { lib, source, root_id, now, cache };
            found.container(file, containers, &mut report, &mut readable, &mut homes)?;
            continue;
        }
        report.seen += 1;
        let Ok(meta) = std::fs::metadata(file) else {
            // A link that points nowhere, or a file gone since the listing.
            // It used to be skipped in silence and still counted as seen: a
            // folder with 20,298 links into a layout that no longer existed
            // reported 20,405 files and recorded 107.
            report.unreadable += 1;
            continue;
        };
        readable.insert(file.display().to_string());
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
        let home = Home::of(&path, None);
        let stale = homes.of(lib, &home, &path).stale;

        let known: Option<(i64, i64, Option<i64>)> = lib
            .db
            .query_row(
                "SELECT mtime, size, track_id FROM scanned_files WHERE path = ?1",
                params![path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();

        if let Some((m, s, track)) = &known {
            // A sidecar that changed makes its folder's files worth reading
            // again, though none of them did.
            if *m == mtime && *s == size && track.is_some() && !stale && evidence_read(lib, *track) {
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
            if let Some(moved) = moved_here(lib, root_id, mtime, size, inode(&meta))? {
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

        let track_id = save_file(lib, source, file, now, cache, &home, homes.of(lib, &home, &path))?;
        lib.db.execute(
            "INSERT INTO scanned_files (root_id, path, mtime, size, inode, track_id, scanned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET
                mtime = excluded.mtime, size = excluded.size,
                track_id = excluded.track_id, scanned_at = excluded.scanned_at",
            params![root_id, path, mtime, size, inode(&meta), track_id, now],
        )?;
        report.read += 1;
    }

    tx.commit()?;
    on_progress(files.len());

    // What was there and is not, or is there and cannot be read: marked
    // missing, never deleted.
    report.sidecars_unreadable = homes.record(lib, now)?;
    report.missing = mark_missing(lib, root_id, &readable)?;
    lib.assemble_local_releases()?;
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

/// What a scan needs to record a track, carried together.
struct Found<'a> {
    lib: &'a Library,
    source: i64,
    root_id: i64,
    now: i64,
    cache: Option<&'a Path>,
}

impl Found<'_> {
    /// A file that holds tracks: each on its own row, keyed to the file.
    ///
    /// What was read from it last time is kept, unopened, when the file has
    /// not changed: reading a disc image's tables is a few sectors that cost
    /// about 0.4 s over a network share, and the rescan that finds nothing
    /// changed is the one that runs at every startup.
    fn container(
        &self,
        file: &Path,
        containers: &dyn Containers,
        report: &mut ScanReport,
        readable: &mut HashSet<String>,
        homes: &mut Homes,
    ) -> rusqlite::Result<()> {
        let lib = self.lib;
        let Ok(meta) = std::fs::metadata(file) else {
            report.seen += 1;
            report.unreadable += 1;
            return Ok(());
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let size = meta.len() as i64;
        let container = file.display().to_string();
        let home = Home::of("", Some(&container));
        let stale = homes.of(lib, &home, &container).stale;

        let known: Vec<(String, i64, i64, Option<i64>)> = lib
            .db
            .prepare("SELECT path, mtime, size, track_id FROM scanned_files WHERE container = ?1")?
            .query_map(params![container], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        if !stale && !known.is_empty() && known.iter().all(|(_, m, s, t)| *m == mtime && *s == size && t.is_some() && evidence_read(lib, *t)) {
            for (path, _, _, track) in &known {
                lib.db.execute("UPDATE tracks SET present = 1 WHERE id = ?1", params![track])?;
                readable.insert(path.clone());
            }
            report.seen += known.len();
            report.unchanged += known.len();
            return Ok(());
        }

        let Some(tracks) = containers.tracks(file) else { return Ok(()) };
        let name = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        for track in &tracks {
            report.seen += 1;
            let seen = homes.of(lib, &home, &container);
            let info = overlaid(Some(track.info.clone()), seen, &home.key(&track.path));
            let track_id = save_track(
                lib,
                self.source,
                &track.path,
                info.as_ref(),
                &name,
                file,
                self.now,
                self.cache,
            )?;
            seen.cover_onto(lib, &home, track_id)?;
            // No inode: every track of a container shares the file's, and a
            // move is told by the inode, so they would be taken for each
            // other. A container that moves comes back as new tracks.
            lib.db.execute(
                "INSERT INTO scanned_files
                   (root_id, path, mtime, size, inode, track_id, scanned_at, container)
                 VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7)
                 ON CONFLICT(path) DO UPDATE SET
                    mtime = excluded.mtime, size = excluded.size,
                    track_id = excluded.track_id, scanned_at = excluded.scanned_at,
                    container = excluded.container",
                params![self.root_id, track.path, mtime, size, track_id, self.now, container],
            )?;
            readable.insert(track.path.clone());
            report.read += 1;
        }
        Ok(())
    }
}

/// A row of `scanned_files` whose file is not where it was.
struct Moved {
    row_id: i64,
    track_id: i64,
}

fn evidence_read(lib: &Library, track: Option<i64>) -> bool {
    lib.db.query_row("SELECT release_evidence_read FROM tracks WHERE id = ?1", [track], |r| r.get(0)).unwrap_or(false)
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
    home: &Home,
    seen: &Seen,
) -> rusqlite::Result<i64> {
    let external_id = path.display().to_string();
    let info = overlaid(chilpa_tags::read(path).ok(), seen, &home.key(&external_id));
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let track_id = save_track(lib, source, &external_id, info.as_ref(), &name, path, now, cache)?;
    seen.cover_onto(lib, home, track_id)?;
    Ok(track_id)
}

/// A track into the library: its row, its album and its genres, and the
/// album's cover out of `cover_from` when the album has none yet. `name` is
/// the title when the tags carry none.
#[allow(clippy::too_many_arguments)]
fn save_track(
    lib: &Library,
    source: i64,
    external_id: &str,
    info: Option<&chilpa_tags::FileInfo>,
    name: &str,
    cover_from: &Path,
    now: i64,
    cache: Option<&Path>,
) -> rusqlite::Result<i64> {
    let tags = info.map(|i| i.tags.clone()).unwrap_or_default();
    let title = tags.title.clone().unwrap_or_else(|| name.to_string());
    let artist = tags.artist_line();
    let album_id = match &tags.album {
        Some(album_title) => {
            // An empty tag names nobody: it is no album artist, and keying a
            // record by it credited the record to nobody and joined every
            // record of that title that had one.
            let named = tags.album_artist.as_deref().map(str::trim).filter(|a| !a.is_empty());
            let location = crate::store::releases::location(external_id);
            // I retain a known track's album until the assembly transaction
            // can move its marks and preferences along with the whole part.
            let existing: Option<i64> = lib.db.query_row(
                "SELECT album_id FROM tracks WHERE source_id = ?1 AND external_id = ?2",
                params![source, external_id], |r| r.get(0)).ok().flatten();
            let id = match existing {
                Some(id) => id,
                None => lib.local_edition(source, album_title, named.unwrap_or(&artist), &location, tags.year())?,
            };
            // The album's cover comes from the first file that carries one.
            if let Some(cache) = cache {
                save_cover(lib, id, cover_from, cache)?;
            }
            Some(id)
        }
        None => None,
    };
    let duration = info.map(|i| i.duration.as_millis() as i64).unwrap_or(0);

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
            external_id,
            title,
            artist,
            album_id,
            tags.track_number,
            tags.disc_number,
            duration,
            info.and_then(|i| i.sample_rate_hz),
            info.and_then(|i| i.bit_depth),
            info.and_then(|i| i.channels),
            tags.isrc,
            tags.musicbrainz_recording_id,
            now
        ],
    )?;
    let track_id: i64 = lib.db.query_row(
        "SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2",
        params![source, external_id],
        |r| r.get(0),
    )?;
    lib.db.execute("UPDATE tracks SET album_credit = ?2, album_title = ?3, disc_total = ?4,
        release_part = ?5, release_year = ?6, release_artwork =
        coalesce(release_artwork, (SELECT artwork_path FROM albums WHERE id = ?7)), release_evidence_read = 1 WHERE id = ?1",
        params![track_id, tags.album_artist.as_deref().unwrap_or("").trim(),
            tags.album.as_deref().unwrap_or(""), tags.disc_total,
            crate::store::releases::location(external_id), tags.year(), album_id])?;
    // Genres go in their own table: a file declares several, and joining
    // them into one string makes none of them usable for filtering.
    lib.set_track_genres(track_id, &tags.genres)?;
    Ok(track_id)
}


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
    let cover = chilpa_tags::artwork_for(path, cache);
    if let Some(cover) = cover {
        lib.db.execute(
            "UPDATE albums SET artwork_path = ?2 WHERE id = ?1",
            params![album, cover.display().to_string()],
        )?;
    }
    Ok(())
}



/// Marks the root's files that did not turn up as missing.
fn mark_missing(lib: &Library, root_id: i64, seen: &HashSet<String>) -> rusqlite::Result<usize> {
    let mut stmt = lib
        .db
        .prepare("SELECT path, track_id FROM scanned_files WHERE root_id = ?1")?;
    let rows: Vec<(String, Option<i64>)> = stmt
        .query_map(params![root_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut missing = 0;
    for (path, track) in rows {
        // A set and not a list: every row was compared against every file,
        // which on a rescan of a large library is its size squared.
        if seen.contains(&path) {
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
fn walk(root: &Path, containers: &dyn Containers) -> Vec<PathBuf> {
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
            } else if is_audio(&path) || containers.may_hold(&path) {
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
fn inode(meta: &std::fs::Metadata) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.ino() as i64)
}

#[cfg(not(target_family = "unix"))]
fn inode(_: &std::fs::Metadata) -> Option<i64> {
    None
}

fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
