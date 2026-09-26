//! Joining the records an earlier scan split by artist.
//!
//! Before records without an album artist were grouped by folder, the scan
//! keyed them by each track's artist, and a compilation became one record
//! per artist. A rescan does not fix it -- it only rereads files that
//! changed -- so the library looks for such splits when it opens.
//!
//! A split shows itself: several records with the same title whose files
//! sit in the same record folder, each one's artist the artist of every
//! track in it. That last part is what a split by artist leaves and what a
//! record whose files name an album artist never does, so two records that
//! were tagged apart stay apart.

use crate::album_folder::{album_folder, VARIOUS_ARTISTS};
use crate::store::Library;
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// What one record contributes to a group.
#[derive(Default)]
struct Split {
    albums: BTreeSet<i64>,
    artists: BTreeSet<String>,
    /// False when a track's artist is not its record's: it was tagged.
    by_artist: bool,
}

impl Library {
    /// Joins the records an earlier scan split by artist. Returns how many
    /// records were folded into another.
    pub fn regroup_split_albums(&self) -> rusqlite::Result<usize> {
        // Only a title more than one record shares can hide a split, which
        // keeps this to a handful of rows on a library that has none.
        let rows: Vec<(i64, String, String, String, String)> = {
            let mut stmt = self.db.prepare(
                "SELECT a.id, a.title, a.album_artist, t.external_id, t.artist
                 FROM albums a JOIN tracks t ON t.album_id = a.id
                 WHERE a.folder = ''
                   AND a.title IN (SELECT title FROM albums GROUP BY title HAVING count(*) > 1)",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut groups: BTreeMap<(String, String), Split> = BTreeMap::new();
        let mut tagged: BTreeSet<i64> = BTreeSet::new();
        for (album, title, album_artist, path, artist) in rows {
            if artist != album_artist {
                tagged.insert(album);
            }
            let group = groups.entry((title, album_folder(Path::new(&path)))).or_default();
            group.albums.insert(album);
            group.artists.insert(artist);
        }
        for group in groups.values_mut() {
            group.by_artist = group.albums.iter().all(|a| !tagged.contains(a));
        }
        let splits: Vec<(&String, &Split)> = groups
            .iter()
            .filter(|(_, g)| g.albums.len() > 1 && g.by_artist)
            .map(|((_, folder), g)| (folder, g))
            .collect();
        if splits.is_empty() {
            return Ok(0);
        }

        let mut folded = 0;
        let tx = self.own_transaction()?;
        for (folder, group) in splits {
            let mut ids = group.albums.iter().copied();
            let Some(keep) = ids.next() else { continue };
            let rest: Vec<i64> = ids.collect();
            let artist = if group.artists.len() > 1 {
                VARIOUS_ARTISTS.to_string()
            } else {
                group.artists.iter().next().cloned().unwrap_or_default()
            };
            for other in &rest {
                // What another part knew that the one kept did not.
                tx.execute(
                    "UPDATE albums SET
                       artwork_path = COALESCE(artwork_path, (SELECT artwork_path FROM albums WHERE id = ?2)),
                       year = COALESCE(year, (SELECT year FROM albums WHERE id = ?2))
                     WHERE id = ?1",
                    params![keep, other],
                )?;
                tx.execute("UPDATE tracks SET album_id = ?1 WHERE album_id = ?2", params![keep, other])?;
                tx.execute("DELETE FROM albums WHERE id = ?1", params![other])?;
                folded += 1;
            }
            tx.execute(
                "UPDATE albums SET album_artist = ?2, folder = ?3 WHERE id = ?1",
                params![keep, artist, folder],
            )?;
        }
        tx.finish()?;
        // The plays counted on the parts that went away belong to the record
        // kept; the counts are rebuilt from the tracks and the log.
        self.rebuild_rollups()?;
        Ok(folded)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::Library;
    use rusqlite::params;

    fn track(lib: &Library, id: i64, album: i64, path: &str, artist: &str) {
        let source = lib.source_id("local", "default").unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, duration_ms, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1000, ?1)",
                params![id, source, path, format!("T{id}"), artist, album],
            )
            .unwrap();
    }

    fn album(lib: &Library, id: i64, title: &str, artist: &str) {
        lib.db
            .execute("INSERT INTO albums (id, title, album_artist) VALUES (?1, ?2, ?3)", params![id, title, artist])
            .unwrap();
    }

    #[test]
    fn a_compilation_split_by_artist_becomes_one_record() {
        let lib = Library::in_memory().unwrap();
        album(&lib, 1, "OP&ED", "ORANGE RANGE");
        album(&lib, 2, "OP&ED", "YUI");
        album(&lib, 3, "OP&ED", "Aimer");
        track(&lib, 10, 1, "/m/Bleach/OP&ED/CD1/01.flac", "ORANGE RANGE");
        track(&lib, 11, 2, "/m/Bleach/OP&ED/CD1/02.flac", "YUI");
        track(&lib, 12, 3, "/m/Bleach/OP&ED/CD2/01.flac", "Aimer");
        lib.log_play(Some(11), None, 1, 1000, 1000, false, None, None, true).unwrap();

        assert_eq!(lib.regroup_split_albums().unwrap(), 2);
        let (count, artist, folder): (i64, String, String) = lib
            .db
            .query_row("SELECT count(*), album_artist, folder FROM albums", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!((count, artist.as_str(), folder.as_str()), (1, "Various Artists", "/m/Bleach/OP&ED"));
        let (present, plays): (i64, i64) = lib
            .db
            .query_row("SELECT present_tracks, plays FROM album_rollup WHERE album_id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((present, plays), (3, 1), "the moved tracks and their play count on the record kept");
        assert_eq!(lib.regroup_split_albums().unwrap(), 0, "once");
    }

    #[test]
    fn records_tagged_apart_stay_apart() {
        let lib = Library::in_memory().unwrap();
        // Same title, same folder, but a track whose artist is not its
        // record's: the files named an album artist.
        album(&lib, 1, "Split EP", "Band A");
        album(&lib, 2, "Split EP", "Band B");
        track(&lib, 10, 1, "/m/Split/01.flac", "Band A feat. C");
        track(&lib, 11, 2, "/m/Split/02.flac", "Band B");
        assert_eq!(lib.regroup_split_albums().unwrap(), 0);
    }

    #[test]
    fn the_same_title_in_two_folders_is_two_records() {
        let lib = Library::in_memory().unwrap();
        album(&lib, 1, "Greatest Hits", "Queen");
        album(&lib, 2, "Greatest Hits", "ABBA");
        track(&lib, 10, 1, "/m/Queen/Greatest Hits/01.flac", "Queen");
        track(&lib, 11, 2, "/m/ABBA/Greatest Hits/01.flac", "ABBA");
        assert_eq!(lib.regroup_split_albums().unwrap(), 0);
    }
}
