//! The open database, and what gets asked of it.
//!
//! Each subject has its file: the album grid, the playlists, what the
//! analysis extracts, the play log, and the fingerprints used to match.
//! What stays here is the connection, sources and roots, genres and search.

pub mod albums;
pub mod features;
pub mod fingerprints;
pub mod playlists;
pub mod plays;

pub use albums::{AlbumSort, StoredAlbum, StoredArtist};
pub use playlists::{PlaylistEntry, StoredPlaylist};
pub use plays::{Stats, TrackStat};

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
    pub present: bool,
}


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
        schema::migrate(&db)?;
        // Not only after a scan: a library that already exists carries the
        // statistics of an empty database and would keep them until its next
        // scan, which may be never.
        schema::update_statistics_if_stale(&db)?;
        Ok(Self { db })
    }

    /// An in-memory library, for tests.
    pub fn in_memory() -> rusqlite::Result<Self> {
        let db = Connection::open_in_memory()?;
        schema::migrate(&db)?;
        Ok(Self { db })
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
        self.db.execute(
            "INSERT OR IGNORE INTO sources (provider, instance) VALUES (?1, ?2)",
            params![provider, instance],
        )?;
        self.db.query_row(
            "SELECT id FROM sources WHERE provider = ?1 AND instance = ?2",
            params![provider, instance],
            |r| r.get(0),
        )
    }

    /// The root's id, creating it if needed.
    pub fn root_id(&self, source_id: i64, path: &Path) -> rusqlite::Result<i64> {
        let path = path.display().to_string();
        self.db.execute(
            "INSERT OR IGNORE INTO roots (source_id, path) VALUES (?1, ?2)",
            params![source_id, path],
        )?;
        self.db
            .query_row("SELECT id FROM roots WHERE path = ?1", params![path], |r| r.get(0))
    }

    /// Sets those genres as a track's, dropping the ones that no longer apply.
    ///
    /// They are replaced, not added to: if a file was retagged, the old genres
    /// are no longer its own.
    pub fn set_track_genres(&self, track_id: i64, genres: &[String]) -> rusqlite::Result<()> {
        let tx = self.db.unchecked_transaction()?;
        tx.execute("DELETE FROM track_genres WHERE track_id = ?1", params![track_id])?;
        {
            let mut insert = tx.prepare("INSERT OR IGNORE INTO genres (name) VALUES (?1)")?;
            let mut find = tx.prepare("SELECT id FROM genres WHERE name = ?1")?;
            let mut link = tx.prepare(
                "INSERT OR IGNORE INTO track_genres (track_id, genre_id) VALUES (?1, ?2)",
            )?;
            for name in genres {
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                insert.execute(params![name])?;
                let id: i64 = find.query_row(params![name], |r| r.get(0))?;
                link.execute(params![track_id, id])?;
            }
        }
        tx.commit()
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
        let rows =
            stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
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
        let mut stmt = self.db.prepare(
            "SELECT id, path, last_scan, reachable FROM roots ORDER BY path COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredRoot {
                id: r.get(0)?,
                path: r.get::<_, String>(1)?.into(),
                last_scan: r.get(2)?,
                reachable: r.get::<_, i64>(3)? != 0,
            })
        })?;
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
    pub fn forget_root(&self, root_id: i64) -> rusqlite::Result<()> {
        self.set_root_reachable(root_id, false)?;
        self.db.execute("DELETE FROM scanned_files WHERE root_id = ?1", params![root_id])?;
        self.db.execute("DELETE FROM roots WHERE id = ?1", params![root_id])?;
        Ok(())
    }

    /// Marks whether a root is reachable. A missing root prunes NOTHING.
    pub fn set_root_reachable(&self, root_id: i64, reachable: bool) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE roots SET reachable = ?2 WHERE id = ?1",
            params![root_id, i64::from(reachable)],
        )?;
        if !reachable {
            // What is its own is marked missing, not deleted.
            self.db.execute(
                "UPDATE tracks SET present = 0 WHERE id IN
                 (SELECT track_id FROM scanned_files WHERE root_id = ?1 AND track_id IS NOT NULL)",
                params![root_id],
            )?;
        }
        Ok(())
    }

    /// How many tracks there are, counting the missing ones or not.
    pub fn track_count(&self, include_absent: bool) -> rusqlite::Result<usize> {
        let sql = if include_absent {
            "SELECT count(*) FROM tracks"
        } else {
            "SELECT count(*) FROM tracks WHERE present = 1"
        };
        let n: i64 = self.db.query_row(sql, [], |r| r.get(0))?;
        Ok(n as usize)
    }


    /// Searches by title, artist or album.
    pub fn search(&self, text: &str, limit: usize) -> rusqlite::Result<Vec<StoredTrack>> {
        let Some(query) = fts_query(text) else { return Ok(Vec::new()) };
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present
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
        let rows = stmt.query_map([], |r| {
            Ok(format!(
                "{}: a row points at a {} that is not there",
                r.get::<_, String>(0)?,
                r.get::<_, String>(2)?
            ))
        })?;
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
        self.db.execute_batch(
            "BEGIN;
             DELETE FROM album_rollup;
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
             COMMIT;",
        )
    }

    /// The track at that path, if it is in the library.
    pub fn track_by_path(&self, source_id: i64, path: &Path) -> rusqlite::Result<Option<i64>> {
        self.db
            .query_row(
                "SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2",
                params![source_id, path.display().to_string()],
                |r| r.get(0),
            )
            .optional()
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
fn fts_query(text: &str) -> Option<String> {
    let tokens: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    (!tokens.is_empty()).then(|| tokens.join(" "))
}

pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn row_to_track(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredTrack> {
    Ok(StoredTrack {
        id: r.get(0)?,
        external_id: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        track_number: r.get::<_, Option<i64>>(5)?.map(|n| n as u16),
        duration_ms: r.get::<_, i64>(6)? as u64,
        present: r.get::<_, i64>(7)? == 1,
    })
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
        for (id, title, artist, año, high) in [
            (1, "Bayou", "Creedence", 1969, 100),
            (2, "Aqualung", "Tull", 1971, 300),
            (3, "Cosmo", "Creedence", 1970, 200),
        ] {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist, year)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, title, artist, año],
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
        lib.albums_sorted(order, 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, _)| a.title)
            .collect()
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
        lib.db
            .execute(
                "INSERT INTO albums (id, title, album_artist) VALUES (1, 'An album', 'Someone')",
                [],
            )
            .unwrap();
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

#[cfg(test)]
mod tests {
    use super::*;
    use fixtures::*;

    /// What the planner knows about the library on a fresh row of numbers.
    fn believed_tracks(lib: &Library) -> i64 {
        use rusqlite::OptionalExtension;
        lib.db
            .query_row("SELECT stat FROM sqlite_stat1 WHERE tbl = 'tracks' LIMIT 1", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .unwrap_or(None)
            .and_then(|s| s.split_whitespace().next().and_then(|n| n.parse().ok()))
            .unwrap_or(0)
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
    fn an_unreachable_root_marks_its_tracks_but_does_not_delete_them() {
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

        lib.set_root_reachable(root, false).unwrap();

        assert_eq!(lib.track_count(true).unwrap(), 1, "the row is still there");
        assert_eq!(lib.track_count(false).unwrap(), 0, "but it no longer counts as present");
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
        let id: i64 = lib
            .db
            .query_row("SELECT id FROM tracks WHERE album_id = 1", [], |r| r.get(0))
            .unwrap();
        lib.set_track_genres(id, &["Delta Blues".to_string()]).unwrap();

        let found = lib.search("blues", 50).unwrap();
        assert_eq!(found.len(), 1, "it came out by its genre, not by its title");
        assert_eq!(found[0].id, id);
    }

}
