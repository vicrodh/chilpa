//! The open database, and what gets asked of it.
//!
//! Each subject has its file: the album grid, the playlists, what the
//! analysis extracts, the play log, and the fingerprints used to match.
//! What stays here is the connection, sources and roots, genres and search.

pub mod albums;
pub mod artists;
pub mod browse;
pub mod credits;
pub mod edit;
pub mod favorites;
pub mod features;
mod genres;
mod integrations;
pub mod find;
pub mod fingerprints;
pub mod importing;
pub mod playlists;
pub mod plays;
pub mod progress;
pub mod progress_writer;
pub mod listening;
pub mod ratings;
pub mod external_ratings;
pub mod regroup;
pub mod saved;
mod related;
pub(crate) mod reach;
pub mod servers;
mod compilations;
pub(crate) mod releases;
pub mod versions;

pub use albums::{AlbumDisc, AlbumSort, StoredAlbum, StoredArtist};
pub use artists::{is_various, ArtistRole};
pub use edit::{AlbumEdit, EditTrack, Edited, Put, TrackEdit};
pub use browse::{letter_of, BrowseTrack, FacetEntry, Filter, FolderEntry, Snapshot, Totals, TrackOrder};
pub use find::Found;
pub use playlists::{PlaylistEdit, PlaylistEntry, PlaylistFolder, PlaylistMembership, PlaylistTrackMetadata, StoredPlaylist};
pub use plays::{Stats, TrackStat};
pub use servers::{NewServer, Removal, ServerTrack, StoredServer};

use crate::schema;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// A track as the library stores it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredTrack {
    pub id: i64,
    pub external_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u16>,
    pub duration_ms: u64,
    /// In the library and reachable right now: its folder is mounted, its
    /// server answers. What is not is still listed, and cannot be played.
    pub playable: bool,
}

pub use genres::split_genres;

/// A folder that is part of the library.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredRoot {
    pub id: i64,
    pub path: std::path::PathBuf,
    /// When it was last scanned, in unix seconds.
    pub last_scan: Option<i64>,
    pub reachable: bool,
}

/// The open library.
pub struct Library {
    pub(crate) db: Connection,
    versions: std::cell::RefCell<Option<versions::CachedVersions>>,
    related: std::cell::RefCell<Option<related::Profiles>>,
}

/// A transaction of one's own, or somebody else's.
///
/// `Deref` to a `Connection` either way, so the code that uses it does not
/// have to know which. `finish` commits when it is ours and does nothing
/// when it is not: whoever opened the outer one decides when it lands.
pub(crate) enum MaybeTransaction<'a> {
    Own(rusqlite::Transaction<'a>),
    Someone(&'a Connection),
}

impl MaybeTransaction<'_> {
    fn finish(self) -> rusqlite::Result<()> {
        match self {
            MaybeTransaction::Own(tx) => tx.commit(),
            MaybeTransaction::Someone(_) => Ok(()),
        }
    }
}

impl std::ops::Deref for MaybeTransaction<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        match self {
            MaybeTransaction::Own(tx) => tx,
            MaybeTransaction::Someone(db) => db,
        }
    }
}

impl Library {
    /// The connection, for tests that need to seed data in one go.
    ///
    /// It is not for the application: what the application needs comes out
    /// through a named method, not through loose SQL.
    #[doc(hidden)]
    pub fn connection(&self) -> &Connection {
        &self.db
    }

    /// Opens (or creates) the library in that file.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let db = Connection::open(path)?;
        schema::apply_pragmas(&db)?;
        // Before the shape of the file changes, and only when it is going to.
        schema::back_up_before_upgrading(&db, path)?;
        schema::migrate(&db)?;
        // Not only after a scan: a library that already exists carries the
        // statistics of an empty database and would keep them until its next
        // scan, which may be never.
        schema::update_statistics_if_stale(&db)?;
        let lib = Self { db, versions: Default::default(), related: Default::default() };
        // Genres an earlier scan stored joined are split once, here: files
        // that did not change are never read again to do it.
        lib.split_stored_genres()?;
        // And records an earlier scan split by artist, or credited to an
        // empty album artist tag, the same way.
        lib.assemble_local_releases()?;
        // And credits whose artists the library already has on their own.
        // Another connection writing right now is no reason not to open:
        // the next one that opens does it.
        if let Err(e) = lib.match_credits() {
            eprintln!("chilpa-library: credits were not matched this time: {e}");
        }
        Ok(lib)
    }

    /// Opens an existing, current library for queries only. The main
    /// connection owns migrations and maintenance; a reader must never
    /// upgrade a read transaction to a write while another writer runs.
    pub fn open_read_only(path: &Path) -> rusqlite::Result<Self> {
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != schema::VERSION {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISMATCH),
                Some(format!("library reader needs schema version {}, found {version}; open the main library first", schema::VERSION)),
            ));
        }
        Ok(Self { db, versions: Default::default(), related: Default::default() })
    }

    /// A background writer for an already initialized library, without migrations.
    pub fn open_existing(path: &Path) -> rusqlite::Result<Self> {
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        schema::apply_pragmas(&db)?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != schema::VERSION { return Err(rusqlite::Error::InvalidQuery); }
        Ok(Self { db, versions: Default::default(), related: Default::default() })
    }

    /// An in-memory library, for tests.
    ///
    /// It gets the same pragmas as one on disk. It did not, and the one that
    /// matters is `foreign_keys`: with it off, every test in this crate ran
    /// against a database that could not notice a row pointing at nothing,
    /// so a rescan that broke a foreign key passed here and failed on a real
    /// library. A fixture that cannot fail the way the real thing fails is
    /// not testing the real thing.
    pub fn in_memory() -> rusqlite::Result<Self> {
        let db = Connection::open_in_memory()?;
        schema::apply_pragmas(&db)?;
        schema::migrate(&db)?;
        Ok(Self { db, versions: Default::default(), related: Default::default() })
    }

    /// Lets the planner see the library it actually has.
    ///
    /// A scan does this on its own. It is public because a library filled by
    /// any other route -- a test fixture, an import -- needs it too, and
    /// because the numbers in `schema::update_statistics` are what the
    /// difference is worth.
    pub fn update_statistics(&self) -> rusqlite::Result<()> {
        schema::update_statistics(&self.db)
    }

    /// Where this user's library lives.
    pub fn default_path() -> PathBuf {
        chilpa_dirs::data().join("library.db")
    }

    /// The source's id, creating it if needed.
    pub fn source_id(&self, provider: &str, instance: &str) -> rusqlite::Result<i64> {
        // Asked on the window's thread at every track start. A source that
        // is already there is only read, and a read never waits: writing
        // first made this wait for whoever else was writing, a scan or a
        // sync, for as long as the lock allows.
        let known = self.db.query_row("SELECT id FROM sources WHERE provider = ?1 AND instance = ?2", params![provider, instance], |r| r.get(0)).optional()?;
        if let Some(id) = known {
            return Ok(id);
        }
        // What is not the local library and was not added as a server is a
        // service: browsed, never kept as mine (ADR-017).
        self.db.execute(
            "INSERT OR IGNORE INTO sources (provider, instance, kind)
             VALUES (?1, ?2, CASE WHEN ?1 = 'local' THEN 'local' ELSE 'service' END)",
            params![provider, instance],
        )?;
        self.db.query_row("SELECT id FROM sources WHERE provider = ?1 AND instance = ?2", params![provider, instance], |r| r.get(0))
    }

    /// Keeps the name a source wants the interface to show.
    pub fn set_source_label(&self, source_id: i64, label: &str) -> rusqlite::Result<()> {
        self.db.execute("UPDATE sources SET label = ?2 WHERE id = ?1", params![source_id, label])?;
        Ok(())
    }

    /// The root's id, creating it if needed.
    pub fn root_id(&self, source_id: i64, path: &Path) -> rusqlite::Result<i64> {
        let path = path.display().to_string();
        self.db.execute("INSERT OR IGNORE INTO roots (source_id, path) VALUES (?1, ?2)", params![source_id, path])?;
        self.db.query_row("SELECT id FROM roots WHERE path = ?1", params![path], |r| r.get(0))
    }

    /// A transaction that holds the write lock from its first statement.
    pub(crate) fn write_transaction(&self) -> rusqlite::Result<rusqlite::Transaction<'_>> {
        rusqlite::Transaction::new_unchecked(&self.db, rusqlite::TransactionBehavior::Immediate)
    }

    /// A transaction, unless the caller already opened one.
    ///
    /// SQLite has no nested `BEGIN`: asking for one inside another is an
    /// error, not a no-op. A method that wants its writes to land together
    /// needs one when it is called on its own and needs to stay out of the
    /// way when it is called from inside a bigger one.
    ///
    /// It takes the write lock as it begins. A deferred one that reads first
    /// and writes after is refused on the spot -- "database is locked", with
    /// no wait -- whenever another connection is writing or has written
    /// since the read: SQLite does not run the busy timeout for a read that
    /// turns into a write. Asking for the write up front waits its turn.
    pub(crate) fn own_transaction(&self) -> rusqlite::Result<MaybeTransaction<'_>> {
        if self.db.is_autocommit() {
            Ok(MaybeTransaction::Own(self.write_transaction()?))
        } else {
            Ok(MaybeTransaction::Someone(&self.db))
        }
    }

    /// Sets those genres as a track's, dropping the ones that no longer apply.
    ///
    /// They are replaced, not added to: if a file was retagged, the old genres
    /// are no longer its own.
    ///
    /// The transaction is its own only when there is not one already. SQLite
    /// has no nested `BEGIN`, and a scan wraps whole batches of files in one
    /// (`scan.rs`), so opening a second here is an error and not a nicety:
    /// inside somebody else's transaction, theirs is what makes this atomic.
    pub fn set_track_genres(&self, track_id: i64, genres: &[String]) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute("DELETE FROM track_genres WHERE track_id = ?1", params![track_id])?;
        {
            let mut insert = tx.prepare("INSERT OR IGNORE INTO genres (name) VALUES (?1)")?;
            let mut find = tx.prepare("SELECT id FROM genres WHERE name = ?1")?;
            let mut link = tx.prepare("INSERT OR IGNORE INTO track_genres (track_id, genre_id) VALUES (?1, ?2)")?;
            for name in split_genres(genres) {
                let name = name.as_str();
                insert.execute(params![name])?;
                let id: i64 = find.query_row(params![name], |r| r.get(0))?;
                link.execute(params![track_id, id])?;
            }
        }
        tx.finish()
    }

    /// Who wrote a track's music, each apart, replacing what it had: a
    /// value with several names in it is split at ";" and "/", never at a
    /// comma, which joins one person's names as often as it separates two
    /// people. Inside somebody else's transaction, theirs makes it atomic.
    pub fn set_track_composers(&self, track_id: i64, composers: &[String]) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute("DELETE FROM track_composers WHERE track_id = ?1", params![track_id])?;
        {
            let mut insert = tx.prepare("INSERT OR IGNORE INTO composers (name) VALUES (?1)")?;
            let mut find = tx.prepare("SELECT id FROM composers WHERE name = ?1")?;
            let mut link = tx.prepare("INSERT OR IGNORE INTO track_composers (track_id, composer_id) VALUES (?1, ?2)")?;
            for name in split_composers(composers) {
                insert.execute(params![name])?;
                let id: i64 = find.query_row(params![name], |r| r.get(0))?;
                link.execute(params![track_id, id])?;
            }
        }
        tx.finish()
    }

    /// Files again the genres stored as an earlier reading left them
    /// ("Rock; Blues", "Pop, Rock", a catalog's page pasted whole), one
    /// genre each as `split_genres` reads them now, and moves their tracks
    /// onto them. A rescan does not do it, since it only rereads files that
    /// changed, so it runs when the library opens and finds any. Returns how
    /// many were filed again.
    pub fn split_stored_genres(&self) -> rusqlite::Result<usize> {
        let stale: Vec<(i64, Vec<String>)> = {
            let mut stmt = self.db.prepare("SELECT id, name FROM genres")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            let mut stale = Vec::new();
            for row in rows {
                let (id, name) = row?;
                let parts = split_genres(std::slice::from_ref(&name));
                if parts.len() != 1 || parts[0] != name {
                    stale.push((id, parts));
                }
            }
            stale
        };
        if stale.is_empty() {
            return Ok(0);
        }
        let tx = self.own_transaction()?;
        {
            let mut insert = tx.prepare("INSERT OR IGNORE INTO genres (name) VALUES (?1)")?;
            let mut find = tx.prepare("SELECT id FROM genres WHERE name = ?1")?;
            let mut relink = tx.prepare(
                "INSERT OR IGNORE INTO track_genres (track_id, genre_id)
                 SELECT track_id, ?2 FROM track_genres WHERE genre_id = ?1",
            )?;
            for (id, parts) in &stale {
                let mut kept = false;
                for part in parts {
                    insert.execute(params![part])?;
                    let part_id: i64 = find.query_row(params![part], |r| r.get(0))?;
                    // Names compare without case: a part can be the row itself.
                    if part_id == *id {
                        tx.execute("UPDATE genres SET name = ?2 WHERE id = ?1", params![id, part])?;
                        kept = true;
                    } else {
                        relink.execute(params![id, part_id])?;
                    }
                }
                if !kept {
                    tx.execute("DELETE FROM track_genres WHERE genre_id = ?1", params![id])?;
                    tx.execute("DELETE FROM genres WHERE id = ?1", params![id])?;
                }
            }
        }
        tx.finish()?;
        Ok(stale.len())
    }

    /// The library's genres, with how many present tracks each has.
    pub fn genres(&self) -> rusqlite::Result<Vec<(String, usize)>> {
        let mut stmt = self.db.prepare(
            "SELECT g.name, count(t.id)
             FROM genres g
             JOIN track_genres tg ON tg.genre_id = g.id
             JOIN tracks t ON t.id = tg.track_id AND t.present = 1
             GROUP BY g.id
             ORDER BY count(t.id) DESC, g.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
        rows.collect()
    }

    /// A track's genres.
    pub fn track_genres(&self, track_id: i64) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.db.prepare(
            "SELECT g.name FROM genres g
             JOIN track_genres tg ON tg.genre_id = g.id
             WHERE tg.track_id = ?1
             ORDER BY g.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![track_id], |r| r.get(0))?;
        rows.collect()
    }

    /// The folders that have ever been scanned, with when that was.
    ///
    /// It is what is needed to rescan without the user having to remember
    /// where their music is.
    pub fn roots(&self) -> rusqlite::Result<Vec<StoredRoot>> {
        let mut stmt = self.db.prepare("SELECT id, path, last_scan, reachable FROM roots ORDER BY path COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| Ok(StoredRoot { id: r.get(0)?, path: r.get::<_, String>(1)?.into(), last_scan: r.get(2)?, reachable: r.get::<_, i64>(3)? != 0 }))?;
        rows.collect()
    }

    /// Forgets a folder: it stops being rescanned, and what came from it
    /// stops counting as present.
    ///
    /// The tracks are not deleted. The play log points at them -- that is
    /// where the stats come from -- and adding the folder back finds them
    /// again instead of starting the history from zero. What does go is what
    /// the scan remembered about the files, because forgetting a folder and
    /// keeping notes on it is not forgetting it.
    ///
    /// Gone, not unreachable: a folder I took out of the library is not
    /// waiting to come back, so its records are not listed (ADR-017).
    pub fn forget_root(&self, root_id: i64) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE tracks SET present = 0 WHERE present = 1 AND id IN
             (SELECT track_id FROM scanned_files WHERE root_id = ?1 AND track_id IS NOT NULL)",
            params![root_id],
        )?;
        self.db.execute("DELETE FROM scanned_files WHERE root_id = ?1", params![root_id])?;
        self.db.execute("DELETE FROM roots WHERE id = ?1", params![root_id])?;
        Ok(())
    }

    /// Marks whether a root is reachable, and its tracks with it. A missing
    /// root prunes NOTHING and hides nothing: its records stay listed and
    /// cannot be played until it is back (ADR-017).
    ///
    /// Returns whether anything changed, so a watcher that asks every
    /// minute only tells the window when there is something to redraw.
    pub fn set_root_reachable(&self, root_id: i64, reachable: bool) -> rusqlite::Result<bool> {
        let changed = self.db.execute(
            "UPDATE roots SET reachable = ?2 WHERE id = ?1 AND reachable <> ?2",
            params![root_id, i64::from(reachable)],
        )?;
        // Only when the folder itself came or went: while it stays, which of
        // its tracks can be read is the scan's to say (a subfolder it could
        // not open, `scan::mark_missing`).
        if changed == 0 {
            return Ok(false);
        }
        let moved = self.db.execute(
            "UPDATE tracks SET reachable = ?2 WHERE reachable <> ?2 AND id IN
             (SELECT track_id FROM scanned_files WHERE root_id = ?1 AND track_id IS NOT NULL)",
            params![root_id, i64::from(reachable)],
        )?;
        Ok(changed + moved > 0)
    }

    /// How many tracks there are, counting the missing ones or not.
    pub fn track_count(&self, include_absent: bool) -> rusqlite::Result<usize> {
        let sql = if include_absent { "SELECT count(*) FROM tracks" } else { "SELECT count(*) FROM tracks WHERE present = 1" };
        let n: i64 = self.db.query_row(sql, [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Searches by title, artist or album.
    pub fn search(&self, text: &str, limit: usize) -> rusqlite::Result<Vec<StoredTrack>> {
        let Some(query) = fts_query(text) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, (t.present = 1 AND t.reachable = 1)
             FROM (SELECT rowid FROM track_search WHERE track_search MATCH ?1 LIMIT ?3) s
             JOIN tracks t ON t.id = s.rowid
             LEFT JOIN albums a ON a.id = t.album_id
             WHERE t.present = 1
             ORDER BY t.artist COLLATE NOCASE, t.title COLLATE NOCASE
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit as i64, CONSIDERED as i64], row_to_track)?;
        rows.collect()
    }

    /// What is wrong with the library, if anything.
    ///
    /// Two questions SQLite can answer and nobody ever asks it until the
    /// day something is already broken: is the file itself intact, and do
    /// the rows still point at rows that exist.
    ///
    /// It is not run on open. `integrity_check` reads the whole file, which
    /// on a large library is seconds, and paying that every launch to catch
    /// something that almost never happens is the wrong trade. It is here so
    /// that when something does look wrong there is an answer better than a
    /// guess.
    ///
    /// An empty list means nothing was found.
    pub fn check(&self) -> rusqlite::Result<Vec<String>> {
        let mut found = Vec::new();
        let mut stmt = self.db.prepare("PRAGMA integrity_check")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        for row in rows {
            let row = row?;
            // It says "ok" when there is nothing to say.
            if row != "ok" {
                found.push(row);
            }
        }
        let mut stmt = self.db.prepare("PRAGMA foreign_key_check")?;
        let rows = stmt.query_map([], |r| Ok(format!("{}: a row points at a {} that is not there", r.get::<_, String>(0)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            found.push(row?);
        }
        Ok(found)
    }

    /// Rebuilds everything that is derived, from what it is derived from.
    ///
    /// The rollups and the search index exist because recomputing them per
    /// view was too slow. That makes them a copy, and a copy can be wrong.
    /// This is the way back: it throws the copies away and makes them again
    /// from the tracks and the log, which are the only things that are not
    /// derived from anything.
    ///
    /// It does not repair a corrupt file -- nothing in here can -- but it is
    /// the answer to a library whose numbers stopped adding up.
    pub fn repair(&self) -> rusqlite::Result<()> {
        self.rebuild_rollups()?;
        self.rebuild_search()
    }

    /// Builds the search index again from the tracks.
    pub fn rebuild_search(&self) -> rusqlite::Result<()> {
        self.db.execute_batch(
            "BEGIN;
             DELETE FROM track_search;
             INSERT INTO track_search (rowid, title, artist, album, genres)
             SELECT t.id,
                    t.title,
                    t.artist,
                    COALESCE(a.title, ''),
                    COALESCE((SELECT group_concat(g.name, ' ') FROM track_genres tg
                              JOIN genres g ON g.id = tg.genre_id
                              WHERE tg.track_id = t.id), '')
             FROM tracks t LEFT JOIN albums a ON a.id = t.album_id;
             COMMIT;",
        )
    }

    /// Recomputes what is kept, from what it is kept from.
    ///
    /// Everything the grid, the artists view and the stats read fast is a
    /// number the triggers maintain. **A number that is kept can drift**, and
    /// one that drifts is worse than one that is slow: it is wrong without
    /// saying so. Three things guard against that, and this is the third.
    ///
    /// The first is that the triggers cover every way a row gets in. The
    /// second is that the tests compare kept against counted. The third is
    /// this: a repair anyone can run, and what the tests compare against.
    ///
    /// It is one transaction, so a library is never left half rebuilt.
    pub fn rebuild_rollups(&self) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute_batch(
            "DELETE FROM album_rollup;
             INSERT INTO album_rollup (album_id, plays, last_played, added_at, present_tracks)
             SELECT a.id,
                    COALESCE(e.plays, 0),
                    e.last_at,
                    COALESCE((SELECT max(t.added_at) FROM tracks t WHERE t.album_id = a.id), 0),
                    (SELECT count(*) FROM tracks t
                     WHERE t.album_id = a.id AND t.present = 1)
             FROM albums a
             LEFT JOIN (
                 SELECT t2.album_id AS aid, count(*) AS plays, max(p.started_at) AS last_at
                 FROM play_events p
                 JOIN tracks t2 ON t2.id = p.track_id
                 WHERE p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms
                 GROUP BY t2.album_id
             ) e ON e.aid = a.id;

             DELETE FROM track_rollup;
             INSERT INTO track_rollup (track_id, plays, last_played, last_touched)
             SELECT p.track_id,
                    sum(CASE WHEN p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms
                             THEN 1 ELSE 0 END),
                    max(CASE WHEN p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms
                             THEN p.started_at END),
                    max(p.started_at)
             FROM play_events p
             WHERE p.track_id IS NOT NULL
             GROUP BY p.track_id;

             UPDATE library_totals
                SET plays = COALESCE((SELECT sum(CASE WHEN duration_ms = 0
                                                        OR played_ms * 2 >= duration_ms
                                                      THEN 1 ELSE 0 END) FROM play_events), 0),
                    listened_ms = COALESCE((SELECT sum(played_ms) FROM play_events), 0),
                    tracks = (SELECT count(DISTINCT track_id) FROM play_events),
                    skips = COALESCE((SELECT sum(skipped) FROM play_events), 0),
                    bit_perfect = COALESCE((SELECT sum(bit_perfect) FROM play_events), 0),
                    since = (SELECT min(started_at) FROM play_events)
              WHERE id = 1;",
        )?;
        tx.finish()
    }

    /// The track at that path, if it is in the library.
    pub fn track_by_path(&self, source_id: i64, path: &Path) -> rusqlite::Result<Option<i64>> {
        self.db.query_row("SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2", params![source_id, path.display().to_string()], |r| r.get(0)).optional()
    }
}

/// How many matches are looked at before sorting them.
///
/// The index finds them fast; putting them in order is what costs, and the
/// order asked for -- by artist, then title -- is not the index's. At two
/// hundred thousand tracks a one-letter search matched everything and spent
/// 143 ms sorting a library to show two hundred rows of it, on the thread
/// that draws the window.
///
/// So there is a ceiling. Below it nothing changes: a real search matches
/// hundreds, they all get sorted, the answer is exactly what it was. Above
/// it the answer is a bounded slice of the matches rather than the first
/// page of all of them -- and a query matching five thousand tracks is not
/// a search, it is a browse, and nobody reads to the end of it.
///
/// What this is not is a fix for the shape of the problem. If someone ever
/// wants the true first page of a hundred-thousand-match query, that needs
/// the ordering to come from the index, which means keeping a sorted key,
/// which is a different piece of work.
const CONSIDERED: usize = 5_000;

/// What someone typed, as something FTS5 will accept.
///
/// Every token is quoted and given a `*`, which is prefix matching: typing
/// "suz" finds "Suzie" before the word is finished, which is what a search
/// box is for. Quoting is what makes the rest safe -- FTS5 has an operator
/// language, and `AND`, `NOT`, `*`, `^`, `:` and `"` in a title would
/// otherwise either change the query or fail it.
///
/// `None` when there is nothing to search for: whitespace, or punctuation
/// that tokenizes to nothing. An empty FTS5 query is a syntax error, and
/// answering "nothing found" is the truth anyway.
pub(crate) fn fts_query(text: &str) -> Option<String> {
    let tokens: Vec<String> = text.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).map(|t| format!("\"{}\"*", t.replace('"', "\"\""))).collect();
    (!tokens.is_empty()).then(|| tokens.join(" "))
}

pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub(crate) fn row_to_track(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredTrack> {
    Ok(StoredTrack { id: r.get(0)?, external_id: r.get(1)?, title: r.get(2)?, artist: r.get(3)?, album: r.get(4)?, track_number: r.get::<_, Option<i64>>(5)?.map(|n| n as u16), duration_ms: r.get::<_, i64>(6)? as u64, playable: r.get::<_, i64>(7)? == 1 })
}

/// What every subject's tests seed, in one place: if each file built its
/// own sample library, a schema change would have to be fixed five times
/// over instead of once.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Three albums with different artist, year, date added and plays.
    pub(crate) fn sample_library() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        // (id, title, artist, year, added_at)
        for (id, title, artist, year, high) in [(1, "Bayou", "Creedence", 1969, 100), (2, "Aqualung", "Tull", 1971, 300), (3, "Cosmo", "Creedence", 1970, 200)] {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist, year)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, title, artist, year],
                )
                .unwrap();
            lib.db
                .execute(
                    "INSERT INTO tracks (source_id, external_id, title, album_id, duration_ms, added_at)
                     VALUES (?1, ?2, 'Track', ?3, 1000, ?4)",
                    params![source, format!("/{title}.flac"), id, high],
                )
                .unwrap();
        }
        // Number 3 plays twice, and last of all; number 1, once.
        lib.log_play(Some(1), Some(source), 10, 1000, 1000, false, None, None, true).unwrap();
        lib.log_play(Some(3), Some(source), 20, 1000, 1000, false, None, None, true).unwrap();
        lib.log_play(Some(3), Some(source), 30, 1000, 1000, false, None, None, true).unwrap();
        lib
    }

    pub(crate) fn titles(lib: &Library, order: AlbumSort) -> Vec<String> {
        lib.albums_sorted(order, 50, 0).unwrap().into_iter().map(|(a, _)| a.title).collect()
    }
}

#[cfg(test)]
mod genre_tests {
    use super::*;

    #[test]
    fn genres_stored_joined_are_split_once_with_their_tracks() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO tracks (id, source_id, external_id, title, added_at) VALUES (1, ?1, '/a.flac', 'A', 1)", params![source]).unwrap();
        lib.db.execute("INSERT INTO genres (id, name) VALUES (9, 'Rock; Blues')", []).unwrap();
        lib.db.execute("INSERT INTO track_genres (track_id, genre_id) VALUES (1, 9)", []).unwrap();
        assert_eq!(lib.split_stored_genres().unwrap(), 1);
        assert_eq!(lib.track_genres(1).unwrap(), ["Blues", "Rock"]);
        assert_eq!(lib.split_stored_genres().unwrap(), 0, "once");
    }

    #[test]
    fn genres_stored_as_a_tagger_wrote_them_are_filed_again() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for id in 1..=4 {
            lib.db.execute("INSERT INTO tracks (id, source_id, external_id, title, added_at) VALUES (?1, ?2, ?3, 'A', 1)", params![id, source, format!("/{id}.flac")]).unwrap();
        }
        let stored = [(1, "Rock, Metal, Pop"), (2, "Pop, Rock, Metal"), (3, "& CountryStyle:Heavy Metal"), (4, "Rock"), (5, " rock ;")];
        for (id, name) in stored {
            lib.db.execute("INSERT INTO genres (id, name) VALUES (?1, ?2)", params![id, name]).unwrap();
        }
        for (track, genre) in [(1, 1), (2, 2), (3, 3), (4, 4), (4, 5)] {
            lib.db.execute("INSERT INTO track_genres (track_id, genre_id) VALUES (?1, ?2)", params![track, genre]).unwrap();
        }
        assert_eq!(lib.split_stored_genres().unwrap(), 4);
        assert_eq!(lib.track_genres(1).unwrap(), ["Metal", "Pop", "Rock"]);
        assert_eq!(lib.track_genres(2).unwrap(), lib.track_genres(1).unwrap(), "the same genres in another order");
        assert_eq!(lib.track_genres(3).unwrap(), ["Country", "Heavy Metal"]);
        assert_eq!(lib.track_genres(4).unwrap(), ["Rock"]);
        let names: Vec<String> = lib.genres().unwrap().into_iter().map(|(name, _)| name).collect();
        assert_eq!(names.len(), 5, "{names:?}");
        assert_eq!(lib.split_stored_genres().unwrap(), 0, "once");
    }

    #[test]
    fn a_stored_genre_that_only_changes_its_spelling_keeps_its_tracks() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO tracks (id, source_id, external_id, title, added_at) VALUES (1, ?1, '/a.flac', 'A', 1)", params![source]).unwrap();
        lib.db.execute("INSERT INTO genres (id, name) VALUES (9, 'Hip   Hop')", []).unwrap();
        lib.db.execute("INSERT INTO track_genres (track_id, genre_id) VALUES (1, 9)", []).unwrap();
        assert_eq!(lib.split_stored_genres().unwrap(), 1);
        assert_eq!(lib.track_genres(1).unwrap(), ["Hip Hop"]);
    }
}

#[cfg(test)]
mod repair_tests {
    use super::*;

    #[test]
    fn a_healthy_library_has_nothing_to_report() {
        let lib = Library::in_memory().unwrap();
        assert!(lib.check().unwrap().is_empty());
    }

    #[test]
    fn repair_puts_back_what_was_taken_away() {
        // The rollups and the search index are copies, and this is what a
        // copy going wrong looks like: they are emptied behind the code's
        // back, which is what a crash mid-write or a file someone edited
        // would leave. Repair has to put them back from the tracks and the
        // log, and nothing else has to be touched.
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO albums (id, title, album_artist) VALUES (1, 'An album', 'Someone')", []).unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id,
                                     duration_ms, added_at)
                 VALUES (1, ?1, '/music/a.flac', 'A song', 'Someone', 1, 200000, 5)",
                params![source],
            )
            .unwrap();
        lib.set_track_genres(1, &["Delta Blues".to_string()]).unwrap();
        lib.log_play(Some(1), Some(source), 10, 200_000, 200_000, false, None, None, true).unwrap();

        assert_eq!(lib.search("song", 10).unwrap().len(), 1);
        assert_eq!(lib.search("delta", 10).unwrap().len(), 1, "the genre is indexed too");
        assert_eq!(lib.top_tracks(10).unwrap().len(), 1);

        lib.db.execute("DELETE FROM track_search", []).unwrap();
        lib.db.execute("DELETE FROM track_rollup", []).unwrap();
        lib.db.execute("DELETE FROM album_rollup", []).unwrap();
        assert!(lib.search("song", 10).unwrap().is_empty(), "gone, as staged");
        assert!(lib.top_tracks(10).unwrap().is_empty());

        lib.repair().unwrap();

        assert_eq!(lib.search("song", 10).unwrap().len(), 1, "the index came back");
        assert_eq!(lib.search("delta", 10).unwrap().len(), 1, "with the genres");
        assert_eq!(lib.top_tracks(10).unwrap().len(), 1, "and the counts");
        assert_eq!(lib.albums_sorted(crate::AlbumSort::Played, 10, 0).unwrap().len(), 1);
        assert!(lib.check().unwrap().is_empty(), "and it is healthy");
    }
}

/// Composers, each once, from values that may hold several names: split at
/// ";", "/" and line breaks, and kept apart at commas.
pub fn split_composers(values: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        for name in value.split([';', '/', '\0', '\n', '\r']) {
            let name = name.trim();
            if !name.is_empty() && !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                out.push(name.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn composers_are_split_at_semicolons_and_slashes_and_kept_at_commas() {
        let said = vec!["Lennon, McCartney; Martin".to_string(), "Bach / bach".to_string()];
        assert_eq!(split_composers(&said), ["Lennon, McCartney", "Martin", "Bach"]);
    }

    #[test]
    fn a_track_s_composers_are_replaced_not_added_to() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO tracks (id, source_id, external_id, title, added_at) VALUES (1, ?1, '/a.flac', 'A', 1)", params![source]).unwrap();
        lib.set_track_composers(1, &["Bach; Händel".to_string()]).unwrap();
        lib.set_track_composers(1, &["Bach".to_string()]).unwrap();
        let names: Vec<String> = lib.db.prepare("SELECT c.name FROM track_composers tc JOIN composers c ON c.id = tc.composer_id WHERE tc.track_id = 1").unwrap()
            .query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(names, ["Bach"]);
    }

    use super::*;
    use fixtures::*;

    /// What the planner knows about the library on a fresh row of numbers.
    fn believed_tracks(lib: &Library) -> i64 {
        use rusqlite::OptionalExtension;
        lib.db.query_row("SELECT stat FROM sqlite_stat1 WHERE tbl = 'tracks' LIMIT 1", [], |r| r.get::<_, String>(0)).optional().unwrap_or(None).and_then(|s| s.split_whitespace().next().and_then(|n| n.parse().ok())).unwrap_or(0)
    }

    #[test]
    fn a_track_with_no_album_never_makes_up_a_rollup_row() {
        // A loose file -- opened, not scanned into an album -- has a null
        // album_id. The rollup trigger used to insert that null into a
        // column that is INTEGER PRIMARY KEY, so SQLite made up a rowid and
        // the library got a rollup row for an album that does not exist.
        // On disk, where foreign keys are on, it was not junk but a hard
        // error out of the statement a scan runs on every file it finds
        // again, so a library with one loose file could not be rescanned.
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (id, source_id, external_id, title, artist,
                                     duration_ms, added_at)
                 VALUES (1, ?1, '/loose.flac', 'On its own', 'Someone', 200000, 0)",
                params![source],
            )
            .unwrap();

        // What a rescan does to a file it finds again.
        lib.db.execute("UPDATE tracks SET present = 1 WHERE id = 1", []).expect("it rescans");
        lib.db.execute("UPDATE tracks SET present = 0 WHERE id = 1", []).expect("and it goes");
        lib.db.execute("UPDATE tracks SET present = 1 WHERE id = 1", []).expect("and comes back");

        let rollups: i64 = lib.db.query_row("SELECT count(*) FROM album_rollup", [], |r| r.get(0)).unwrap();
        assert_eq!(rollups, 0, "no album, no rollup row");
        assert!(lib.check().unwrap().is_empty(), "and nothing points at nothing");

        // And a track that does have an album still gets counted.
        lib.db.execute("INSERT INTO albums (id, title, album_artist) VALUES (1, 'A', 'B')", []).unwrap();
        lib.db.execute("UPDATE tracks SET album_id = 1 WHERE id = 1", []).unwrap();
        let (album, present): (i64, i64) = lib.db.query_row("SELECT album_id, present_tracks FROM album_rollup", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((album, present), (1, 1), "it moved into the album and was counted");
    }

    #[test]
    fn writing_from_inside_a_transaction_does_not_ask_for_another() {
        // SQLite has no nested BEGIN, and a scan wraps batches of files in
        // one while calling straight into these. Before this, scanning a
        // file with a genre tag failed with "cannot start a transaction
        // within a transaction" -- and only when the file had genres, which
        // is why the tests that used bare files never saw it.
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (id, source_id, external_id, title, artist,
                                     duration_ms, added_at)
                 VALUES (1, ?1, '/a.flac', 'One', 'Someone', 200000, 0)",
                params![source],
            )
            .unwrap();

        let tx = lib.db.unchecked_transaction().unwrap();
        lib.set_track_genres(1, &["Rock".into(), "Jazz".into()]).expect("inside a transaction");
        let playlist = lib.create_playlist(source, "Mine", "").unwrap();
        lib.set_playlist_entries(playlist, &[PlaylistEntry { track_id: Some(1), source_id: Some(source), external_id: "/a.flac".into() }]).expect("inside a transaction");
        tx.commit().unwrap();

        assert_eq!(lib.track_genres(1).unwrap(), vec!["Jazz".to_string(), "Rock".to_string()]);
        assert_eq!(lib.playlist_entries(playlist).unwrap().len(), 1);

        // And on their own they still land together.
        lib.set_track_genres(1, &["Soul".into()]).unwrap();
        assert_eq!(lib.track_genres(1).unwrap(), vec!["Soul".to_string()]);
    }

    #[test]
    fn rebuilding_the_search_index_leaves_every_trigger_working() {
        // MIGRATION_V12 drops the search table and its five triggers and
        // makes them again, with their text repeated from MIGRATION_V7
        // because a migration cannot be edited once it has shipped. Two
        // copies drift, so this drives all five after the migration has run.
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO albums (id, title, album_artist) VALUES (1, 'Kind of Blue', 'Miles')", []).unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id,
                                     duration_ms, added_at)
                 VALUES (1, ?1, '/a.flac', 'So What', 'Miles Davis', 1, 200000, 0)",
                params![source],
            )
            .unwrap();

        // Inserting indexes it, by title, by artist and by its album.
        assert_eq!(lib.search("So What", 10).unwrap().len(), 1, "by title");
        assert_eq!(lib.search("Davis", 10).unwrap().len(), 1, "by artist");
        assert_eq!(lib.search("Kind of Blue", 10).unwrap().len(), 1, "by album");

        // A genre arriving and leaving refreshes it.
        lib.db.execute("INSERT INTO genres (id, name) VALUES (1, 'Modal')", []).unwrap();
        lib.db.execute("INSERT INTO track_genres (track_id, genre_id) VALUES (1, 1)", []).unwrap();
        assert_eq!(lib.search("Modal", 10).unwrap().len(), 1, "by genre");
        lib.db.execute("DELETE FROM track_genres WHERE track_id = 1", []).unwrap();
        assert!(lib.search("Modal", 10).unwrap().is_empty(), "and gone when the genre goes");

        // Retitling it moves it.
        lib.db.execute("UPDATE tracks SET title = 'Blue in Green' WHERE id = 1", []).unwrap();
        assert!(lib.search("So What", 10).unwrap().is_empty(), "not under the old title");
        assert_eq!(lib.search("Blue in Green", 10).unwrap().len(), 1, "under the new one");

        // And what typing looks like: two and three letters, which is what
        // the prefix index is for.
        assert_eq!(lib.search("Bl", 10).unwrap().len(), 1, "two letters");
        assert_eq!(lib.search("Gre", 10).unwrap().len(), 1, "three letters");

        // Deleting it takes it out.
        lib.db.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();
        assert!(lib.search("Blue in Green", 10).unwrap().is_empty(), "and gone when the track goes");
    }

    #[test]
    fn upgrading_a_library_leaves_a_copy_of_what_it_was() {
        // The copy is only worth having if it opens. This one is opened as a
        // library and asked for the track that was in it.
        let dir = std::env::temp_dir().join("chilpa-backup-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("library.db");

        // A library stopped at an older version, with something in it.
        {
            std::fs::create_dir_all(&dir).unwrap();
            let db = rusqlite::Connection::open(&path).unwrap();
            crate::schema::apply_pragmas(&db).unwrap();
            crate::schema::stop_at(&db, 1);
            db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", []).unwrap();
            db.execute(
                "INSERT INTO tracks (id, source_id, external_id, title, added_at)
                 VALUES (1, 1, '/a.flac', 'What was there', 0)",
                [],
            )
            .unwrap();
        }

        let backup = crate::schema::backup_path(&path);
        assert!(!backup.exists(), "nothing to go back to yet");

        let lib = Library::open(&path).unwrap();
        assert_eq!(lib.track_count(true).unwrap(), 1, "the library still has its track");
        assert!(backup.exists(), "and there is a way back");

        let kept = Library::open(&backup).unwrap();
        assert_eq!(kept.db.query_row("SELECT title FROM tracks WHERE id = 1", [], |r| r.get::<_, String>(0)).unwrap(), "What was there", "the copy opens and holds what was in it");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_that_needs_no_upgrade_is_not_copied() {
        let dir = std::env::temp_dir().join("chilpa-backup-none-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("library.db");
        drop(Library::open(&path).unwrap());
        // Opening it again changes nothing, so there is nothing to keep.
        drop(Library::open(&path).unwrap());
        assert!(!crate::schema::backup_path(&path).exists(), "a copy per open would be half a second and a second library on disk");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_that_was_filled_gets_its_statistics_when_it_opens() {
        // The bug this closes: the migrations end in ANALYZE and a migration
        // runs on an empty database, so the planner spent every library's
        // life believing it had no tracks. What that cost is in
        // `schema::update_statistics`.
        let dir = std::env::temp_dir().join("chilpa-stats-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("library.db");
        {
            let lib = Library::open(&path).unwrap();
            let source = lib.source_id("local", "default").unwrap();
            for i in 1..=5_000i64 {
                lib.db
                    .execute(
                        "INSERT INTO tracks (id, source_id, external_id, title, artist,
                                             duration_ms, added_at)
                         VALUES (?1, ?2, ?3, ?4, 'Someone', 200000, ?1)",
                        params![i, source, format!("/music/{i}.flac"), format!("Track {i}")],
                    )
                    .unwrap();
            }
            assert_eq!(believed_tracks(&lib), 0, "nothing has looked at it yet");
        }

        let lib = Library::open(&path).unwrap();
        assert_eq!(believed_tracks(&lib), 5_000, "opening it noticed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn statistics_that_are_current_are_not_redone() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for i in 1..=100i64 {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, artist,
                                         duration_ms, added_at)
                     VALUES (?1, ?2, ?3, ?4, 'Someone', 200000, ?1)",
                    params![i, source, format!("/music/{i}.flac"), format!("Track {i}")],
                )
                .unwrap();
        }
        lib.update_statistics().unwrap();
        assert_eq!(believed_tracks(&lib), 100);

        // Ten more is not a different library, and re-analyzing costs what a
        // full pass costs. The check is a factor, not an equality.
        for i in 101..=110i64 {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, artist,
                                         duration_ms, added_at)
                     VALUES (?1, ?2, ?3, ?4, 'Someone', 200000, ?1)",
                    params![i, source, format!("/music/{i}.flac"), format!("Track {i}")],
                )
                .unwrap();
        }
        crate::schema::update_statistics_if_stale(&lib.db).unwrap();
        assert_eq!(believed_tracks(&lib), 100, "still what it was");
    }

    #[test]
    fn the_source_and_the_root_are_created_once() {
        let lib = Library::in_memory().unwrap();
        let a = lib.source_id("local", "default").unwrap();
        let b = lib.source_id("local", "default").unwrap();
        assert_eq!(a, b);
        let r1 = lib.root_id(a, Path::new("/music")).unwrap();
        let r2 = lib.root_id(a, Path::new("/music")).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn an_unreachable_root_keeps_its_tracks_listed_and_unplayable() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let root = lib.root_id(source, Path::new("/nas")).unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (source_id, external_id, title, added_at)
                 VALUES (?1, '/nas/a.flac', 'One', 0)",
                params![source],
            )
            .unwrap();
        lib.db
            .execute(
                "INSERT INTO scanned_files (root_id, path, mtime, size, track_id, scanned_at)
                 VALUES (?1, '/nas/a.flac', 0, 0, 1, 0)",
                params![root],
            )
            .unwrap();

        assert!(lib.set_root_reachable(root, false).unwrap(), "it changed");
        assert!(!lib.set_root_reachable(root, false).unwrap(), "asking again changes nothing");

        assert_eq!(lib.track_count(false).unwrap(), 1, "still in the library, so still listed");
        let playable: i64 = lib.db.query_row("SELECT reachable FROM tracks WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(playable, 0, "and it cannot be played");

        assert!(lib.set_root_reachable(root, true).unwrap());
        let playable: i64 = lib.db.query_row("SELECT reachable FROM tracks WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(playable, 1, "back when the folder is, without a rescan");
    }

    #[test]
    fn forgetting_a_folder_stops_it_being_rescanned_and_deletes_nothing() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let root = lib.root_id(source, Path::new("/nas")).unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (source_id, external_id, title, added_at)
                 VALUES (?1, '/nas/a.flac', 'One', 0)",
                params![source],
            )
            .unwrap();
        lib.db
            .execute(
                "INSERT INTO scanned_files (root_id, path, mtime, size, track_id, scanned_at)
                 VALUES (?1, '/nas/a.flac', 0, 0, 1, 0)",
                params![root],
            )
            .unwrap();

        lib.forget_root(root).unwrap();

        assert!(lib.roots().unwrap().is_empty(), "it is not on the list to rescan");
        assert_eq!(lib.track_count(true).unwrap(), 1, "the track is not deleted");
        assert_eq!(lib.track_count(false).unwrap(), 0, "it stops counting as present");

        // Adding it back finds the same track again, with whatever history
        // it had: that is the difference between forgetting and deleting.
        let again = lib.root_id(source, Path::new("/nas")).unwrap();
        assert_ne!(again, 0);
        assert_eq!(lib.roots().unwrap().len(), 1);
    }

    #[test]
    fn search_looks_at_the_genre_too() {
        let lib = sample_library();
        // Album 1's track is blues; the others are not.
        let id: i64 = lib.db.query_row("SELECT id FROM tracks WHERE album_id = 1", [], |r| r.get(0)).unwrap();
        lib.set_track_genres(id, &["Delta Blues".to_string()]).unwrap();

        let found = lib.search("blues", 50).unwrap();
        assert_eq!(found.len(), 1, "it came out by its genre, not by its title");
        assert_eq!(found[0].id, id);
    }
}
