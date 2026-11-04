//! A music server's own playlists in the library (ADR-017, as amended for
//! them).
//!
//! The server owns each one. What is here is what it said at the last
//! reading, written whole or not at all, and an entry is here only when its
//! track is already in the library: a playlist is a view of the library,
//! never a way into it. Nothing in this file asks the server anything.

use super::{now_unix, Library, PlaylistEntry};
use rusqlite::{params, OptionalExtension};

/// A playlist as a server has it, already in the library's words.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerPlaylist {
    /// Its id on its server.
    pub external_id: String,
    pub name: String,
    pub description: String,
    /// The account may change it on the server.
    pub writable: bool,
    /// The `external_id` of each of its tracks (`ServerTrack::external_id`),
    /// in the server's order, repeats included. What is not a track is not
    /// here.
    pub entries: Vec<String>,
}

/// What writing one of a server's playlists did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MirroredPlaylist {
    /// Its id in the library.
    pub id: i64,
    /// Its entries whose track the library does not hold, left out.
    pub left_out: usize,
    /// Which of the entries it was given are in it, by their place among
    /// them, in order: the playlist's row `n` is the entry at `kept[n]`.
    pub kept: Vec<usize>,
    /// Its entries here changed: the server had another list than the one
    /// the library showed.
    pub changed: bool,
}

impl Library {
    /// Writes one of a server's playlists as the server has it: its name,
    /// what it says of itself, whether the account may change it, and the
    /// entries whose track the library holds, in the server's order and
    /// with its repeats. Its folder, its place in the sidebar and its plays
    /// here are kept. What did not change is not written, so reading the
    /// same playlist again moves it nowhere in "Recent". One transaction.
    ///
    /// What it returns says which entries are rows here, and whether the
    /// rows changed: a change asked of the server by row is made only when
    /// the rows are still the server's.
    pub fn mirror_server_playlist(&self, source_id: i64, playlist: &ServerPlaylist) -> rusqlite::Result<MirroredPlaylist> {
        let tx = self.own_transaction()?;
        let mut entries = Vec::with_capacity(playlist.entries.len());
        let mut kept = Vec::with_capacity(playlist.entries.len());
        let mut left_out = 0;
        {
            let mut find = tx.prepare_cached("SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2 AND present = 1")?;
            for (place, entry) in playlist.entries.iter().enumerate() {
                match find.query_row(params![source_id, entry], |r| r.get::<_, i64>(0)).optional()? {
                    Some(track) => {
                        entries.push(PlaylistEntry { track_id: Some(track), source_id: Some(source_id), external_id: entry.clone() });
                        kept.push(place);
                    }
                    None => left_out += 1,
                }
            }
        }
        let known: Option<(i64, String, String, i64, i64)> = tx
            .query_row("SELECT id, name, description, writable, left_out FROM playlists WHERE source_id = ?1 AND external_id = ?2", params![source_id, playlist.external_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .optional()?;
        let id = match known {
            Some((id, name, description, writable, out)) => {
                if name != playlist.name || description != playlist.description {
                    tx.execute("UPDATE playlists SET name = ?2, description = ?3, updated_at = ?4 WHERE id = ?1", params![id, playlist.name, playlist.description, now_unix()])?;
                }
                if (writable != 0) != playlist.writable || out != left_out as i64 {
                    tx.execute("UPDATE playlists SET writable = ?2, left_out = ?3 WHERE id = ?1", params![id, playlist.writable, left_out as i64])?;
                }
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO playlists
                            (source_id, external_id, name, description, created_at, updated_at,
                             custom_position, writable, left_out)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5,
                             COALESCE((SELECT max(custom_position) + 1 FROM playlists), 0), ?6, ?7)",
                    params![source_id, playlist.external_id, playlist.name, playlist.description, now_unix(), playlist.writable, left_out as i64],
                )?;
                tx.last_insert_rowid()
            }
        };
        let changed = self.playlist_entries(id)? != entries;
        if changed {
            self.set_playlist_entries(id, &entries)?;
        }
        tx.finish()?;
        Ok(MirroredPlaylist { id, left_out, kept, changed })
    }

    /// Takes out the server's playlists it no longer lists. `listed` is
    /// every id of the server's whole list: a list that was cut short never
    /// comes here. The tracks stay; a playlist is a view. Returns how many
    /// left.
    pub fn prune_server_playlists(&self, source_id: i64, listed: &[String]) -> rusqlite::Result<usize> {
        let tx = self.own_transaction()?;
        let stored: Vec<(i64, Option<String>)> = {
            let mut stmt = tx.prepare("SELECT id, external_id FROM playlists WHERE source_id = ?1")?;
            let rows = stmt.query_map(params![source_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut gone = 0;
        for (id, external_id) in stored {
            if !external_id.is_some_and(|external_id| listed.contains(&external_id)) {
                self.delete_playlist(id)?;
                gone += 1;
            }
        }
        tx.finish()?;
        Ok(gone)
    }

    /// How many entries of the server's playlists name a track the library
    /// does not hold, at their last reading.
    pub fn server_playlists_left_out(&self, source_id: i64) -> rusqlite::Result<usize> {
        self.db.query_row("SELECT COALESCE(sum(left_out), 0) FROM playlists WHERE source_id = ?1", params![source_id], |r| r.get::<_, i64>(0)).map(|n| n as usize)
    }
}

#[cfg(test)]
mod tests;
