//! Servers of mine: their place in the library (ADR-017).
//!
//! A server is a source like the local one, and what it holds is mirrored
//! into the same tables. Every view, filter, version and statistic works on
//! it without knowing where a track came from. What is here is what only a
//! server has: how it is reached, which sync saw what, and how all of it
//! goes away when the server is removed.
//!
//! Nothing in this file speaks a protocol. The words are the library's; the
//! crate that talks to each kind of server translates into them.

use super::{now_unix, Library, PlaylistEntry};
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

/// A server being added.
#[derive(Clone, Debug, Default)]
pub struct NewServer {
    pub provider: String,
    /// Opaque, minted when it is added. Never its address: that changes.
    pub instance: String,
    pub label: String,
    pub icon: String,
    pub address: String,
    pub account: String,
    /// The server's own id, or what stands for it. Empty when it has none.
    pub identity: String,
    pub product: String,
    /// JSON.
    pub libraries: String,
    /// JSON.
    pub details: String,
}

/// A server as the library keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredServer {
    pub source_id: i64,
    pub provider: String,
    pub instance: String,
    pub label: String,
    pub reachable: bool,
    pub address: String,
    pub account: String,
    pub identity: String,
    pub product: String,
    pub libraries: String,
    pub details: String,
    pub added_at: i64,
    pub synced_at: Option<i64>,
    /// In the library now.
    pub tracks: usize,
    pub albums: usize,
}

/// A track as a server describes it, already in the library's words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerTrack {
    /// The track's id on its server.
    pub external_id: String,
    pub title: String,
    /// The performers, as one line.
    pub artist: String,
    /// The artists the server gives apart, when `artist` joins several.
    pub artists: Vec<String>,
    pub album: String,
    pub album_artist: String,
    /// The record's id on its server. Tracks that share it are one record.
    /// Empty when the server does not say; the title and the album artist
    /// stand for it then.
    pub album_key: String,
    /// How the server names the record's cover, to fetch it later.
    pub cover: Option<String>,
    pub disc_number: Option<u16>,
    pub disc_total: Option<u16>,
    pub track_number: Option<u16>,
    pub year: Option<u32>,
    pub genres: Vec<String>,
    pub duration_ms: u64,
    pub codec: Option<String>,
    pub bit_depth: Option<u8>,
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u16>,
    pub isrc: Option<String>,
    pub recording_mbid: Option<String>,
    /// What changes when the server's copy changes. A track whose revision
    /// is the one stored does not need to be asked about again.
    pub revision: Option<String>,
    /// When it arrived on the server, in unix seconds.
    pub added_at: Option<i64>,
}

/// What removing a server takes out of the library, counted before doing it
/// so the window can say it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Removal {
    pub tracks: usize,
    pub albums: usize,
    /// Tracks and records that carry my stars.
    pub ratings: usize,
    /// Listens that counted as plays.
    pub plays: usize,
    /// The server's own playlists.
    pub playlists: usize,
    /// Its tracks in my playlists, and in how many of them.
    pub playlist_entries: usize,
    pub playlists_touched: usize,
}

const TRACKS_OF: &str = "SELECT id FROM tracks WHERE source_id = ?1";

impl Library {
    /// Adds a server as a source of the library and returns its id.
    pub fn add_server(&self, new: &NewServer) -> rusqlite::Result<i64> {
        let tx = self.own_transaction()?;
        tx.execute(
            "INSERT INTO sources (provider, instance, label, icon, kind) VALUES (?1, ?2, ?3, ?4, 'server')",
            params![new.provider, new.instance, new.label, new.icon],
        )?;
        let source = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO servers (source_id, address, account, identity, product, libraries, details, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![source, new.address, new.account, new.identity, new.product, new.libraries, new.details, now_unix()],
        )?;
        tx.finish()?;
        Ok(source)
    }

    /// The servers, in the order they were added.
    pub fn servers(&self) -> rusqlite::Result<Vec<StoredServer>> {
        self.read_servers("ORDER BY v.added_at, v.source_id", params![])
    }

    pub fn server(&self, source_id: i64) -> rusqlite::Result<Option<StoredServer>> {
        Ok(self.read_servers("WHERE v.source_id = ?1", params![source_id])?.pop())
    }

    fn read_servers(&self, tail: &str, args: &[&dyn rusqlite::ToSql]) -> rusqlite::Result<Vec<StoredServer>> {
        let sql = format!(
            "SELECT v.source_id, s.provider, s.instance, s.label, s.reachable, v.address, v.account,
                    v.identity, v.product, v.libraries, v.details, v.added_at, v.synced_at,
                    (SELECT count(*) FROM tracks t WHERE t.source_id = v.source_id AND t.present = 1),
                    (SELECT count(DISTINCT t.album_id) FROM tracks t
                      WHERE t.source_id = v.source_id AND t.present = 1 AND t.album_id IS NOT NULL)
             FROM servers v JOIN sources s ON s.id = v.source_id {tail}"
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(args, |r| {
            Ok(StoredServer {
                source_id: r.get(0)?,
                provider: r.get(1)?,
                instance: r.get(2)?,
                label: r.get(3)?,
                reachable: r.get::<_, i64>(4)? != 0,
                address: r.get(5)?,
                account: r.get(6)?,
                identity: r.get(7)?,
                product: r.get(8)?,
                libraries: r.get(9)?,
                details: r.get(10)?,
                added_at: r.get(11)?,
                synced_at: r.get(12)?,
                tracks: r.get::<_, i64>(13)? as usize,
                albums: r.get::<_, i64>(14)? as usize,
            })
        })?;
        rows.collect()
    }

    /// The server of that kind already known by that identity, so it is not
    /// added twice.
    pub fn server_with_identity(&self, provider: &str, identity: &str) -> rusqlite::Result<Option<i64>> {
        if identity.is_empty() {
            return Ok(None);
        }
        self.db
            .query_row(
                "SELECT v.source_id FROM servers v JOIN sources s ON s.id = v.source_id
                 WHERE s.provider = ?1 AND v.identity = ?2",
                params![provider, identity],
                |r| r.get(0),
            )
            .optional()
    }

    /// Keeps what a server said about itself the last time it was asked.
    pub fn update_server(&self, source_id: i64, label: &str, address: &str, product: &str, libraries: &str, details: &str) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute("UPDATE sources SET label = ?2 WHERE id = ?1", params![source_id, label])?;
        tx.execute(
            "UPDATE servers SET address = ?2, product = ?3, libraries = ?4, details = ?5 WHERE source_id = ?1",
            params![source_id, address, product, libraries, details],
        )?;
        tx.finish()
    }

    /// Starts a sync and returns its generation. Every track the sync sees
    /// is stamped with it; when the sync finishes, what was not stamped is
    /// what the server no longer has.
    pub fn begin_server_sync(&self, source_id: i64) -> rusqlite::Result<i64> {
        self.db.execute("UPDATE servers SET generation = generation + 1 WHERE source_id = ?1", params![source_id])?;
        self.db.query_row("SELECT generation FROM servers WHERE source_id = ?1", params![source_id], |r| r.get(0))
    }

    /// Every track of the server the library holds, with its revision. What
    /// a sync compares against so it only asks again about what changed.
    pub fn server_revisions(&self, source_id: i64) -> rusqlite::Result<HashMap<String, Option<String>>> {
        let mut stmt = self.db.prepare("SELECT external_id, revision FROM tracks WHERE source_id = ?1 AND present IN (0, 1)")?;
        let rows = stmt.query_map(params![source_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// Stamps tracks the sync saw unchanged, without rewriting them.
    pub fn keep_server_tracks(&self, source_id: i64, generation: i64, external_ids: &[String]) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        {
            let mut stamp = tx.prepare(
                "UPDATE tracks SET sync_generation = ?3, present = 1, reachable = 1
                 WHERE source_id = ?1 AND external_id = ?2",
            )?;
            for id in external_ids {
                stamp.execute(params![source_id, id, generation])?;
            }
        }
        tx.finish()
    }

    /// Writes tracks as the server describes them, with their records and
    /// genres, in one transaction: a page lands whole or not at all.
    pub fn save_server_tracks(&self, source_id: i64, generation: i64, tracks: &[ServerTrack]) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        let now = now_unix();
        for track in tracks {
            if track.artists.len() > 1 {
                self.state_credit(&track.artist, &track.artists)?;
            }
            let album_id = if track.album.trim().is_empty() {
                None
            } else {
                let key = if track.album_key.is_empty() {
                    format!("{}\u{1f}{}", track.album, track.album_artist)
                } else {
                    track.album_key.clone()
                };
                let artist = if track.album_artist.trim().is_empty() { &track.artist } else { &track.album_artist };
                Some(self.source_edition(source_id, &key, &track.album, artist, track.year)?)
            };
            tx.execute(
                "INSERT INTO tracks
                   (source_id, external_id, title, artist, album_id, track_number, disc_number,
                    duration_ms, sample_rate_hz, bit_depth, channels, codec, isrc, recording_mbid,
                    present, reachable, added_at, album_credit, album_title, disc_total,
                    release_year, sync_generation, revision)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                         1, 1, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                   title = excluded.title, artist = excluded.artist, album_id = excluded.album_id,
                   track_number = excluded.track_number, disc_number = excluded.disc_number,
                   duration_ms = excluded.duration_ms, sample_rate_hz = excluded.sample_rate_hz,
                   bit_depth = excluded.bit_depth, channels = excluded.channels, codec = excluded.codec,
                   isrc = excluded.isrc, recording_mbid = excluded.recording_mbid,
                   present = 1, reachable = 1, album_credit = excluded.album_credit,
                   album_title = excluded.album_title, disc_total = excluded.disc_total,
                   release_year = excluded.release_year, sync_generation = excluded.sync_generation,
                   revision = excluded.revision",
                params![
                    source_id,
                    track.external_id,
                    track.title,
                    track.artist,
                    album_id,
                    track.track_number,
                    track.disc_number,
                    track.duration_ms as i64,
                    track.sample_rate_hz,
                    track.bit_depth,
                    track.channels,
                    track.codec,
                    track.isrc,
                    track.recording_mbid,
                    track.added_at.unwrap_or(now),
                    track.album_artist.trim(),
                    track.album,
                    track.disc_total,
                    track.year,
                    generation,
                    track.revision,
                ],
            )?;
            let track_id: i64 = tx.query_row(
                "SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2",
                params![source_id, track.external_id],
                |r| r.get(0),
            )?;
            self.set_track_genres(track_id, &track.genres)?;
            if let (Some(album), Some(cover)) = (album_id, track.cover.as_deref().filter(|c| !c.is_empty())) {
                // The first cover named for a record is its cover; a later
                // name replaces it and asks for it to be fetched again.
                tx.execute(
                    "INSERT INTO server_covers (album_id, cover) VALUES (?1, ?2)
                     ON CONFLICT(album_id) DO UPDATE SET
                        fetched = CASE WHEN cover = excluded.cover THEN fetched ELSE 0 END,
                        cover = excluded.cover",
                    params![album, cover],
                )?;
            }
        }
        tx.finish()
    }

    /// Closes a sync that saw everything: what it did not see is gone
    /// (`present = 0`, kept for my ratings and history, never listed).
    /// Returns how many went. A sync that was cut off never calls this, so
    /// a half-read server takes nothing out of the library.
    pub fn finish_server_sync(&self, source_id: i64, generation: i64) -> rusqlite::Result<usize> {
        let tx = self.own_transaction()?;
        let gone = tx.execute(
            "UPDATE tracks SET present = 0
             WHERE source_id = ?1 AND present = 1 AND (sync_generation IS NULL OR sync_generation <> ?2)",
            params![source_id, generation],
        )?;
        tx.execute("UPDATE servers SET synced_at = ?2 WHERE source_id = ?1", params![source_id, now_unix()])?;
        tx.finish()?;
        self.match_credits()?;
        Ok(gone)
    }

    /// Records of the server whose cover has not been fetched yet.
    pub fn pending_server_covers(&self, source_id: i64, limit: usize) -> rusqlite::Result<Vec<(i64, String)>> {
        let mut stmt = self.db.prepare(
            "SELECT c.album_id, c.cover FROM server_covers c
             WHERE c.fetched = 0 AND EXISTS
                (SELECT 1 FROM tracks t WHERE t.album_id = c.album_id AND t.source_id = ?1 AND t.present = 1)
             ORDER BY c.album_id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![source_id, limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// The cover of a server's record, once it is on disk. `None` is a
    /// record the server has no cover for: it is not asked about again.
    pub fn set_server_cover(&self, album_id: i64, path: Option<&str>) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        if let Some(path) = path {
            tx.execute("UPDATE albums SET artwork_path = ?2 WHERE id = ?1", params![album_id, path])?;
        }
        tx.execute("UPDATE server_covers SET fetched = 1 WHERE album_id = ?1", params![album_id])?;
        tx.finish()
    }

    /// The source with that provider and instance, without creating it.
    pub fn find_source(&self, provider: &str, instance: &str) -> rusqlite::Result<Option<i64>> {
        self.db
            .query_row("SELECT id FROM sources WHERE provider = ?1 AND instance = ?2", params![provider, instance], |r| r.get(0))
            .optional()
    }

    /// What playing a server's track needs from the library: its codec, for
    /// the decoder's hint, and its revision, which on some servers says
    /// where its bytes are.
    pub fn server_track(&self, source_id: i64, external_id: &str) -> rusqlite::Result<Option<(Option<String>, Option<String>)>> {
        self.db
            .query_row(
                "SELECT codec, revision FROM tracks WHERE source_id = ?1 AND external_id = ?2",
                params![source_id, external_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
    }

    /// The track and the source behind a queue entry of a server's track,
    /// which is its `external_id` (`chilpa_models::RemoteEntry`).
    pub fn track_by_entry(&self, entry: &str) -> rusqlite::Result<Option<(i64, i64)>> {
        self.db
            .query_row(
                "SELECT t.id, t.source_id FROM tracks t JOIN sources s ON s.id = t.source_id
                 WHERE t.external_id = ?1 AND s.kind = 'server'",
                params![entry],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
    }

    /// What the window shows for a server's track while it plays, from the
    /// library: title, artist, record, record artist and cover.
    pub fn server_track_card(&self, entry: &str) -> rusqlite::Result<Option<(String, String, String, String, Option<String>)>> {
        self.db
            .query_row(
                "SELECT t.title, t.artist, coalesce(a.title, ''), coalesce(a.album_artist, ''), a.artwork_path
                 FROM tracks t LEFT JOIN albums a ON a.id = t.album_id
                 WHERE t.external_id = ?1",
                params![entry],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
    }

    /// Which of those queue entries cannot be played now: in the library and
    /// not reachable, or gone. What the library does not know stays: a file
    /// opened from a folder, a radio.
    pub fn unplayable_entries(&self, entries: &[String]) -> rusqlite::Result<std::collections::HashSet<String>> {
        let mut out = std::collections::HashSet::new();
        for chunk in entries.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let sql = format!("SELECT external_id FROM tracks WHERE external_id IN ({marks}) AND NOT (present = 1 AND reachable = 1) AND present <> 2");
            let mut stmt = self.db.prepare(&sql)?;
            for entry in stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| r.get::<_, String>(0))? {
                out.insert(entry?);
            }
        }
        Ok(out)
    }

    /// What removing this source would take out of the library.
    pub fn removal(&self, source_id: i64) -> rusqlite::Result<Removal> {
        let count = |sql: &str| -> rusqlite::Result<usize> {
            self.db.query_row(sql, params![source_id], |r| r.get::<_, i64>(0)).map(|n| n as usize)
        };
        Ok(Removal {
            tracks: count("SELECT count(*) FROM tracks WHERE source_id = ?1 AND present = 1")?,
            albums: count(&format!("SELECT count(*) FROM ({})", ALBUMS_ONLY_OF))?,
            ratings: count(&format!(
                "SELECT (SELECT count(*) FROM track_ratings WHERE stars > 0 AND track_id IN ({TRACKS_OF}))
                      + (SELECT count(*) FROM album_ratings WHERE stars > 0 AND album_id IN ({ALBUMS_ONLY_OF}))"
            ))?,
            plays: count(&format!(
                "SELECT count(*) FROM play_events
                 WHERE (source_id = ?1 OR track_id IN ({TRACKS_OF}))
                   AND (duration_ms = 0 OR played_ms * 2 >= duration_ms)"
            ))?,
            playlists: count("SELECT count(*) FROM playlists WHERE source_id = ?1")?,
            playlist_entries: count(&format!("SELECT count(*) FROM ({ENTRIES_IN_OTHERS})"))?,
            playlists_touched: count(&format!("SELECT count(DISTINCT playlist_id) FROM ({ENTRIES_IN_OTHERS})"))?,
        })
    }

    /// Takes a source and everything that came from it out of the library:
    /// tracks, records, genres, analysis, ratings, favorites, history, its
    /// playlists, its tracks in mine, its covers and its settings. One
    /// transaction; the totals are counted again at the end.
    ///
    /// What was already sent elsewhere -- a scrobble -- is not retracted,
    /// and nothing is ever asked of the server itself.
    pub fn remove_source(&self, source_id: i64) -> rusqlite::Result<()> {
        self.remove_source_catalog(source_id, false)
    }

    /// Uninstall removes the source's catalog, retaining entries saved in other playlists.
    pub fn remove_plugin_source(&self, source_id: i64) -> rusqlite::Result<()> {
        self.remove_source_catalog(source_id, true)
    }

    fn remove_source_catalog(&self, source_id: i64, preserve_playlists: bool) -> rusqlite::Result<()> {
        let tracks_of = if preserve_playlists {
            "SELECT id FROM tracks WHERE source_id = ?1 AND id NOT IN
             (SELECT pt.track_id FROM playlist_tracks pt JOIN playlists p ON p.id = pt.playlist_id
              WHERE p.source_id <> ?1 AND pt.track_id IS NOT NULL)"
        } else { TRACKS_OF };
        let tx = self.own_transaction()?;
        let touched: Vec<i64> = {
            let mut stmt = tx.prepare(&format!("SELECT DISTINCT playlist_id FROM ({ENTRIES_IN_OTHERS})"))?;
            let rows = stmt.query_map(params![source_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let theirs: std::collections::HashSet<i64> = {
            let mut stmt = tx.prepare(tracks_of)?;
            let rows = stmt.query_map(params![source_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for playlist in touched.into_iter().filter(|_| !preserve_playlists) {
            let kept: Vec<PlaylistEntry> = self
                .playlist_entries(playlist)?
                .into_iter()
                .filter(|e| e.source_id != Some(source_id) && !e.track_id.is_some_and(|id| theirs.contains(&id)))
                .collect();
            self.set_playlist_entries(playlist, &kept)?;
        }
        let albums: Vec<i64> = {
            let mut stmt = tx.prepare(ALBUMS_ONLY_OF)?;
            let rows = stmt.query_map(params![source_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        tx.execute("DELETE FROM playlists WHERE source_id = ?1", params![source_id])?;
        tx.execute(&format!("DELETE FROM play_events WHERE source_id = ?1 OR track_id IN ({tracks_of})"), params![source_id])?;
        let genres: Vec<i64> = {
            let mut stmt = tx.prepare(&format!("SELECT DISTINCT genre_id FROM track_genres WHERE track_id IN ({tracks_of})"))?;
            let rows = stmt.query_map(params![source_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        tx.execute(&format!("DELETE FROM track_genres WHERE track_id IN ({tracks_of})"), params![source_id])?;
        tx.execute(&format!("DELETE FROM track_composers WHERE track_id IN ({tracks_of})"), params![source_id])?;
        // A genre only its tracks had leaves with them.
        {
            let mut delete = tx.prepare("DELETE FROM genres WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM track_genres WHERE genre_id = ?1)")?;
            for genre in genres {
                delete.execute(params![genre])?;
            }
        }
        tx.execute(&format!("DELETE FROM audio_features WHERE track_id IN ({tracks_of})"), params![source_id])?;
        tx.execute(&format!("DELETE FROM scanned_files WHERE track_id IN ({tracks_of})"), params![source_id])?;
        tx.execute(&format!("DELETE FROM tracks WHERE id IN ({tracks_of})"), params![source_id])?;
        {
            let mut delete = tx.prepare("DELETE FROM albums WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM tracks WHERE album_id = ?1)")?;
            for album in albums {
                delete.execute(params![album])?;
            }
        }
        tx.execute("DELETE FROM roots WHERE source_id = ?1", params![source_id])?;
        if preserve_playlists {
            tx.execute("UPDATE sources SET reachable = 0 WHERE id = ?1", params![source_id])?;
            tx.execute("UPDATE tracks SET reachable = 0 WHERE source_id = ?1", params![source_id])?;
            tx.execute("DELETE FROM sources WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM tracks WHERE source_id = ?1)
                AND NOT EXISTS (SELECT 1 FROM playlist_tracks WHERE source_id = ?1)", params![source_id])?;
        } else {
            tx.execute("DELETE FROM sources WHERE id = ?1", params![source_id])?;
        }
        // The credits only it carried name nobody now.
        self.db.execute(
            "DELETE FROM credit_parts WHERE credit NOT IN
                 (SELECT trim(artist) FROM tracks UNION SELECT trim(album_credit) FROM tracks
                  UNION SELECT trim(album_artist) FROM albums)",
            [],
        )?;
        self.match_credits()?;
        self.rebuild_rollups()?;
        tx.finish()
    }
}

/// Records every track of which is the source's: they go with it. A record
/// that also holds tracks of another source stays.
const ALBUMS_ONLY_OF: &str = "SELECT DISTINCT t.album_id FROM tracks t
     WHERE t.source_id = ?1 AND t.album_id IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM tracks o WHERE o.album_id = t.album_id AND o.source_id <> ?1)";

/// The source's tracks in playlists that are not its own.
const ENTRIES_IN_OTHERS: &str = "SELECT pt.playlist_id FROM playlist_tracks pt
     JOIN playlists p ON p.id = pt.playlist_id
     WHERE p.source_id <> ?1
       AND (pt.source_id = ?1 OR pt.track_id IN (SELECT id FROM tracks WHERE source_id = ?1))";

#[cfg(test)]
mod tests;
