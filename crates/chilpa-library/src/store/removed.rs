//! Albums taken out of the library, and brought back. Their files are never
//! touched: the tracks stop being listed, counted and found, and a scan or a
//! server's sync that sees them again keeps them out until they are brought
//! back.

use rusqlite::params;

use super::Library;

/// An album taken out of the library, as the list of removed albums shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedAlbum {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub artwork_path: String,
    pub tracks: usize,
    pub removed_at: i64,
}

impl Library {
    /// Takes an album out of the library, with every track it has. Returns
    /// how many tracks went.
    pub fn remove_album(&self, album_id: i64, now: i64) -> rusqlite::Result<usize> {
        let tx = self.own_transaction()?;
        let taken = tx.execute(
            "INSERT OR IGNORE INTO removed_tracks (track_id, was_present, removed_at)
             SELECT id, present, ?2 FROM tracks WHERE album_id = ?1",
            params![album_id, now],
        )?;
        tx.execute("UPDATE tracks SET present = 0 WHERE album_id = ?1 AND present <> 0", params![album_id])?;
        tx.finish()?;
        Ok(taken)
    }

    /// The albums taken out, the latest first.
    pub fn removed_albums(&self) -> rusqlite::Result<Vec<RemovedAlbum>> {
        let mut stmt = self.db.prepare(
            "SELECT a.id, a.title, a.album_artist,
                    coalesce(a.artwork_path, max(t.release_artwork), ''), count(*), max(r.removed_at)
             FROM removed_tracks r JOIN tracks t ON t.id = r.track_id JOIN albums a ON a.id = t.album_id
             GROUP BY a.id ORDER BY max(r.removed_at) DESC, a.title",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(RemovedAlbum {
                id: r.get(0)?,
                title: r.get(1)?,
                artist: r.get(2)?,
                artwork_path: r.get(3)?,
                tracks: r.get::<_, i64>(4)? as usize,
                removed_at: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Brings an album back as it was when it was taken out: a track that
    /// was there is listed again; one whose file has gone since is found
    /// missing by the next scan. Returns how many tracks came back.
    pub fn restore_album(&self, album_id: i64) -> rusqlite::Result<usize> {
        let tx = self.own_transaction()?;
        tx.execute(
            "UPDATE tracks SET present = (SELECT was_present FROM removed_tracks WHERE track_id = tracks.id)
             WHERE album_id = ?1 AND id IN (SELECT track_id FROM removed_tracks)",
            params![album_id],
        )?;
        let back = tx.execute(
            "DELETE FROM removed_tracks WHERE track_id IN (SELECT id FROM tracks WHERE album_id = ?1)",
            params![album_id],
        )?;
        tx.finish()?;
        Ok(back)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One local album of three tracks, the third already missing.
    fn library() -> (Library, i64) {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let db = lib.connection();
        db.execute_batch(
            "INSERT INTO albums (id, title, album_artist, artwork_path) VALUES (7, 'Disco', 'Artist', '/art/record.jpg');
             INSERT INTO albums (id, title, album_artist) VALUES (8, 'Other', 'Artist');",
        )
        .unwrap();
        for (id, album, present) in [(1, 7, 1), (2, 7, 1), (3, 7, 0), (4, 8, 1)] {
            db.execute(
                "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, present, added_at)
                 VALUES (?1, ?2, ?3, 'Track', 'Artist', ?4, ?5, 0)",
                params![id, source, format!("/music/{id}.flac"), album, present],
            )
            .unwrap();
        }
        (lib, 7)
    }

    fn present(lib: &Library) -> Vec<i64> {
        let mut stmt = lib.connection().prepare("SELECT id FROM tracks WHERE present = 1 ORDER BY id").unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.collect::<rusqlite::Result<_>>().unwrap()
    }

    #[test]
    fn a_removed_album_leaves_the_library_and_is_listed_to_be_brought_back() {
        let (lib, record) = library();
        assert_eq!(lib.remove_album(record, 1_000).unwrap(), 3, "its missing track goes too");
        assert_eq!(present(&lib), [4], "only the other album is left");
        let removed = lib.removed_albums().unwrap();
        assert_eq!(removed, [RemovedAlbum { id: 7, title: "Disco".into(), artist: "Artist".into(), artwork_path: "/art/record.jpg".into(), tracks: 3, removed_at: 1_000 }]);
        assert_eq!(lib.remove_album(record, 2_000).unwrap(), 0, "taking it out twice changes nothing");
        assert_eq!(lib.removed_albums().unwrap()[0].removed_at, 1_000);
    }

    #[test]
    fn a_brought_back_album_is_as_it_was_when_it_left() {
        let (lib, record) = library();
        lib.remove_album(record, 1_000).unwrap();
        assert_eq!(lib.restore_album(record).unwrap(), 3);
        assert_eq!(present(&lib), [1, 2, 4], "the missing track is still missing");
        assert!(lib.removed_albums().unwrap().is_empty());
        assert_eq!(lib.restore_album(record).unwrap(), 0);
    }

    #[test]
    fn a_track_found_again_stays_out_while_its_album_is_removed() {
        let (lib, record) = library();
        lib.remove_album(record, 1_000).unwrap();
        // What a scan does for a file it finds again under the same path.
        lib.connection()
            .execute(
                "UPDATE tracks SET present = NOT EXISTS (SELECT 1 FROM removed_tracks WHERE track_id = ?1), reachable = 1 WHERE id = ?1",
                [1],
            )
            .unwrap();
        assert_eq!(present(&lib), [4]);
    }

    #[test]
    fn a_track_that_is_deleted_leaves_the_list_of_removed_albums() {
        let (lib, record) = library();
        lib.remove_album(record, 1_000).unwrap();
        lib.connection().execute("DELETE FROM tracks WHERE album_id = 7", []).unwrap();
        assert!(lib.removed_albums().unwrap().is_empty());
    }
}
