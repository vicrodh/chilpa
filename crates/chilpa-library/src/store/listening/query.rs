//! I query the log by event start, using captured local time or an explicit UTC offset.

use crate::store::Library;
use rusqlite::{params, Row};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListenFilter {
    /// Inclusive Unix seconds. No bound means all history.
    pub from: Option<i64>,
    /// Exclusive Unix seconds, so adjacent periods never double-count an event.
    pub until: Option<i64>,
    pub source_id: Option<i64>,
    pub genre: Option<String>,
    pub time: TimeBasis,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeBasis {
    /// Historical events without a captured offset have an unknown time bucket.
    #[default]
    RecordedLocal,
    Utc,
    /// Seconds east of UTC, explicitly chosen for comparisons across time zones.
    Offset(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenDimension {
    Track,
    Album,
    Artist,
    Genre,
    Source,
    Hour,
    Weekday,
    Day,
    GenreHour,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenOrder {
    Plays,
    ListenedTime,
    Key,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListenMetrics {
    pub events: u64,
    /// I retain the existing rule: at least half heard, or unknown duration.
    pub plays: u64,
    pub listened_ms: u64,
    pub skips: u64,
    pub distinct_tracks: u64,
    pub bit_perfect: u64,
    /// Metadata reconstructed during migration, not captured during that listen.
    pub backfilled: u64,
    pub unknown_local_time: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListenBucket {
    /// None explicitly represents missing identity, genre or time evidence.
    pub key: Option<String>,
    pub label: String,
    /// Only GenreHour supplies this second dimension (0..23).
    pub hour: Option<u8>,
    pub metrics: ListenMetrics,
}

const METRICS: &str = "count(*),
    coalesce(sum(duration_ms = 0 OR played_ms >= duration_ms / 2 + duration_ms % 2), 0),
    coalesce(sum(played_ms), 0), coalesce(sum(skipped), 0), count(DISTINCT identity),
    coalesce(sum(bit_perfect), 0), coalesce(sum(backfilled), 0),
    coalesce(sum(utc_offset_seconds IS NULL), 0)";

fn metrics(r: &Row<'_>, at: usize) -> rusqlite::Result<ListenMetrics> {
    let count = |i| r.get::<_, i64>(i).map(|v| v.max(0) as u64);
    Ok(ListenMetrics {
        events: count(at)?,
        plays: count(at + 1)?,
        listened_ms: count(at + 2)?,
        skips: count(at + 3)?,
        distinct_tracks: count(at + 4)?,
        bit_perfect: count(at + 5)?,
        backfilled: count(at + 6)?,
        unknown_local_time: count(at + 7)?,
    })
}

fn offset(filter: &ListenFilter) -> rusqlite::Result<Option<i32>> {
    if filter.from.zip(filter.until).is_some_and(|(a, b)| a >= b) {
        return Err(rusqlite::Error::InvalidParameterName(
            "empty or reversed listening period".into(),
        ));
    }
    match filter.time {
        TimeBasis::RecordedLocal => Ok(None),
        TimeBasis::Utc => Ok(Some(0)),
        TimeBasis::Offset(s) if (-86400..=86400).contains(&s) => Ok(Some(s)),
        _ => Err(rusqlite::Error::InvalidParameterName(
            "invalid UTC offset".into(),
        )),
    }
}

const LISTENS: &str = "WITH listens AS (
    SELECT p.*, c.title, c.artist, c.album,
        CASE WHEN coalesce(t.album_id, c.album_id) IS NOT NULL
             THEN 'album:' || coalesce(t.album_id, c.album_id)
             WHEN c.album_external_id <> '' AND p.source_id IS NOT NULL
             THEN printf('source:%d:%d:%s', p.source_id, length(c.album_external_id), c.album_external_id) END AS release_id,
        coalesce(c.backfilled, 1) AS backfilled, c.utc_offset_seconds,
        CASE WHEN c.external_id <> '' AND p.source_id IS NOT NULL
             THEN printf('%d:%d:%s', p.source_id, length(c.external_id), c.external_id)
             WHEN p.track_id IS NOT NULL THEN 'track:' || p.track_id END AS identity,
        strftime('%H', p.started_at + coalesce(?5, c.utc_offset_seconds), 'unixepoch') AS hour,
        strftime('%w', p.started_at + coalesce(?5, c.utc_offset_seconds), 'unixepoch') AS weekday,
        date(p.started_at + coalesce(?5, c.utc_offset_seconds), 'unixepoch') AS day
    FROM play_events p LEFT JOIN play_context c ON c.event_id = p.id
    LEFT JOIN tracks t ON t.id = p.track_id";

fn listens(filter: &ListenFilter) -> String {
    // I omit unused predicates so SQLite can seek the time/source indexes.
    let mut conditions = vec!["1"];
    if filter.from.is_some() {
        conditions.push("p.started_at >= ?1");
    }
    if filter.until.is_some() {
        conditions.push("p.started_at < ?2");
    }
    if filter.source_id.is_some() {
        conditions.push("p.source_id = ?3");
    }
    if filter.genre.is_some() {
        conditions
            .push("EXISTS (SELECT 1 FROM play_genres f WHERE f.event_id = p.id AND f.genre = ?4)");
    }
    format!("{LISTENS} WHERE {})", conditions.join(" AND "))
}

impl Library {
    pub fn listening_summary(&self, filter: &ListenFilter) -> rusqlite::Result<ListenMetrics> {
        let offset = offset(filter)?;
        self.db.query_row(
            &format!("{} SELECT {METRICS} FROM listens", listens(filter)),
            params![
                filter.from,
                filter.until,
                filter.source_id,
                filter.genre,
                offset
            ],
            |r| metrics(r, 0),
        )
    }

    /// I credit every genre in a multi-genre event in full. Genre buckets overlap;
    /// I use listening_summary for additive totals. Hours/days describe starts,
    /// not a reconstruction of wall-clock listening intervals after pauses/seeks.
    pub fn listening_buckets(
        &self,
        filter: &ListenFilter,
        dimension: ListenDimension,
        order: ListenOrder,
        limit: usize,
    ) -> rusqlite::Result<Vec<ListenBucket>> {
        let offset = offset(filter)?;
        let (key, label, hour, join) = match dimension {
            ListenDimension::Track => ("identity", "title", "NULL", ""),
            ListenDimension::Album => ("cast(release_id AS TEXT)", "album", "NULL", ""),
            ListenDimension::Artist => ("nullif(artist, '') COLLATE NOCASE", "artist", "NULL", ""),
            ListenDimension::Genre => (
                "g.genre",
                "g.genre",
                "NULL",
                "LEFT JOIN play_genres g ON g.event_id = listens.id",
            ),
            ListenDimension::GenreHour => (
                "g.genre",
                "g.genre",
                "cast(listens.hour AS INTEGER)",
                "LEFT JOIN play_genres g ON g.event_id = listens.id",
            ),
            ListenDimension::Source => (
                "cast(source_id AS TEXT)",
                "cast(source_id AS TEXT)",
                "NULL",
                "",
            ),
            ListenDimension::Hour => ("listens.hour", "listens.hour", "NULL", ""),
            // SQLite weekday: Sunday = 0, Saturday = 6.
            ListenDimension::Weekday => ("weekday", "weekday", "NULL", ""),
            ListenDimension::Day => ("day", "day", "NULL", ""),
        };
        let ordering = match order {
            ListenOrder::Plays => "5 DESC, 6 DESC, 1, 3",
            ListenOrder::ListenedTime => "6 DESC, 5 DESC, 1, 3",
            ListenOrder::Key => "1, 3",
        };
        let sql = format!(
            "{} SELECT {key}, coalesce(min({label}), ''), {hour}, {METRICS}
            FROM listens {join} GROUP BY 1, 3 ORDER BY {ordering} LIMIT ?6",
            listens(filter)
        );
        self.db
            .prepare(&sql)?
            .query_map(
                params![
                    filter.from,
                    filter.until,
                    filter.source_id,
                    filter.genre,
                    offset,
                    limit.min(1000) as i64
                ],
                |r| {
                    Ok(ListenBucket {
                        key: r.get(0)?,
                        label: r.get(1)?,
                        hour: r.get(2)?,
                        metrics: metrics(r, 3)?,
                    })
                },
            )?
            .collect()
    }
}
