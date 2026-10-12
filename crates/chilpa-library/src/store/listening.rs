//! I keep listening evidence independent of mutable tags and future source adapters.

mod query;
use super::Library;
pub use query::{
    HistoryEntry, ListenBucket, ListenDimension, ListenFilter, ListenMetrics, ListenOrder, TimeBasis,
};
use rusqlite::{params, OptionalExtension};

/// I capture this at playback start. Unknown metadata and time zones remain unknown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListenContext {
    pub track_id: Option<i64>,
    pub source_id: Option<i64>,
    /// A source's stable media key, never its temporary or authenticated stream URL.
    pub external_id: String,
    /// An adapter can name a release even before it is cached in the library.
    pub album_external_id: String,
    pub album_id: Option<i64>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    /// I keep the local cover available when this listen began.
    pub artwork_path: String,
    pub genres: Vec<String>,
    /// Seconds east of UTC at the instant listening started, including DST.
    pub utc_offset_seconds: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct ListenEvent {
    pub context: ListenContext,
    pub started_at: i64,
    pub played_ms: u64,
    pub duration_ms: u64,
    pub skipped: bool,
    pub sample_rate_hz: Option<u32>,
    pub bit_depth: Option<u8>,
    pub bit_perfect: bool,
}

impl Library {
    pub fn listen_context(&self, track_id: i64) -> rusqlite::Result<ListenContext> {
        let mut context = self.db.query_row(
            "SELECT t.source_id, t.external_id, t.album_id,
            t.title, t.artist, coalesce(a.title, ''), coalesce(a.album_artist, '')
            FROM tracks t LEFT JOIN albums a ON a.id = t.album_id WHERE t.id = ?1",
            [track_id],
            |r| {
                Ok(ListenContext {
                    track_id: Some(track_id),
                    source_id: Some(r.get(0)?),
                    external_id: r.get(1)?,
                    album_id: r.get(2)?,
                    title: r.get(3)?,
                    artist: r.get(4)?,
                    album: r.get(5)?,
                    album_artist: r.get(6)?,
                    ..Default::default()
                })
            },
        )?;
        context.genres = self
            .db
            .prepare(
                "SELECT g.name FROM genres g JOIN track_genres tg
            ON tg.genre_id = g.id WHERE tg.track_id = ?1 ORDER BY g.name",
            )?
            .query_map([track_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(context)
    }

    /// I resolve local paths only within the local source, never by a URL's spelling.
    pub fn local_listen_context(&self, path: &str) -> rusqlite::Result<Option<ListenContext>> {
        let id = self
            .db
            .query_row(
                "SELECT t.id FROM tracks t JOIN sources s ON s.id = t.source_id
            WHERE s.provider = 'local' AND s.instance = 'default' AND t.external_id = ?1",
                [path],
                |r| r.get(0),
            )
            .optional()?;
        id.map(|id| self.listen_context(id)).transpose()
    }

    pub fn local_utc_offset(&self, started_at: i64) -> rusqlite::Result<Option<i32>> {
        self.db.query_row(
            "SELECT unixepoch(?1, 'unixepoch', 'localtime') - ?1",
            [started_at],
            |r| r.get(0),
        )
    }

    /// I commit the event and its evidence together; source mismatches are rejected.
    pub fn record_listen(&self, event: &ListenEvent) -> rusqlite::Result<i64> {
        let invalid = || rusqlite::Error::InvalidParameterName("invalid listening evidence".into());
        let c = &event.context;
        if event.played_ms > i64::MAX as u64
            || event.duration_ms > i64::MAX as u64
            || c.utc_offset_seconds
                .is_some_and(|s| !(-86400..=86400).contains(&s))
            || (c.source_id.is_none()
                && (!c.external_id.is_empty() || !c.album_external_id.is_empty()))
        {
            return Err(invalid());
        }
        let tx = self.own_transaction()?;
        if let Some(track) = c.track_id {
            let source: i64 = tx.query_row(
                "SELECT source_id FROM tracks WHERE id = ?1",
                [track],
                |r| r.get(0),
            )?;
            // I allow a scan to relocate the same track while it is playing.
            if c.source_id != Some(source) {
                return Err(invalid());
            }
        }
        tx.execute(
            "INSERT INTO play_events(track_id, source_id, started_at, played_ms,
            duration_ms, skipped, sample_rate_hz, bit_depth, bit_perfect)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                c.track_id,
                c.source_id,
                event.started_at,
                event.played_ms as i64,
                event.duration_ms as i64,
                event.skipped,
                event.sample_rate_hz,
                event.bit_depth,
                event.bit_perfect
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO play_context(event_id, external_id, album_id, title, artist,
            album, album_artist, utc_offset_seconds, album_external_id, artwork_path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, c.external_id, c.album_id, c.title, c.artist, c.album, c.album_artist, c.utc_offset_seconds, c.album_external_id, c.artwork_path])?;
        for genre in super::split_genres(&c.genres) {
            tx.prepare_cached(
                "INSERT OR IGNORE INTO play_genres(event_id, genre) VALUES (?1, ?2)",
            )?
            .execute(params![id, genre])?;
        }
        tx.finish()?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests;
