//! I resolve device selections on a read-only library snapshot, without exposing its connection.
use super::Library;
use rusqlite::{params, params_from_iter, OptionalExtension};

#[derive(Clone, Debug)]
pub struct ExportTrack {
    pub id: i64,
    pub key: String,
    pub path: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub number: u32,
    pub disc: u32,
    pub available: bool,
}
#[derive(Clone, Debug)]
pub struct ExportPlaylist {
    pub key: String,
    pub name: String,
    pub tracks: usize,
}

impl Library {
    pub fn export_tracks(&self, ids: &[i64]) -> rusqlite::Result<Vec<ExportTrack>> {
        let mut found = std::collections::HashMap::new();
        for chunk in ids.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let mut statement = self.db.prepare(&format!("SELECT t.id,s.provider,s.instance,t.external_id,t.title,
                coalesce(nullif(a.album_artist,''),t.artist),coalesce(a.title,''),coalesce(t.track_number,0),
                coalesce(t.disc_number,0),t.present AND coalesce(r.reachable,1)
                FROM tracks t JOIN sources s ON s.id=t.source_id LEFT JOIN albums a ON a.id=t.album_id
                LEFT JOIN scanned_files f ON f.path=t.external_id LEFT JOIN roots r ON r.id=f.root_id
                WHERE t.id IN ({marks})"))?;
            for row in statement.query_map(params_from_iter(chunk), |r| {
                let provider: String = r.get(1)?;
                let instance: String = r.get(2)?;
                let external: String = r.get(3)?;
                Ok(ExportTrack {
                    id: r.get(0)?,
                    key: serde_json::to_string(&(provider.as_str(), instance, external.as_str()))
                        .unwrap(),
                    path: (provider == "local" && std::path::Path::new(&external).is_absolute())
                        .then_some(external),
                    title: r.get(4)?,
                    artist: r.get(5)?,
                    album: r.get(6)?,
                    number: r.get::<_, i64>(7)? as u32,
                    disc: r.get::<_, i64>(8)? as u32,
                    available: r.get::<_, bool>(9)?,
                })
            })? {
                let row = row?;
                found.insert(row.id, row);
            }
        }
        ids.iter()
            .map(|id| {
                found
                    .get(id)
                    .cloned()
                    .ok_or(rusqlite::Error::QueryReturnedNoRows)
            })
            .collect()
    }
    pub fn export_track_ids(&self, keys: &[String]) -> rusqlite::Result<Vec<i64>> {
        let mut statement = self.db.prepare(
            "SELECT t.id FROM tracks t JOIN sources s ON s.id=t.source_id
            WHERE s.provider=?1 AND s.instance=?2 AND t.external_id=?3",
        )?;
        keys.iter()
            .map(|key| {
                let (provider, instance, external): (String, String, String) =
                    serde_json::from_str(key).map_err(|_| rusqlite::Error::InvalidQuery)?;
                statement.query_row(params![provider, instance, external], |r| r.get(0))
            })
            .collect()
    }
    pub fn export_playlists(&self) -> rusqlite::Result<Vec<ExportPlaylist>> {
        let mut statement = self.db.prepare("SELECT k.key,p.name,(SELECT count(*) FROM playlist_tracks pt WHERE pt.playlist_id=p.id)
            FROM playlists p JOIN playlist_sync_keys k ON k.playlist_id=p.id JOIN sources s ON s.id=p.source_id
            WHERE s.provider='local' ORDER BY p.name COLLATE NOCASE,p.id LIMIT 2049")?;
        let rows = statement.query_map([], |r| {
            Ok(ExportPlaylist {
                key: r.get(0)?,
                name: r.get(1)?,
                tracks: r.get::<_, i64>(2)? as usize,
            })
        })?;
        rows.collect()
    }
    /// I reject a missing member instead of turning it into an implicit deletion on the device.
    pub fn export_playlist(&self, key: &str) -> rusqlite::Result<(String, Vec<i64>)> {
        let (id, name): (i64, String) = self.db.query_row(
            "SELECT p.id,p.name FROM playlists p
            JOIN playlist_sync_keys k ON k.playlist_id=p.id JOIN sources s ON s.id=p.source_id
            WHERE k.key=?1 AND s.provider='local'",
            [key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let entries = self.playlist_entries(id)?;
        if entries.len() > 50_000 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let mut tracks = Vec::with_capacity(entries.len());
        for entry in entries {
            let track = if let Some(id) = entry.track_id {
                Some(id)
            } else if let Some(source) = entry.source_id {
                self.db
                    .query_row(
                        "SELECT id FROM tracks WHERE source_id=?1 AND external_id=?2",
                        params![source, entry.external_id],
                        |r| r.get(0),
                    )
                    .optional()?
            } else {
                None
            };
            tracks.push(track.ok_or(rusqlite::Error::QueryReturnedNoRows)?);
        }
        Ok((name, tracks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playlist_keys_survive_renaming_and_do_not_follow_recycled_rows() {
        let library = Library::in_memory().unwrap();
        let source = library.source_id("local", "").unwrap();
        let id = library.create_playlist(source, "First", "").unwrap();
        let key = library.export_playlists().unwrap()[0].key.clone();
        library.rename_playlist(id, "Renamed").unwrap();
        assert_eq!(library.export_playlist(&key).unwrap().0, "Renamed");
        library.delete_playlist(id).unwrap();
        library.create_playlist(source, "First", "").unwrap();
        assert_ne!(library.export_playlists().unwrap()[0].key, key);
        assert!(library.export_playlist(&key).is_err());
    }
    #[test]
    fn export_is_read_only_and_keeps_occurrences_and_missing_members() {
        let library = Library::in_memory().unwrap();
        let source = library.source_id("local", "").unwrap();
        library.db.execute("INSERT INTO tracks(source_id,external_id,title,added_at) VALUES (?1,'/music/a.flac','A',1)",[source]).unwrap();
        let track = library.db.last_insert_rowid();
        let id = library.create_playlist(source, "List", "").unwrap();
        library
            .set_playlist_entries(
                id,
                &vec![
                    super::super::PlaylistEntry {
                        track_id: Some(track),
                        source_id: Some(source),
                        external_id: "/music/a.flac".into()
                    };
                    2
                ],
            )
            .unwrap();
        let before = library.db.total_changes();
        let key = library.export_playlists().unwrap()[0].key.clone();
        assert_eq!(library.export_playlist(&key).unwrap().1, vec![track, track]);
        let rows = library.export_tracks(&[track, track]).unwrap();
        assert_eq!(
            library.export_track_ids(&[rows[0].key.clone()]).unwrap(),
            vec![track]
        );
        assert_eq!(library.db.total_changes(), before);
        library.db.execute("UPDATE playlist_tracks SET track_id=NULL,source_id=NULL WHERE playlist_id=?1 AND position=1",[id]).unwrap();
        assert!(library.export_playlist(&key).is_err());
    }
}
