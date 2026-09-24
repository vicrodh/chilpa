//! Saved playlists: the hand-made ones and the ones a source brings in.
//!
//! An entry stores the path or the external id as well as the track, so a
//! playlist does not break because something is not in the library yet.

use crate::store::Library;
use rusqlite::{params, OptionalExtension};
use crate::store::now_unix;

/// A playlist, either local or brought in by a source.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredPlaylist {
    pub id: i64,
    /// Where it comes from: "local", or the provider that brought it.
    pub provider: String,
    /// The id its source gives it, empty when it is local.
    pub external_id: String,
    pub name: String,
    pub description: String,
    pub artwork_path: Option<String>,
    pub track_count: usize,
    pub updated_at: i64,
}

/// One entry of a playlist.
///
/// It stores the path or the external id as well as the track: a playlist
/// does not break because something is not in the library yet.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlaylistEntry {
    pub track_id: Option<i64>,
    pub external_id: String,
}

impl Library {
    /// Creates a playlist, or returns the one that already has that external id.
    ///
    /// An empty `external_id` means the playlist is local: two hand-made lists
    /// with the same name are two different lists, as in any player.
        pub fn create_playlist(
        &self,
        source_id: i64,
        name: &str,
        external_id: &str,
    ) -> rusqlite::Result<i64> {
        let now = now_unix();
        if !external_id.is_empty() {
            if let Some(id) = self
                .db
                .query_row(
                    "SELECT id FROM playlists WHERE source_id = ?1 AND external_id = ?2",
                    params![source_id, external_id],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
            {
                self.db.execute(
                    "UPDATE playlists SET name = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, name, now],
                )?;
                return Ok(id);
            }
        }
        self.db.execute(
            "INSERT INTO playlists (source_id, external_id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![
                source_id,
                (!external_id.is_empty()).then_some(external_id),
                name,
                now
            ],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    /// Replaces a playlist's contents, in that order.
    ///
    /// Done in a transaction: a half-written playlist is worse than an old
    /// one.
    pub fn set_playlist_entries(
        &self,
        playlist_id: i64,
        entries: &[PlaylistEntry],
    ) -> rusqlite::Result<()> {
        // Its own transaction unless the caller already has one: SQLite has
        // no nested BEGIN. See `Library::own_transaction`.
        let tx = self.own_transaction()?;
        tx.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", params![playlist_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO playlist_tracks (playlist_id, position, track_id, external_id)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (pos, e) in entries.iter().enumerate() {
                stmt.execute(params![playlist_id, pos as i64, e.track_id, e.external_id])?;
            }
        }
        tx.execute(
            "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
            params![playlist_id, now_unix()],
        )?;
        tx.finish()
    }

    /// The playlists, with how many tracks each carries.
    pub fn playlists(&self) -> rusqlite::Result<Vec<StoredPlaylist>> {
        let mut stmt = self.db.prepare(
            "SELECT p.id, s.provider, COALESCE(p.external_id, ''), p.name, p.description,
                    p.artwork_path, count(pt.position), p.updated_at
             FROM playlists p
             JOIN sources s ON s.id = p.source_id
             LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id
             GROUP BY p.id
             ORDER BY p.updated_at DESC, p.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredPlaylist {
                id: r.get(0)?,
                provider: r.get(1)?,
                external_id: r.get(2)?,
                name: r.get(3)?,
                description: r.get(4)?,
                artwork_path: r.get(5)?,
                track_count: r.get::<_, i64>(6)? as usize,
                updated_at: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// What a playlist carries, in its order.
    pub fn playlist_entries(&self, playlist_id: i64) -> rusqlite::Result<Vec<PlaylistEntry>> {
        let mut stmt = self.db.prepare(
            "SELECT track_id, external_id FROM playlist_tracks
             WHERE playlist_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map(params![playlist_id], |r| {
            Ok(PlaylistEntry { track_id: r.get(0)?, external_id: r.get(1)? })
        })?;
        rows.collect()
    }

    /// Renames a playlist.
    pub fn rename_playlist(&self, playlist_id: i64, name: &str) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE playlists SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![playlist_id, name, now_unix()],
        )?;
        Ok(())
    }

    /// Deletes a playlist. The tracks are untouched: a playlist is a view.
    pub fn delete_playlist(&self, playlist_id: i64) -> rusqlite::Result<()> {
        self.db
            .execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", params![playlist_id])?;
        self.db.execute("DELETE FROM playlists WHERE id = ?1", params![playlist_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fixtures::*;

    #[test]
    fn a_playlist_keeps_its_order_and_survives_what_is_missing() {
        let lib = sample_library();
        let source = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(source, "De la mañana", "").unwrap();

        let entries = vec![
            PlaylistEntry { track_id: Some(2), external_id: "/Aqualung.flac".into() },
            // Something the playlist names that has not been scanned yet.
            PlaylistEntry { track_id: None, external_id: "/pending.flac".into() },
            PlaylistEntry { track_id: Some(1), external_id: "/Bayou.flac".into() },
        ];
        lib.set_playlist_entries(id, &entries).unwrap();

        let read_back = lib.playlist_entries(id).unwrap();
        assert_eq!(read_back, entries, "the order is the one that was saved");
        assert_eq!(read_back[1].track_id, None, "what is not there is still noted");

        let lists = lib.playlists().unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].track_count, 3);
        assert_eq!(lists[0].provider, "local");
        assert!(lists[0].external_id.is_empty(), "a list from here has no external id");
    }

    #[test]
    fn fetching_a_source_playlist_again_updates_it_without_duplicating_it() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("server", "home").unwrap();
        let a = lib.create_playlist(source, "Mix", "abc123").unwrap();
        let b = lib.create_playlist(source, "Renamed mix", "abc123").unwrap();
        assert_eq!(a, b, "the same external id is the same list");
        let lists = lib.playlists().unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].name, "Renamed mix", "the new name stuck");
    }

    #[test]
    fn two_hand_made_playlists_with_the_same_name_are_two_playlists() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let a = lib.create_playlist(source, "Unnamed", "").unwrap();
        let b = lib.create_playlist(source, "Unnamed", "").unwrap();
        assert_ne!(a, b);
        assert_eq!(lib.playlists().unwrap().len(), 2);
    }

    #[test]
    fn replacing_the_contents_leaves_no_leftovers() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(source, "Mine", "").unwrap();
        let three: Vec<PlaylistEntry> = (0..3)
            .map(|i| PlaylistEntry { track_id: None, external_id: format!("/{i}.flac") })
            .collect();
        lib.set_playlist_entries(id, &three).unwrap();
        lib.set_playlist_entries(id, &three[..1]).unwrap();
        assert_eq!(lib.playlist_entries(id).unwrap().len(), 1, "the old ones did not stay");
    }

    #[test]
    fn deleting_a_playlist_does_not_touch_the_tracks() {
        let lib = sample_library();
        let source = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(source, "Temporal", "").unwrap();
        lib.set_playlist_entries(
            id,
            &[PlaylistEntry { track_id: Some(1), external_id: "/Bayou.flac".into() }],
        )
        .unwrap();
        lib.delete_playlist(id).unwrap();
        assert!(lib.playlists().unwrap().is_empty());
        assert_eq!(lib.track_count(false).unwrap(), 3, "the tracks are still there");
    }

}
