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
        Ok(Self { db })
    }

    /// An in-memory library, for tests.
    pub fn in_memory() -> rusqlite::Result<Self> {
        let db = Connection::open_in_memory()?;
        schema::migrate(&db)?;
        Ok(Self { db })
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
        let patron = format!("%{text}%");
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present
             FROM tracks t LEFT JOIN albums a ON a.id = t.album_id
             WHERE t.present = 1
               AND (t.title LIKE ?1 OR t.artist LIKE ?1 OR a.title LIKE ?1
                    OR EXISTS (SELECT 1 FROM track_genres tg
                               JOIN genres g ON g.id = tg.genre_id
                               WHERE tg.track_id = t.id AND g.name LIKE ?1))
             ORDER BY t.artist COLLATE NOCASE, t.title COLLATE NOCASE
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![patron, limit as i64], row_to_track)?;
        rows.collect()
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
mod tests {
    use super::*;
    use fixtures::*;

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
