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
use rusqlite::{params, OptionalExtension};
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
    /// I stopped before reconciliation; completed batches remain available.
    pub interrupted: bool,
    /// Availability, paths or derived records changed without rereading tags.
    pub changed: bool,
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

    /// A side file's change marker, folded into the container's scan stamp.
    fn evidence_stamp(&self, _: &Path) -> u64 { 0 }
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

use crate::scan_walk::{walk, walk_controlled, Listing};

/// What a scan says while it runs. Each does nothing unless a watcher asks.
pub trait ScanWatch {
    /// How many audio files the walk has found so far, every few hundred.
    fn found(&self, _so_far: usize) {}
    /// The walk finished and `files` are about to be read.
    fn listed(&self, _files: usize) {}
    /// How many of them have been looked at, every 25.
    fn advanced(&self, _done: usize) {}
    /// The records of the first `done` files are written and can be shown.
    /// `written` is how many rows reading has changed so far: it stays at
    /// zero while every file is as it was, so a rescan that finds nothing
    /// asks no screen to read the library again.
    fn committed(&self, _done: usize, _written: u64) {}
    /// Reading ended and what is gone is being reconciled.
    fn finalizing(&self) {}
}

/// The two callbacks `scan_controlled` takes, as a watcher.
struct Callbacks<'a> {
    progress: &'a dyn Fn(usize),
    finalizing: &'a dyn Fn(),
}

impl ScanWatch for Callbacks<'_> {
    fn advanced(&self, done: usize) { (self.progress)(done) }
    fn finalizing(&self) { (self.finalizing)() }
}

/// Scans a folder and its subfolders, leaving covers in `cache`.
pub fn scan_root_with_cache(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
) -> rusqlite::Result<ScanReport> {
    scan_root_with(lib, root, cache, &|_| {}, &NoContainers)
}

/// Scans a folder and its subfolders.
pub fn scan_root(lib: &Library, root: &Path) -> rusqlite::Result<ScanReport> {
    scan_root_with(lib, root, None, &|_| {}, &NoContainers)
}

/// The same, reporting how many files it has looked at.
pub fn scan_root_reporting(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
) -> rusqlite::Result<ScanReport> {
    scan_root_with(lib, root, cache, on_progress, &NoContainers)
}

/// The same, with files that hold several tracks read by `containers`.
pub fn scan_root_with(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
    containers: &dyn Containers,
) -> rusqlite::Result<ScanReport> {
    scan_controlled(lib, root, cache, on_progress, containers, &ScanControl::default(), &|| {})
}

/// How many audio files live under a folder. It does not touch the database.
pub fn count_audio_files(root: &Path) -> usize {
    walk(root, &NoContainers).files.len()
}

/// The same, counting a file that may hold tracks as one.
pub fn count_files(root: &Path, containers: &dyn Containers) -> usize {
    walk(root, containers).files.len()
}

/// Walks the root and scans what it finds.
pub fn scan_controlled(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
    containers: &dyn Containers,
    control: &ScanControl,
    on_finalizing: &dyn Fn(),
) -> rusqlite::Result<ScanReport> {
    scan_watched(lib, root, cache, containers, control, &Callbacks { progress: on_progress, finalizing: on_finalizing })
}

/// How many files go into the first batch. It is small so the first
/// records reach the screens about as soon as reading starts, instead of
/// after a full batch; `ScanWatch::committed` says when they have.
const FIRST_BATCH: usize = 25;

/// Walks the root and scans what it finds, telling `watch` as it goes.
/// When `control` stops it, nothing more is read and the scan reports
/// itself interrupted.
///
/// The root is asked whether it answers before the walk, and the walk is
/// read straight after it. A listing is never taken ahead and kept: one
/// taken while a share was away is empty, and read once the share was back
/// it took every track under it for deleted.
pub fn scan_watched(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    containers: &dyn Containers,
    control: &ScanControl,
    watch: &dyn ScanWatch,
) -> rusqlite::Result<ScanReport> {
    let mut report = ScanReport { root: root.to_path_buf(), ..ScanReport::default() };
    if control.stopped() { report.interrupted = true; return Ok(report); }
    let source = lib.source_id("local", "default")?;
    let root_id = lib.root_id(source, root)?;

    if !crate::store::reach::root_answers(lib, root_id, root)? {
        report.changed |= lib.set_root_reachable(root_id, false)?;
        report.unreachable = true;
        return Ok(report);
    }
    report.changed |= lib.set_root_reachable(root_id, true)?;
    let now = now_unix();
    let Listing { files, blocked } = walk_controlled(root, containers, control, &|found| watch.found(found));
    if control.stopped() { report.interrupted = true; return Ok(report); }
    watch.listed(files.len());
    // The tracks whose record was named by their folder this time.
    let mut from_place: Vec<i64> = Vec::new();

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
    // The rows reading writes. A file found as it was writes none.
    let unwritten = lib.db.total_changes();
    let written = || lib.db.total_changes() - unwritten;
    let mut tx = lib.write_transaction()?;
    // What could be read. Only these are present at the end: a path that is
    // listed but cannot be opened is not a track anybody can play.
    let mut readable: HashSet<String> = HashSet::with_capacity(files.len());
    let mut homes = Homes::new();

    for (examined, file) in files.iter().enumerate() {
        if control.stopped() {
            tx.commit()?;
            report.interrupted = true;
            return Ok(report);
        }
        // Reported every 25 files: one by one floods the channel without the
        // interface being able to draw any faster.
        if examined % 25 == 0 {
            watch.advanced(examined);
        }
        if examined == FIRST_BATCH || (examined > 0 && examined % BATCH == 0) {
            tx.commit()?;
            watch.committed(examined, written());
            tx = lib.write_transaction()?;
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

        // Every statement of this loop is prepared once and kept: a
        // statement on `tracks` compiles its triggers as it is prepared,
        // which was 0.45 ms of the 1.4 ms an insert took, and the same
        // again for a no-op update on a rescan.
        let known: Option<(i64, i64, Option<i64>)> = lib
            .db
            .prepare_cached("SELECT mtime, size, track_id FROM scanned_files WHERE path = ?1")
            .and_then(|mut stmt| stmt.query_row(params![path], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))))
            .ok();

        if let Some((m, s, track)) = &known {
            report.changed |= lib.db.prepare_cached(
                "UPDATE tracks SET present = 1, reachable = 1 WHERE id = ?1 AND (present <> 1 OR reachable <> 1)",
            )?.execute(params![track])? > 0;
            // A sidecar that changed makes its folder's files worth reading
            // again, though none of them did.
            // A track that came in without a record is read again, when its
            // folders can name one now; a loose file at the root cannot.
            let settled = has_album(lib, *track) || crate::from_path::from_path(file, Some(root)).album.is_none();
            if *m == mtime && *s == size && track.is_some() && !stale && evidence_read(lib, *track) && settled {
                // Same as last time: neither read nor rewritten.
                report.unchanged += 1;
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
                    "UPDATE tracks SET external_id = ?2, present = 1, reachable = 1 WHERE id = ?1",
                    params![moved.track_id, path],
                )?;
                lib.db.execute(
                    "UPDATE scanned_files SET path = ?2, scanned_at = ?3 WHERE id = ?1",
                    params![moved.row_id, path, now],
                )?;
                report.unchanged += 1;
                report.changed = true;
                continue;
            }
        }

        let (track_id, guessed) = save_file(lib, source, file, root, now, cache, &home, homes.of(lib, &home, &path))?;
        if guessed {
            from_place.push(track_id);
        }
        lib.db.prepare_cached(
            "INSERT INTO scanned_files (root_id, path, mtime, size, inode, track_id, scanned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET
                mtime = excluded.mtime, size = excluded.size,
                track_id = excluded.track_id, scanned_at = excluded.scanned_at",
        )?.execute(params![root_id, path, mtime, size, inode(&meta), track_id, now])?;
        report.read += 1;
    }

    tx.commit()?;
    watch.advanced(files.len());
    if control.stopped() { report.interrupted = true; return Ok(report); }
    // Asked again before anything is taken for gone: a share that went away
    // while it was read leaves the rest of its files unreadable, and an
    // unreadable file is reconciled as missing.
    if !crate::store::reach::root_answers(lib, root_id, root)? {
        report.changed |= lib.set_root_reachable(root_id, false)?;
        report.unreachable = true;
        return Ok(report);
    }
    watch.finalizing();

    // What was there and is not, or is there and cannot be read: marked
    // missing, never deleted.
    report.sidecars_unreadable = homes.record(lib, now)?;
    let before_reconcile = lib.db.total_changes();
    report.missing = mark_missing(lib, root_id, &readable, &blocked)?;
    join_tagged_neighbours(lib, &from_place)?;
    lib.assemble_local_releases()?;
    // Once the parts are one record, its discs can be compared.
    own_disc_covers(lib, root, cache)?;
    // What came in can be the artist a joined credit was waiting for.
    lib.match_credits()?;
    report.changed |= lib.db.total_changes() != before_reconcile;
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
        let size = (meta.len() as i64) ^ (containers.evidence_stamp(file) as i64);
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
                report.changed |= lib.db.execute("UPDATE tracks SET present = 1, reachable = 1 WHERE id = ?1 AND (present <> 1 OR reachable <> 1)", params![track])? > 0;
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
            let (track_id, _) = save_track(
                lib,
                self.source,
                &track.path,
                info.as_ref(),
                &name,
                file,
                None,
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

/// Each disc of a record shows its own cover, found as QBZ's scan finds a
/// track's (`decorate_tracks`): from the narrowest place outwards, the
/// picture inside the disc's first file, then its own folder's cover
/// ([`chilpa_tags::cover_in`], which does not look up to the record's
/// folder), then its record's.
///
/// A track's cover is kept as its record's was when the track was read,
/// and a record's is its first disc's. So the discs that hold their record's
/// cover, or none, are looked at again, every time: their files did not
/// change, so no scan reads them as tracks. A disc that holds another cover
/// already found its own and is left alone. Before this, a cover taken out
/// of a file was left alone wherever it came from, and a box whose first
/// disc carries its cover inside its files showed that one on all thirteen.
///
/// Then the discs of one record are compared. A set that copies one cover
/// into every disc's folder has one cover, not several, and its discs are
/// given the same one, so nothing shows it once per disc. Two files of the
/// same size are taken for the same picture; nothing is opened.
fn own_disc_covers(lib: &Library, root: &Path, cache: Option<&Path>) -> rusqlite::Result<()> {
    let under = format!("{}/%", root.display().to_string().trim_end_matches('/'));
    // Only a record of several parts can hold another part's cover.
    let parts: Vec<(String, Option<String>, i64, String, Option<String>)> = {
        let mut stmt = lib.db.prepare(
            "SELECT t.release_part, min(t.release_artwork), t.album_id, min(t.external_id),
                    (SELECT artwork_path FROM albums WHERE id = t.album_id)
             FROM tracks t JOIN sources s ON s.id = t.source_id
             WHERE s.provider = 'local' AND t.present = 1 AND t.release_part LIKE ?1 AND t.album_id IN
               (SELECT album_id FROM tracks WHERE present = 1 GROUP BY album_id HAVING count(DISTINCT release_part) > 1)
             GROUP BY t.album_id, t.release_part ORDER BY t.album_id, t.release_part",
        )?;
        let rows = stmt.query_map(params![under], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    // The covers a record's parts have so far.
    let mut records: std::collections::BTreeMap<i64, Vec<(u64, String)>> = Default::default();
    for (part, kept, album, first, record) in parts {
        let folder = Path::new(&part);
        // A disc image is its own part; its folder is the image's.
        let folder = if folder.is_file() { folder.parent().unwrap_or(folder) } else { folder };
        let found = kept.is_some() && kept != record;
        let own = if found {
            kept.clone()
        } else {
            cache
                .and_then(|cache| chilpa_tags::extract_artwork(Path::new(&first), cache).ok().flatten())
                .or_else(|| chilpa_tags::cover_in(folder))
                .map(|own| own.display().to_string())
                .or_else(|| kept.clone())
        };
        let Some(own) = own else { continue };
        let Ok(size) = std::fs::metadata(&own).map(|meta| meta.len()) else { continue };
        let siblings = records.entry(album).or_default();
        let cover = match siblings.iter().find(|(other, _)| *other == size) {
            Some((_, same)) => same.clone(),
            None => {
                siblings.push((size, own.clone()));
                own
            }
        };
        if kept.as_deref() != Some(cover.as_str()) {
            lib.db.execute(
                "UPDATE tracks SET release_artwork = ?2 WHERE release_part = ?1 AND release_artwork IS NOT ?2",
                params![part, cover],
            )?;
        }
    }
    Ok(())
}

/// The date when it is whole, "1981-04-26" out of "1981-04-26T00:00:00Z";
/// nothing when it holds only a year or does not read as a date.
fn whole_date(date: Option<&str>) -> Option<&str> {
    let head = date?.trim().get(..10)?;
    let b = head.as_bytes();
    let digits = |range: std::ops::Range<usize>| b[range].iter().all(u8::is_ascii_digit);
    if !(digits(0..4) && b[4] == b'-' && digits(5..7) && b[7] == b'-' && digits(8..10)) {
        return None;
    }
    let month: u8 = head[5..7].parse().ok()?;
    let day: u8 = head[8..10].parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some(head)
}

fn evidence_read(lib: &Library, track: Option<i64>) -> bool {
    lib.db.prepare_cached("SELECT release_evidence_read FROM tracks WHERE id = ?1")
        .and_then(|mut stmt| stmt.query_row([track], |r| r.get(0))).unwrap_or(false)
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

#[allow(clippy::too_many_arguments)]
fn save_file(
    lib: &Library,
    source: i64,
    path: &Path,
    root: &Path,
    now: i64,
    cache: Option<&Path>,
    home: &Home,
    seen: &Seen,
) -> rusqlite::Result<(i64, bool)> {
    let external_id = path.display().to_string();
    let info = overlaid(chilpa_tags::read(path).ok(), seen, &home.key(&external_id));
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let saved = save_track(lib, source, &external_id, info.as_ref(), &name, path, Some(root), now, cache)?;
    seen.cover_onto(lib, home, saved.0)?;
    Ok(saved)
}

/// A track into the library: its row, its album and its genres, and the
/// album's cover out of `cover_from` when the album has none yet. `name` is
/// the title when the tags carry none.
///
/// With `root`, a file whose tags leave out its record joins the one its
/// tagged neighbours in the folder share, or failing that takes the one its
/// folders name (`from_path`), and its number and title from its name: a
/// track with no record is on no record's screen. The second value says the
/// record was named by the folder.
#[allow(clippy::too_many_arguments)]
fn save_track(
    lib: &Library,
    source: i64,
    external_id: &str,
    info: Option<&chilpa_tags::FileInfo>,
    name: &str,
    cover_from: &Path,
    root: Option<&Path>,
    now: i64,
    cache: Option<&Path>,
) -> rusqlite::Result<(i64, bool)> {
    let mut tags = info.map(|i| i.tags.clone()).unwrap_or_default();
    let mut joined = None;
    let mut from_folder = false;
    if let Some(root) = root {
        let place = crate::from_path::from_path(Path::new(external_id), Some(root));
        if tags.album.is_none() {
            joined = neighbours_album(lib, source, external_id, None)?;
            match &joined {
                Some((_, title, artist)) => {
                    tags.album = Some(title.clone());
                    tags.album_artist = tags.album_artist.take().or_else(|| artist.clone());
                }
                None => {
                    from_folder = place.album.is_some();
                    tags.album = place.album.clone();
                    tags.album_artist = tags.album_artist.take().or_else(|| place.album_artist.clone());
                }
            }
        }
        // A year left blank in the tags is the one the folder says.
        if tags.year().is_none() {
            tags.date = place.year.map(|year| year.to_string()).or(tags.date.take());
        }
        tags.title = tags.title.take().or_else(|| place.title.clone());
        tags.track_number = tags.track_number.or(place.track);
        tags.disc_number = tags.disc_number.or(place.disc);
        if tags.artists.is_empty() {
            tags.artists = place.artist.clone().or_else(|| tags.album_artist.clone()).into_iter().collect();
        }
    }
    let title = tags.title.clone().unwrap_or_else(|| name.to_string());
    let artist = tags.artist_line();
    // A file with a field for each artist says who they are.
    if tags.artists.len() > 1 {
        lib.state_credit(&artist, &tags.artists)?;
    }
    let album_id = match &tags.album {
        Some(album_title) => {
            // An empty tag names nobody: it is no album artist, and keying a
            // record by it credited the record to nobody and joined every
            // record of that title that had one.
            let named = tags.album_artist.as_deref().map(str::trim).filter(|a| !a.is_empty());
            let location = crate::store::releases::location(external_id);
            // I retain a known track's album until the assembly transaction
            // can move its marks and preferences along with the whole part.
            let existing: Option<i64> = lib.db
                .prepare_cached("SELECT album_id FROM tracks WHERE source_id = ?1 AND external_id = ?2")
                .and_then(|mut stmt| stmt.query_row(params![source, external_id], |r| r.get(0))).ok().flatten();
            let id = match (existing, &joined) {
                (Some(id), _) => id,
                (None, Some((id, _, _))) => *id,
                (None, None) => lib.local_edition(source, album_title, named.unwrap_or(&artist), &location, tags.year())?,
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

    // The row and what was read about its release go in together. They
    // went in as an insert and then an update, and the update named
    // `album_credit`, which made the members trigger delete and write
    // again what the insert's trigger had just written: on a library being
    // filled that was 0.8 ms of the 1 ms the update took, per file.
    lib.db.prepare_cached(
        "INSERT INTO tracks
           (source_id, external_id, title, artist, album_id, track_number, disc_number,
            duration_ms, sample_rate_hz, bit_depth, channels, isrc, recording_mbid,
            present, added_at, label, explicit,
            album_credit, album_title, disc_total, release_part, release_year, release_artwork,
            release_date, disc_subtitle, work, release_evidence_read)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14, ?15, ?16,
                 ?17, ?18, ?19, ?20, ?21, (SELECT artwork_path FROM albums WHERE id = ?5),
                 ?22, ?23, ?24, 1)
         ON CONFLICT(source_id, external_id) DO UPDATE SET
           title = excluded.title, artist = excluded.artist, album_id = excluded.album_id,
           track_number = excluded.track_number, disc_number = excluded.disc_number,
           duration_ms = excluded.duration_ms, sample_rate_hz = excluded.sample_rate_hz,
           bit_depth = excluded.bit_depth, channels = excluded.channels,
           isrc = excluded.isrc, recording_mbid = excluded.recording_mbid, present = 1,
           label = excluded.label, explicit = excluded.explicit,
           album_credit = excluded.album_credit, album_title = excluded.album_title,
           disc_total = excluded.disc_total, release_part = excluded.release_part,
           release_year = excluded.release_year,
           release_artwork = coalesce(tracks.release_artwork, excluded.release_artwork),
           release_date = excluded.release_date, disc_subtitle = excluded.disc_subtitle,
           work = excluded.work, release_evidence_read = 1",
    )?.execute(
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
            now,
            tags.label,
            tags.explicit,
            tags.album_artist.as_deref().unwrap_or("").trim(),
            tags.album.as_deref().unwrap_or(""),
            tags.disc_total,
            crate::store::releases::location(external_id),
            tags.year(),
            whole_date(tags.date.as_deref()),
            tags.disc_subtitle,
            tags.work
        ],
    )?;
    let track_id: i64 = lib.db.prepare_cached(
        "SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2",
    )?.query_row(params![source, external_id], |r| r.get(0))?;
    // Genres go in their own table: a file declares several, and joining
    // them into one string makes none of them usable for filtering.
    lib.set_track_genres(track_id, &tags.genres)?;
    lib.set_track_composers(track_id, &tags.composers)?;
    Ok((track_id, from_folder))
}

/// The record the other files of `external_id`'s folder share, when they
/// all share one -- leaving out `except` -- with its title and album
/// artist. A folder of loose tracks from several records names none.
fn neighbours_album(lib: &Library, source: i64, external_id: &str, except: Option<i64>) -> rusqlite::Result<Option<(i64, String, Option<String>)>> {
    let Some(dir) = Path::new(external_id).parent().map(|d| format!("{}/", d.display())) else { return Ok(None) };
    let mut stmt = lib.db.prepare_cached(
        "SELECT DISTINCT t.album_id, a.title, a.album_artist FROM tracks t JOIN albums a ON a.id = t.album_id
          WHERE t.source_id = ?1 AND t.external_id != ?2 AND t.album_id IS NOT ?4
            AND substr(t.external_id, 1, length(?3)) = ?3
            AND instr(substr(t.external_id, length(?3) + 1), '/') = 0
          LIMIT 2",
    )?;
    let found: Vec<(i64, String, Option<String>)> = stmt
        .query_map(params![source, external_id, dir, except], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(if found.len() == 1 { found.into_iter().next() } else { None })
}

/// Tracks whose folder named their record, moved to the record their tagged
/// neighbours share when the neighbours came later in the same scan: files
/// are read in name order, and "05" can come before the "06" that carries
/// the tags. The record the folder named is left empty and goes with the
/// releases' clean-up.
fn join_tagged_neighbours(lib: &Library, guessed: &[i64]) -> rusqlite::Result<()> {
    for track in guessed {
        let row: Option<(String, i64, Option<i64>)> = lib
            .db
            .query_row("SELECT external_id, source_id, album_id FROM tracks WHERE id = ?1", params![track], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?;
        let Some((path, source, own)) = row else { continue };
        let Some((album, title, artist)) = neighbours_album(lib, source, &path, own)? else { continue };
        lib.db.execute(
            "UPDATE tracks SET album_id = ?2, album_title = ?3, album_credit = ?4 WHERE id = ?1",
            params![track, album, title, artist.unwrap_or_default()],
        )?;
    }
    Ok(())
}

/// Whether a track has a record: one that came in without is read again.
fn has_album(lib: &Library, track: Option<i64>) -> bool {
    lib.db.prepare_cached("SELECT album_id IS NOT NULL FROM tracks WHERE id = ?1")
        .and_then(|mut stmt| stmt.query_row([track], |r| r.get(0))).unwrap_or(true)
}


/// Gives the album a cover if it does not have one yet.
///
/// The file's own first; failing that, the one next to it in the folder,
/// which is how half the libraries in the world keep their covers; and for
/// a disc of a set that has none, the one its set keeps in the folder above.
fn save_cover(lib: &Library, album: i64, path: &Path, cache: &Path) -> rusqlite::Result<()> {
    let has: Option<String> = lib
        .db
        .prepare_cached("SELECT artwork_path FROM albums WHERE id = ?1")
        .and_then(|mut stmt| stmt.query_row(params![album], |r| r.get(0)))
        .unwrap_or(None);
    if has.is_some() {
        return Ok(());
    }
    let cover = chilpa_tags::artwork_for(path, cache)
        .or_else(|| chilpa_tags::cover_beside(&Path::new(&crate::album_folder::album_folder(path)).join("_")));
    if let Some(cover) = cover {
        lib.db.execute(
            "UPDATE albums SET artwork_path = ?2 WHERE id = ?1",
            params![album, cover.display().to_string()],
        )?;
    }
    Ok(())
}



/// Marks the root's files that did not turn up as missing.
///
/// Not the ones under a subfolder that could not be opened: whether they
/// are still there is unknown, so they stay listed and cannot be played,
/// like a folder that does not answer (ADR-017 §2). A subfolder that was
/// never readable, a `lost+found`, holds none and changes nothing.
fn mark_missing(lib: &Library, root_id: i64, seen: &HashSet<String>, blocked: &[PathBuf]) -> rusqlite::Result<usize> {
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
        if blocked.iter().any(|dir| Path::new(&path).starts_with(dir)) {
            if let Some(id) = track {
                lib.db.execute("UPDATE tracks SET reachable = 0 WHERE id = ?1 AND reachable <> 0", params![id])?;
            }
            continue;
        }
        if let Some(id) = track {
            lib.db.execute("UPDATE tracks SET present = 0 WHERE id = ?1 AND present <> 0", params![id])?;
        }
        lib.db.execute("DELETE FROM scanned_files WHERE path = ?1", params![path])?;
        missing += 1;
    }
    Ok(missing)
}

/// Every audio file under a folder, and the subfolders that could not be
/// opened.
/// I cancel between filesystem operations and complete records, never in a
/// transaction statement. A blocked filesystem read must return first.
#[derive(Clone, Default)]
pub struct ScanControl(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl ScanControl {
    pub fn stop(&self) { self.0.store(true, std::sync::atomic::Ordering::Relaxed); }
    pub fn stopped(&self) -> bool { self.0.load(std::sync::atomic::Ordering::Relaxed) }
}

pub fn count_controlled(root: &Path, containers: &dyn Containers, control: &ScanControl) -> Option<usize> {
    let count = walk_controlled(root, containers, control, &|_| {}).files.len();
    (!control.stopped()).then_some(count)
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

#[cfg(test)]
mod tests {
    use super::whole_date;

    #[test]
    fn a_date_is_kept_only_when_it_is_whole() {
        assert_eq!(whole_date(Some("1981-04-26")), Some("1981-04-26"));
        assert_eq!(whole_date(Some(" 1981-04-26T00:00:00Z")), Some("1981-04-26"), "the time is not the date");
        assert_eq!(whole_date(Some("1981")), None, "a year is kept as the year");
        assert_eq!(whole_date(Some("1981-13-02")), None);
        assert_eq!(whole_date(Some("1981/04/26")), None);
        assert_eq!(whole_date(Some("19ö1-04-26")), None, "no panic on a letter that is two bytes");
        assert_eq!(whole_date(None), None);
    }
}
