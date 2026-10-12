//! Saved playlists: the hand-made ones and the ones a source brings in.
//!
//! An entry stores the path or the external id as well as the track, so a
//! playlist does not break because something is not in the library yet.

use crate::store::now_unix;
use crate::store::Library;
use rusqlite::{params, OptionalExtension};

/// A playlist, either local or brought in by a source.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredPlaylist {
    pub id: i64,
    pub source_id: i64,
    /// Where it comes from: "local", or the provider that brought it.
    pub provider: String,
    /// Which account or server instance owns it.
    pub instance: String,
    /// The source's display name, when it supplied one.
    pub source_label: String,
    /// The id its source gives it, empty when it is local.
    pub external_id: String,
    pub name: String,
    pub description: String,
    pub artwork_path: Option<String>,
    pub tags: Vec<String>,
    pub track_count: usize,
    pub play_count: u64,
    pub folder_id: Option<i64>,
    pub custom_position: i64,
    pub visible_sidebar: bool,
    pub writable: bool,
    pub updated_at: i64,
}

/// A folder that organises local and source-owned playlists.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlaylistFolder {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub position: i64,
    pub playlist_count: usize,
}

/// Editable metadata of a local playlist.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PlaylistEdit {
    pub name: String,
    pub description: String,
    pub artwork_path: Option<String>,
    pub tags: Vec<String>,
    pub folder_id: Option<i64>,
}

/// How a payload overlaps one playlist.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PlaylistMembership {
    pub exact: usize,
    pub similar: usize,
}

/// Metadata cached from one source playlist track.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PlaylistTrackMetadata {
    pub external_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub sample_rate_hz: Option<u32>,
    pub bit_depth: Option<u16>,
    pub channels: Option<u16>,
    pub codec: Option<String>,
    pub isrc: Option<String>,
    pub recording_mbid: Option<String>,
    pub artwork_path: Option<String>,
}

/// One entry of a playlist.
///
/// It stores the path or the external id as well as the track: a playlist
/// does not break because something is not in the library yet.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlaylistEntry {
    pub track_id: Option<i64>,
    /// The source owns the external id. Always present on new entries.
    pub source_id: Option<i64>,
    pub external_id: String,
}

fn split_tags(tags: &str) -> Vec<String> {
    tags.split('\u{1f}').filter(|tag| !tag.is_empty()).map(str::to_string).collect()
}

fn set_tags(db: &rusqlite::Connection, playlist_id: i64, tags: &[String]) -> rusqlite::Result<()> {
    db.execute("DELETE FROM playlist_tags WHERE playlist_id = ?1", params![playlist_id])?;
    let mut insert = db.prepare("INSERT OR IGNORE INTO playlist_tags (playlist_id, tag) VALUES (?1, ?2)")?;
    for tag in tags.iter().map(|tag| tag.trim()).filter(|tag| !tag.is_empty()) {
        insert.execute(params![playlist_id, tag])?;
    }
    Ok(())
}

fn text_key(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()).collect()
}

/// The recording title without edition qualifiers. A live or remastered
/// version is close enough to warn about, but not close enough to block.
fn title_key(title: &str) -> String {
    let lower = title.to_lowercase();
    let bracket = lower.find(['(', '[']).unwrap_or(lower.len());
    let mut base = lower[..bracket].trim();
    for qualifier in [" - live", " – live", " — live", " - remaster", " – remaster", " — remaster"] {
        if let Some(at) = base.find(qualifier) {
            base = base[..at].trim();
        }
    }
    text_key(base)
}

fn cache_tracks(tx: &rusqlite::Connection, source_id: i64, tracks: &[PlaylistTrackMetadata]) -> rusqlite::Result<Vec<PlaylistEntry>> {
        let mut entries = Vec::with_capacity(tracks.len());
        for track in tracks {
            let album_id: Option<i64> = if track.album.trim().is_empty() {
                None
            } else {
                tx.execute(
                    "INSERT OR IGNORE INTO albums
                            (title, album_artist, artwork_path)
                     VALUES (?1, ?2, ?3)",
                    params![track.album, track.artist, track.artwork_path],
                )?;
                tx.query_row(
                    "SELECT id FROM albums
                      WHERE title = ?1 AND album_artist = ?2
                        AND ifnull(mbid, '') = '' AND folder = ''",
                    params![track.album, track.artist],
                    |r| r.get(0),
                )
                .optional()?
            };
            tx.execute(
                "INSERT INTO tracks
                        (source_id, external_id, title, artist, album_id,
                         duration_ms, sample_rate_hz, bit_depth, channels,
                         codec, isrc, recording_mbid, present, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                         ?12, 2, ?13)
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                    title = excluded.title,
                    artist = excluded.artist,
                    album_id = excluded.album_id,
                    duration_ms = excluded.duration_ms,
                    sample_rate_hz = excluded.sample_rate_hz,
                    bit_depth = excluded.bit_depth,
                    channels = excluded.channels,
                    codec = excluded.codec,
                    isrc = excluded.isrc,
                    recording_mbid = excluded.recording_mbid,
                    present = 2",
                params![source_id, track.external_id, track.title, track.artist, album_id, track.duration_ms as i64, track.sample_rate_hz, track.bit_depth, track.channels, track.codec, track.isrc, track.recording_mbid, now_unix()],
            )?;
            let track_id = tx.query_row("SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2", params![source_id, track.external_id], |r| r.get(0))?;
            entries.push(PlaylistEntry { track_id: Some(track_id), source_id: Some(source_id), external_id: track.external_id.clone() });
        }
    Ok(entries)
}

impl Library {
    /// Keeps source metadata for mixed playlists without importing it into the library.
    pub fn cache_playlist_tracks(&self, source_id: i64, tracks: &[PlaylistTrackMetadata]) -> rusqlite::Result<Vec<PlaylistEntry>> {
        let tx = self.own_transaction()?;
        let entries = cache_tracks(&tx, source_id, tracks)?;
        tx.finish()?;
        Ok(entries)
    }

    pub fn set_folder_order(&self, ids: &[i64]) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        for (position, id) in ids.iter().enumerate() {
            tx.execute("UPDATE playlist_folders SET position = ?2 WHERE id = ?1", params![id, position as i64])?;
        }
        tx.finish()
    }

    /// Creates a playlist, or returns the one that already has that external id.
    ///
    /// An empty `external_id` means the playlist is local: two hand-made lists
    /// with the same name are two different lists, as in any player.
    pub fn create_playlist(&self, source_id: i64, name: &str, external_id: &str) -> rusqlite::Result<i64> {
        let now = now_unix();
        if !external_id.is_empty() {
            if let Some(id) = self.db.query_row("SELECT id FROM playlists WHERE source_id = ?1 AND external_id = ?2", params![source_id, external_id], |r| r.get::<_, i64>(0)).optional()? {
                self.db.execute("UPDATE playlists SET name = ?2, updated_at = ?3 WHERE id = ?1", params![id, name, now])?;
                return Ok(id);
            }
        }
        self.db.execute(
            "INSERT INTO playlists
                    (source_id, external_id, name, created_at, updated_at,
                     custom_position, writable)
             VALUES (?1, ?2, ?3, ?4, ?4,
                    COALESCE((SELECT max(custom_position) + 1 FROM playlists), 0),
                    COALESCE((SELECT provider = 'local' FROM sources WHERE id = ?1), 0))",
            params![source_id, (!external_id.is_empty()).then_some(external_id), name, now],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    /// Creates a local playlist and keeps all metadata in one transaction.
    pub fn create_playlist_with(&self, source_id: i64, edit: &PlaylistEdit) -> rusqlite::Result<i64> {
        let tx = self.own_transaction()?;
        let now = now_unix();
        tx.execute(
            "INSERT INTO playlists
                    (source_id, name, description, artwork_path, folder_id,
                     created_at, updated_at, custom_position, writable)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6,
                    COALESCE((SELECT max(custom_position) + 1 FROM playlists), 0), 1)",
            params![source_id, edit.name, edit.description, edit.artwork_path, edit.folder_id, now],
        )?;
        let id = tx.last_insert_rowid();
        set_tags(&tx, id, &edit.tags)?;
        tx.finish()?;
        Ok(id)
    }

    /// Updates the cached metadata of a source-owned playlist without
    /// replacing its entries. A missing source must never erase the cache.
    pub fn sync_source_playlist(&self, source_id: i64, external_id: &str, name: &str, description: &str, artwork_path: Option<&str>) -> rusqlite::Result<i64> {
        let id = self.create_playlist(source_id, name, external_id)?;
        self.db.execute(
            "UPDATE playlists
                SET name = ?2, description = ?3, artwork_path = ?4,
                    updated_at = ?5
              WHERE id = ?1",
            params![id, name, description, artwork_path, now_unix()],
        )?;
        Ok(id)
    }

    /// Replaces a source playlist's cached entries and refreshes the tracks
    /// they refer to. The source remains the owner of every identity.
    ///
    /// The tracks are kept as `present = 2`: a copy the playlist points at,
    /// never part of my library (ADR-017). This is a service's playlist; a
    /// server's tracks come in through its sync, as `present = 1`.
    pub fn sync_source_playlist_tracks(&self, source_id: i64, playlist_id: i64, tracks: &[PlaylistTrackMetadata]) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        let entries = cache_tracks(&tx, source_id, tracks)?;
        tx.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", params![playlist_id])?;
        for (position, entry) in entries.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_tracks
                        (playlist_id, position, track_id, external_id, source_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![playlist_id, position as i64, entry.track_id, entry.external_id, entry.source_id],
            )?;
        }
        tx.execute("UPDATE playlists SET updated_at = ?2 WHERE id = ?1", params![playlist_id, now_unix()])?;
        tx.finish()
    }

    /// Replaces a playlist's contents, in that order.
    ///
    /// Done in a transaction: a half-written playlist is worse than an old
    /// one.
    pub fn set_playlist_entries(&self, playlist_id: i64, entries: &[PlaylistEntry]) -> rusqlite::Result<()> {
        // Its own transaction unless the caller already has one: SQLite has
        // no nested BEGIN. See `Library::own_transaction`.
        let tx = self.own_transaction()?;
        tx.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", params![playlist_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO playlist_tracks
                        (playlist_id, position, track_id, external_id, source_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (pos, e) in entries.iter().enumerate() {
                stmt.execute(params![playlist_id, pos as i64, e.track_id, e.external_id, e.source_id])?;
            }
        }
        tx.execute("UPDATE playlists SET updated_at = ?2 WHERE id = ?1", params![playlist_id, now_unix()])?;
        tx.finish()
    }

    /// The playlists, with how many tracks each carries.
    pub fn playlists(&self) -> rusqlite::Result<Vec<StoredPlaylist>> {
        let mut stmt = self.db.prepare(
            "SELECT p.id, p.source_id, s.provider, s.instance, s.label,
                    COALESCE(p.external_id, ''), p.name, p.description,
                    p.artwork_path,
                    COALESCE((SELECT group_concat(tag, char(31))
                                FROM playlist_tags WHERE playlist_id = p.id), ''),
                    count(pt.position), p.play_count, p.folder_id,
                    p.custom_position, p.visible_sidebar, p.writable, p.updated_at
             FROM playlists p
             JOIN sources s ON s.id = p.source_id
             LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id
             GROUP BY p.id
             ORDER BY p.updated_at DESC, p.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredPlaylist {
                id: r.get(0)?,
                source_id: r.get(1)?,
                provider: r.get(2)?,
                instance: r.get(3)?,
                source_label: r.get(4)?,
                external_id: r.get(5)?,
                name: r.get(6)?,
                description: r.get(7)?,
                artwork_path: r.get(8)?,
                tags: split_tags(&r.get::<_, String>(9)?),
                track_count: r.get::<_, i64>(10)? as usize,
                play_count: r.get::<_, i64>(11)? as u64,
                folder_id: r.get(12)?,
                custom_position: r.get(13)?,
                visible_sidebar: r.get::<_, i64>(14)? != 0,
                writable: r.get::<_, i64>(15)? != 0,
                updated_at: r.get(16)?,
            })
        })?;
        rows.collect()
    }

    /// What a playlist carries, in its order.
    pub fn playlist_entries(&self, playlist_id: i64) -> rusqlite::Result<Vec<PlaylistEntry>> {
        let mut stmt = self.db.prepare(
            "SELECT track_id, source_id, external_id FROM playlist_tracks
             WHERE playlist_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map(params![playlist_id], |r| Ok(PlaylistEntry { track_id: r.get(0)?, source_id: r.get(1)?, external_id: r.get(2)? }))?;
        rows.collect()
    }

    /// Stable source references for those library tracks, in input order.
    pub fn playlist_entries_for_tracks(&self, track_ids: &[i64]) -> rusqlite::Result<Vec<PlaylistEntry>> {
        let mut find = self.db.prepare("SELECT id, source_id, external_id FROM tracks WHERE id = ?1")?;
        let mut entries = Vec::with_capacity(track_ids.len());
        for id in track_ids {
            if let Some(entry) = find.query_row(params![id], |r| Ok(PlaylistEntry { track_id: r.get(0)?, source_id: r.get(1)?, external_id: r.get(2)? })).optional()? {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    /// Adds to the end of a playlist what it does not already carry.
    /// Returns how many went in.
    pub fn append_to_playlist(&self, playlist_id: i64, entries: &[PlaylistEntry]) -> rusqlite::Result<usize> {
        self.append_entries(playlist_id, entries, false)
    }

    /// Adds every one of them to the end, also what it already carries:
    /// somebody asked for the repeats.
    pub fn append_again_to_playlist(&self, playlist_id: i64, entries: &[PlaylistEntry]) -> rusqlite::Result<usize> {
        self.append_entries(playlist_id, entries, true)
    }

    fn append_entries(&self, playlist_id: i64, entries: &[PlaylistEntry], repeats: bool) -> rusqlite::Result<usize> {
        let (owner, local): (i64, bool) = self.db.query_row(
            "SELECT p.source_id, s.provider = 'local'
               FROM playlists p JOIN sources s ON s.id = p.source_id
              WHERE p.id = ?1",
            params![playlist_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if !local && entries.iter().any(|entry| entry.source_id != Some(owner)) {
            return Ok(0);
        }
        let mut all = self.playlist_entries(playlist_id)?;
        let before = all.len();
        for e in entries {
            if repeats || !all.iter().any(|x| x.source_id == e.source_id && x.external_id == e.external_id) {
                all.push(e.clone());
            }
        }
        let added = all.len() - before;
        if added > 0 {
            self.set_playlist_entries(playlist_id, &all)?;
        }
        Ok(added)
    }

    /// Takes out what is at that position; what comes after moves up.
    pub fn remove_from_playlist(&self, playlist_id: i64, position: usize) -> rusqlite::Result<bool> {
        let mut all = self.playlist_entries(playlist_id)?;
        if position >= all.len() {
            return Ok(false);
        }
        all.remove(position);
        self.set_playlist_entries(playlist_id, &all)?;
        Ok(true)
    }

    /// Removes every occurrence of the selected library tracks.
    pub fn remove_tracks_from_playlist(&self, playlist_id: i64, track_ids: &[i64]) -> rusqlite::Result<usize> {
        let mut all = self.playlist_entries(playlist_id)?;
        let before = all.len();
        all.retain(|entry| entry.track_id.is_none_or(|id| !track_ids.contains(&id)));
        let removed = before - all.len();
        if removed > 0 {
            self.set_playlist_entries(playlist_id, &all)?;
        }
        Ok(removed)
    }

    pub fn set_track_favorites(&self, track_ids: &[i64], favorite: bool) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        for id in track_ids {
            if favorite {
                tx.execute("INSERT OR REPLACE INTO track_favorites (track_id, added_at) VALUES (?1, ?2)", params![id, now_unix()])?;
            } else {
                tx.execute("DELETE FROM track_favorites WHERE track_id = ?1", params![id])?;
            }
        }
        tx.finish()
    }

    /// Moves the track at `from` to `to`; what is between shifts by one.
    pub fn move_in_playlist(&self, playlist_id: i64, from: usize, to: usize) -> rusqlite::Result<bool> {
        let mut all = self.playlist_entries(playlist_id)?;
        if from >= all.len() || to >= all.len() || from == to {
            return Ok(false);
        }
        let entry = all.remove(from);
        all.insert(to, entry);
        self.set_playlist_entries(playlist_id, &all)?;
        Ok(true)
    }

    /// The covers of the first records a playlist holds, one per record, up
    /// to `n`: what its mosaic shows.
    pub fn playlist_covers(&self, playlist_id: i64, n: usize) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.db.prepare(
            concat!("SELECT ", album_art!("a"), " FROM playlist_tracks pt
             JOIN tracks t ON t.id = pt.track_id
             JOIN albums a ON a.id = t.album_id
             WHERE pt.playlist_id = ?1 AND ", album_art!("a"), " IS NOT NULL
             GROUP BY a.id ORDER BY min(pt.position) LIMIT ?2"),
        )?;
        let rows = stmt.query_map(params![playlist_id, n as i64], |r| r.get(0))?;
        rows.collect()
    }

    /// How the payload overlaps one playlist. Exact identity blocks nothing;
    /// similar metadata is advisory and lets the caller warn before adding.
    pub fn playlist_membership(&self, playlist_id: i64, entries: &[PlaylistEntry]) -> rusqlite::Result<PlaylistMembership> {
        let there = self.playlist_entries(playlist_id)?;
        let exact = entries.iter().filter(|entry| there.iter().any(|candidate| entry.source_id == candidate.source_id && entry.external_id == candidate.external_id)).count();
        let mut there_signatures = Vec::new();
        for entry in &there {
            if let Some(signature) = self.entry_signature(entry)? {
                there_signatures.push(signature);
            }
        }
        let mut payload_signatures = Vec::new();
        for entry in entries {
            let is_exact = there.iter().any(|candidate| entry.source_id == candidate.source_id && entry.external_id == candidate.external_id);
            if !is_exact {
                if let Some(signature) = self.entry_signature(entry)? {
                    payload_signatures.push(signature);
                }
            }
        }
        let similar = payload_signatures.iter().filter(|signature| there_signatures.contains(signature)).count();
        Ok(PlaylistMembership { exact, similar })
    }

    fn entry_signature(&self, entry: &PlaylistEntry) -> rusqlite::Result<Option<(String, String)>> {
        let Some(track_id) = entry.track_id else {
            return Ok(None);
        };
        self.db
            .query_row("SELECT title, artist FROM tracks WHERE id = ?1", params![track_id], |r| {
                let title: String = r.get(0)?;
                let artist: String = r.get(1)?;
                Ok((title_key(&title), text_key(&artist)))
            })
            .optional()
    }

    /// All playlist folders in their custom order.
    pub fn playlist_folders(&self) -> rusqlite::Result<Vec<PlaylistFolder>> {
        let mut stmt = self.db.prepare(
            "SELECT f.id, f.name, f.color, f.position, count(p.id)
               FROM playlist_folders f
               LEFT JOIN playlists p ON p.folder_id = f.id
              GROUP BY f.id
              ORDER BY f.position, f.name COLLATE NOCASE",
        )?;
        let folders = stmt.query_map([], |r| Ok(PlaylistFolder { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, position: r.get(3)?, playlist_count: r.get::<_, i64>(4)? as usize }))?.collect();
        folders
    }

    pub fn create_playlist_folder(&self, name: &str, color: &str) -> rusqlite::Result<i64> {
        let now = now_unix();
        self.db.execute(
            "INSERT INTO playlist_folders (name, color, position, created_at, updated_at)
             VALUES (?1, ?2,
                     COALESCE((SELECT max(position) + 1 FROM playlist_folders), 0),
                     ?3, ?3)",
            params![name, color, now],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn update_playlist_folder(&self, folder_id: i64, name: &str, color: &str) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE playlist_folders
                SET name = ?2, color = ?3, updated_at = ?4
              WHERE id = ?1",
            params![folder_id, name, color, now_unix()],
        )?;
        Ok(())
    }

    pub fn delete_playlist_folder(&self, folder_id: i64) -> rusqlite::Result<()> {
        self.db.execute("UPDATE playlists SET folder_id = NULL WHERE folder_id = ?1", params![folder_id])?;
        self.db.execute("DELETE FROM playlist_folders WHERE id = ?1", params![folder_id])?;
        Ok(())
    }

    pub fn move_playlist_to_folder(&self, playlist_id: i64, folder_id: Option<i64>) -> rusqlite::Result<()> {
        self.db.execute("UPDATE playlists SET folder_id = ?2, updated_at = ?3 WHERE id = ?1", params![playlist_id, folder_id, now_unix()])?;
        Ok(())
    }

    pub fn set_playlist_visible(&self, playlist_id: i64, visible: bool) -> rusqlite::Result<()> {
        self.db.execute("UPDATE playlists SET visible_sidebar = ?2 WHERE id = ?1", params![playlist_id, visible])?;
        Ok(())
    }

    pub fn set_playlist_custom_order(&self, ids: &[i64]) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        for (position, id) in ids.iter().enumerate() {
            tx.execute("UPDATE playlists SET custom_position = ?2 WHERE id = ?1", params![id, position as i64])?;
        }
        tx.finish()
    }

    pub fn record_playlist_play(&self, playlist_id: i64) -> rusqlite::Result<()> {
        self.db.execute("UPDATE playlists SET play_count = play_count + 1 WHERE id = ?1", params![playlist_id])?;
        Ok(())
    }

    /// Renames a playlist.
    pub fn rename_playlist(&self, playlist_id: i64, name: &str) -> rusqlite::Result<()> {
        self.db.execute("UPDATE playlists SET name = ?2, updated_at = ?3 WHERE id = ?1", params![playlist_id, name, now_unix()])?;
        Ok(())
    }

    /// Changes all editable metadata of a local playlist.
    pub fn update_playlist(&self, playlist_id: i64, edit: &PlaylistEdit) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute(
            "UPDATE playlists
                SET name = ?2, description = ?3, artwork_path = ?4,
                    folder_id = ?5, updated_at = ?6
              WHERE id = ?1",
            params![playlist_id, edit.name, edit.description, edit.artwork_path, edit.folder_id, now_unix()],
        )?;
        set_tags(&tx, playlist_id, &edit.tags)?;
        tx.finish()
    }

    /// Deletes a playlist. The tracks are untouched: a playlist is a view.
    pub fn delete_playlist(&self, playlist_id: i64) -> rusqlite::Result<()> {
        self.db.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", params![playlist_id])?;
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
            PlaylistEntry { track_id: Some(2), source_id: Some(source), external_id: "/Aqualung.flac".into() },
            // Something the playlist names that has not been scanned yet.
            PlaylistEntry { track_id: None, source_id: Some(source), external_id: "/pending.flac".into() },
            PlaylistEntry { track_id: Some(1), source_id: Some(source), external_id: "/Bayou.flac".into() },
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
    fn adding_skips_what_is_there_and_removing_closes_the_gap() {
        let lib = sample_library();
        let source = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(source, "Mix", "").unwrap();
        let a = PlaylistEntry { track_id: Some(1), source_id: Some(source), external_id: "/Bayou.flac".into() };
        let b = PlaylistEntry { track_id: Some(2), source_id: Some(source), external_id: "/Aqualung.flac".into() };
        assert_eq!(lib.append_to_playlist(id, &[a.clone(), b.clone()]).unwrap(), 2);
        assert_eq!(lib.append_to_playlist(id, &[b.clone()]).unwrap(), 0, "already in it");
        assert!(lib.remove_from_playlist(id, 0).unwrap());
        assert_eq!(lib.playlist_entries(id).unwrap(), [b]);
        assert!(!lib.remove_from_playlist(id, 5).unwrap(), "nothing there");
    }

    #[test]
    fn a_track_moves_and_the_rest_make_room() {
        let lib = sample_library();
        let source = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(source, "Mix", "").unwrap();
        let entries: Vec<PlaylistEntry> = ["/a.flac", "/b.flac", "/c.flac"].iter().map(|p| PlaylistEntry { track_id: None, source_id: Some(source), external_id: p.to_string() }).collect();
        lib.set_playlist_entries(id, &entries).unwrap();
        assert!(lib.move_in_playlist(id, 2, 0).unwrap());
        let order: Vec<String> = lib.playlist_entries(id).unwrap().into_iter().map(|e| e.external_id).collect();
        assert_eq!(order, ["/c.flac", "/a.flac", "/b.flac"]);
        assert!(!lib.move_in_playlist(id, 0, 3).unwrap(), "past the end");
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
        let three: Vec<PlaylistEntry> = (0..3).map(|i| PlaylistEntry { track_id: None, source_id: Some(source), external_id: format!("/{i}.flac") }).collect();
        lib.set_playlist_entries(id, &three).unwrap();
        lib.set_playlist_entries(id, &three[..1]).unwrap();
        assert_eq!(lib.playlist_entries(id).unwrap().len(), 1, "the old ones did not stay");
    }

    #[test]
    fn deleting_a_playlist_does_not_touch_the_tracks() {
        let lib = sample_library();
        let source = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(source, "Temporal", "").unwrap();
        lib.set_playlist_entries(id, &[PlaylistEntry { track_id: Some(1), source_id: Some(source), external_id: "/Bayou.flac".into() }]).unwrap();
        lib.delete_playlist(id).unwrap();
        assert!(lib.playlists().unwrap().is_empty());
        assert_eq!(lib.track_count(false).unwrap(), 3, "the tracks are still there");
    }

    #[test]
    fn only_local_playlists_accept_tracks_from_several_sources() {
        let lib = sample_library();
        let local = lib.source_id("local", "default").unwrap();
        let remote = lib.source_id("server", "living-room").unwrap();
        let local_list = lib.create_playlist(local, "Everywhere", "").unwrap();
        let remote_list = lib.create_playlist(remote, "Upstream", "remote-1").unwrap();
        let local_track = PlaylistEntry { track_id: Some(1), source_id: Some(local), external_id: "/Bayou.flac".into() };
        let remote_track = PlaylistEntry { track_id: None, source_id: Some(remote), external_id: "track-9".into() };

        assert_eq!(lib.append_to_playlist(local_list, &[local_track.clone(), remote_track.clone()]).unwrap(), 2, "a local playlist is the mixed-source container");
        assert_eq!(lib.append_to_playlist(remote_list, &[local_track]).unwrap(), 0, "a source-owned playlist rejects foreign identities");
        assert_eq!(lib.append_to_playlist(remote_list, &[remote_track]).unwrap(), 1);
    }

    #[test]
    fn membership_distinguishes_exact_identity_from_similar_editions() {
        let lib = sample_library();
        let local = lib.source_id("local", "default").unwrap();
        lib.db.execute("UPDATE tracks SET title = 'Track', artist = 'Band' WHERE id = 1", []).unwrap();
        lib.db.execute("UPDATE tracks SET title = 'Track (Live)', artist = 'Band' WHERE id = 2", []).unwrap();
        let id = lib.create_playlist(local, "Versions", "").unwrap();
        let original = PlaylistEntry { track_id: Some(1), source_id: Some(local), external_id: "/Bayou.flac".into() };
        let live = PlaylistEntry { track_id: Some(2), source_id: Some(local), external_id: "/Aqualung.flac".into() };
        lib.append_to_playlist(id, std::slice::from_ref(&original)).unwrap();

        assert_eq!(lib.playlist_membership(id, &[original, live]).unwrap(), PlaylistMembership { exact: 1, similar: 1 });
    }

    #[test]
    fn editable_metadata_and_folder_survive_round_trip() {
        let lib = Library::in_memory().unwrap();
        let local = lib.source_id("local", "default").unwrap();
        let folder = lib.create_playlist_folder("Road", "amber").unwrap();
        let id = lib.create_playlist_with(local, &PlaylistEdit { name: "Night drive".into(), description: "After dark".into(), artwork_path: Some("/cover.jpg".into()), tags: vec!["driving".into(), "night".into()], folder_id: Some(folder) }).unwrap();
        lib.record_playlist_play(id).unwrap();

        let playlist = lib.playlists().unwrap().pop().unwrap();
        assert_eq!(playlist.folder_id, Some(folder));
        assert_eq!(playlist.description, "After dark");
        assert_eq!(playlist.artwork_path.as_deref(), Some("/cover.jpg"));
        assert_eq!(playlist.tags, ["driving", "night"]);
        assert_eq!(playlist.play_count, 1);
        assert_eq!(lib.playlist_folders().unwrap()[0].playlist_count, 1);
    }
}
