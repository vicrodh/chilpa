//! A local release is its source, folder and title, not each file's credit.
//! The original credits remain on tracks for artist browsing and searching.

use super::Library;
use rusqlite::params;
use std::collections::BTreeMap;

impl Library {
    /// Repair existing fragments without rescanning files or changing track ids.
    pub fn join_local_compilations(&self) -> rusqlite::Result<usize> {
        let mut groups: BTreeMap<(String, String), Vec<i64>> = BTreeMap::new();
        let mut stmt = self.db.prepare("SELECT a.id, a.title, a.version_key FROM albums a
            WHERE a.version_key <> ''
            AND EXISTS (SELECT 1 FROM tracks t JOIN sources s ON s.id = t.source_id
                WHERE t.album_id = a.id AND s.provider = 'local')
            AND NOT EXISTS (SELECT 1 FROM tracks t JOIN sources s ON s.id = t.source_id
                WHERE t.album_id = a.id AND s.provider <> 'local') ORDER BY a.id")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
            let (id, title, key) = row?;
            groups.entry((title, key)).or_default().push(id);
        }
        let tx = self.own_transaction()?;
        let mut merged = 0;
        for ((title, _), ids) in groups.into_iter().filter(|(_, ids)| ids.len() > 1) {
            let keep = ids[0];
            let group = super::versions::album_group(&title, crate::VARIOUS_ARTISTS);
            for &other in &ids[1..] {
                super::ratings::merge_album_rating(&tx, other, keep)?;
                tx.execute("UPDATE albums SET
                    artwork_path = coalesce(artwork_path, (SELECT artwork_path FROM albums WHERE id = ?2)),
                    year = coalesce(year, (SELECT year FROM albums WHERE id = ?2)) WHERE id = ?1", params![keep, other])?;
                tx.execute("INSERT OR IGNORE INTO album_favorites(album_id, added_at)
                    SELECT ?1, added_at FROM album_favorites WHERE album_id = ?2", params![keep, other])?;
                tx.execute("INSERT OR IGNORE INTO album_preferences(group_key, album_id)
                    SELECT ?1, ?2 FROM album_preferences WHERE album_id IN (?2, ?3)", params![group, keep, other])?;
                tx.execute("UPDATE album_preferences SET album_id = ?1 WHERE album_id = ?2", params![keep, other])?;
                tx.execute("UPDATE tracks SET album_id = ?1 WHERE album_id = ?2", params![keep, other])?;
                tx.execute("DELETE FROM albums WHERE id = ?1", [other])?;
                merged += 1;
            }
            tx.execute("UPDATE albums SET album_artist = ?2 WHERE id = ?1", params![keep, crate::VARIOUS_ARTISTS])?;
        }
        if merged > 0 { self.rebuild_rollups()?; }
        tx.finish()?;
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{AlbumSort, Filter, TrackOrder};

    #[test]
    fn upgrading_joins_one_release_and_keeps_credits_tracks_favorites_and_history() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        crate::schema::apply_pragmas(&db).unwrap();
        crate::schema::stop_at(&db, 21);
        db.execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default'), (2, 'server', 'home');
            INSERT INTO albums(id, title, album_artist, folder, version_key) VALUES
            (1, 'Reference CD', 'Singer A', '/reference', '1:/reference'),
            (2, 'Reference CD', 'Singer B', '/reference', '1:/reference'),
            (3, 'Reference CD', 'Other Artist', '/another', '1:/another'),
            (4, 'Reference CD', 'Remote Artist', '', '2:reference');
            INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, duration_ms, added_at) VALUES
            (10, 1, '/reference/01.flac', 'One', '歌手 A', 1, 1000, 1),
            (11, 1, '/reference/02.flac', 'Two', '歌手 B', 2, 2000, 1),
            (12, 1, '/another/01.flac', 'Three', 'Other Artist', 3, 1000, 1),
            (13, 2, 'opaque', 'Four', 'Remote Artist', 4, 1000, 1);
            INSERT INTO album_favorites VALUES (2, 123);
            INSERT INTO album_preferences VALUES ('old-group', 2);").unwrap();
        crate::schema::migrate(&db).unwrap();
        // Before V27 every source that was not local was a service, and the
        // migration takes its tracks out of the library. Source 2 stands for
        // a server of mine here, which is what a server is from V27 on.
        db.execute_batch("UPDATE sources SET kind = 'server' WHERE id = 2;
            UPDATE tracks SET present = 1 WHERE source_id = 2;").unwrap();
        let lib = Library { db, versions: Default::default() };
        lib.log_play(Some(11), None, 1, 2000, 2000, false, None, None, true).unwrap();
        let playlist = lib.create_playlist(1, "Mix", "mix").unwrap();
        let entries = lib.playlist_entries_for_tracks(&[10, 11]).unwrap();
        lib.append_to_playlist(playlist, &entries).unwrap();
        assert_eq!(lib.join_local_compilations().unwrap(), 1);
        assert_eq!(lib.join_local_compilations().unwrap(), 0, "idempotent");
        assert_eq!(lib.album_tracks(1).unwrap().iter().map(|t| t.id).collect::<Vec<_>>(), [10, 11]);
        assert_eq!(lib.album(1).unwrap().unwrap().0.album_artist, crate::VARIOUS_ARTISTS);
        assert_eq!(lib.album(1).unwrap().unwrap().1, 1, "play history follows tracks");
        assert_eq!(lib.playlist_entries(playlist).unwrap(), entries);
        assert!(lib.favorite_albums().unwrap().contains(&1));
        assert_eq!(lib.db.query_row("SELECT album_id FROM album_preferences WHERE group_key = 'old-group'", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        let performed = lib.grouped_snapshot(&Filter { performer: Some("歌手 B".into()), ..Filter::default() }, TrackOrder::Album).unwrap();
        assert_eq!(performed.ids, [11], "an artist's tracks exclude other compilation performers");
        assert_eq!(lib.album_ids(AlbumSort::Title).unwrap().len(), 3, "different folders and sources stay separate");
        assert!(lib.check().unwrap().is_empty());
        for artist in ["Singer A", "Singer B", "歌手 A", "歌手 B", "Various Artists"] {
            let filter = Filter { artist: Some(artist.into()), ..Filter::default() };
            let snapshot = lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap();
            assert_eq!(snapshot.ids.len(), 2, "the complete album for {artist}");
            assert_eq!(snapshot.totals.duration_ms, 3000);
            assert_eq!(snapshot.album_ids, [1]);
            assert!(lib.facet_artists(&Filter::default()).unwrap().iter().any(|a| a.label == artist && a.albums == 1));
            assert_eq!(lib.album_ids_matching(AlbumSort::Title, artist).unwrap(), [1]);
            assert_eq!(lib.find(artist, 20).unwrap().albums, [1]);
            let mut typed = Filter::default();
            typed.queries[2] = artist.into();
            assert_eq!(lib.grouped_snapshot(&typed, TrackOrder::Album).unwrap().ids.len(), 2);
        }
        lib.set_group_album_versions(false).unwrap();
        assert_eq!(lib.albums(100, 0).unwrap().len(), 3, "a compilation is one edition even without version grouping");
        lib.db.execute("UPDATE tracks SET present = 0 WHERE id = 11", []).unwrap();
        assert!(!lib.facet_artists(&Filter::default()).unwrap().iter().any(|a| a.label == "Singer B"));
    }
}
