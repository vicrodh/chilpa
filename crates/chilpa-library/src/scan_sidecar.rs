//! What a scan knows about sidecars.
//!
//! A folder's sidecar is asked about once per scan, not once per file: on a
//! network share each question is a round trip, and a record has a dozen
//! files and one sidecar.

use crate::sidecar::{in_force, load, Home, Sidecar, SidecarError};
use crate::store::Library;
use chilpa_tags::{FileInfo, Tags};
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

/// What a scan knows about one home.
pub(crate) struct Seen {
    pub sidecar: Option<Sidecar>,
    /// The sidecar is not what the last scan saw: new, changed or gone.
    pub stale: bool,
    /// There is one and it cannot be read.
    pub unreadable: bool,
    /// It could not be read this time -- the disk, the network -- which
    /// says nothing about what it holds. Nothing is written down about it,
    /// so the next scan asks again.
    failed: bool,
    /// Every name the home's sidecar may go by.
    names: Vec<PathBuf>,
    /// The one in force, with its date in nanoseconds and its size.
    file: Option<(PathBuf, i64, i64)>,
}

fn measure(path: &Path) -> Option<(i64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos() as i64;
    Some((mtime, meta.len() as i64))
}

fn look(lib: &Library, home: &Home, external_id: &str) -> Seen {
    let mut names = vec![home.sidecar()];
    names.extend(home.qbz_candidates(external_id));

    let file = in_force(home, external_id).and_then(|path| {
        let (mtime, size) = measure(&path)?;
        Some((path, mtime, size))
    });
    let mut recorded: Vec<(String, i64, i64)> = Vec::new();
    for name in &names {
        let row = lib
            .db
            .query_row("SELECT path, mtime, size FROM sidecars WHERE path = ?1", params![name.display().to_string()], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()
            .unwrap_or(None);
        recorded.extend(row);
    }
    let stale = match &file {
        Some((path, mtime, size)) => recorded.len() != 1 || recorded[0] != (path.display().to_string(), *mtime, *size),
        None => !recorded.is_empty(),
    };
    let (sidecar, unreadable, failed) = match load(home, external_id) {
        Ok(sidecar) => (sidecar, false, false),
        Err(SidecarError::Io(_)) => (None, true, true),
        Err(_) => (None, true, false),
    };
    // A sidecar that could not be read this time does not send its files to
    // be read again without it: what the library has from them stays.
    Seen { sidecar, stale: stale && !failed, unreadable, failed, names, file }
}

impl Seen {
    /// Gives a track's record the cover its sidecar names, over whatever
    /// cover the record had: it is what somebody chose.
    pub(crate) fn cover_onto(&self, lib: &Library, home: &Home, track_id: i64) -> rusqlite::Result<()> {
        let Some(name) = self.sidecar.as_ref().and_then(|s| s.album.cover.value()) else {
            return Ok(());
        };
        // A name and nothing else: a sidecar does not point outside its folder.
        if name.contains(['/', '\\']) {
            return Ok(());
        }
        let cover = home.beside(name);
        if !cover.is_file() {
            return Ok(());
        }
        lib.db.execute(
            "UPDATE albums SET artwork_path = ?2 WHERE id = (SELECT album_id FROM tracks WHERE id = ?1)",
            params![track_id, cover.display().to_string()],
        )?;
        lib.db.execute("UPDATE tracks SET release_artwork = ?2 WHERE id = ?1",
            params![track_id, cover.display().to_string()])?;
        Ok(())
    }

    fn record(&self, lib: &Library, now: i64) -> rusqlite::Result<()> {
        if self.failed {
            return Ok(());
        }
        for name in &self.names {
            lib.db.execute("DELETE FROM sidecars WHERE path = ?1", params![name.display().to_string()])?;
        }
        if let Some((path, mtime, size)) = &self.file {
            lib.db.execute(
                "INSERT INTO sidecars (path, mtime, size, seen_at) VALUES (?1, ?2, ?3, ?4)",
                params![path.display().to_string(), mtime, size, now],
            )?;
        }
        Ok(())
    }
}

/// What a file says with its sidecar laid over it.
pub(crate) fn overlaid(info: Option<FileInfo>, seen: &Seen, key: &str) -> Option<FileInfo> {
    let Some(sidecar) = &seen.sidecar else { return info };
    // A file whose tags cannot be read still takes what its sidecar says.
    let mut info = info.unwrap_or(FileInfo {
        tags: Tags::default(),
        duration: Duration::ZERO,
        sample_rate_hz: None,
        bit_depth: None,
        channels: None,
    });
    sidecar.overlay(&mut info.tags, key);
    Some(info)
}

/// The homes a scan has come across, each asked about once.
#[derive(Default)]
pub(crate) struct Homes {
    seen: HashMap<Home, Seen>,
}

impl Homes {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn of(&mut self, lib: &Library, home: &Home, external_id: &str) -> &Seen {
        self.seen.entry(home.clone()).or_insert_with(|| look(lib, home, external_id))
    }

    /// Writes down what was seen, and forgets the sidecars that are gone.
    /// Returns how many could not be read.
    pub(crate) fn record(&self, lib: &Library, now: i64) -> rusqlite::Result<usize> {
        let mut unreadable = 0;
        for seen in self.seen.values() {
            unreadable += usize::from(seen.unreadable);
            seen.record(lib, now)?;
        }
        Ok(unreadable)
    }
}

impl Library {
    /// Writes down a home's sidecar as it is on disk now.
    ///
    /// For whoever writes one outside a scan: without this the next scan
    /// takes the write for a change made behind its back, and rereads the
    /// folder to find what it already has.
    pub fn note_home(&self, home: &Home, external_id: &str, now: i64) -> rusqlite::Result<()> {
        look(self, home, external_id).record(self, now)
    }
}
