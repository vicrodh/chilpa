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
    /// The decade of the record's year, as "1970".
    Decade,
    /// The month the listen started in, as "2026-10".
    Month,
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
        CASE WHEN p.track_id IS NOT NULL THEN 'track:' || p.track_id
             WHEN c.external_id <> '' AND p.source_id IS NOT NULL
             THEN printf('%d:%d:%s', p.source_id, length(c.external_id), c.external_id)
             END AS identity,
        strftime('%H', p.started_at + coalesce(?5, c.utc_offset_seconds), 'unixepoch') AS hour,
        strftime('%w', p.started_at + coalesce(?5, c.utc_offset_seconds), 'unixepoch') AS weekday,
        date(p.started_at + coalesce(?5, c.utc_offset_seconds), 'unixepoch') AS day,
        (SELECT ra.year FROM albums ra WHERE ra.id = coalesce(t.album_id, c.album_id)) AS year
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

/// One listen of the history, as it sounded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryEntry {
    pub event_id: i64,
    pub started_at: i64,
    /// The date and time it started where it was heard: "2026-10-09 21:14".
    pub local_time: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub played_ms: u64,
    pub duration_ms: u64,
    pub skipped: bool,
    pub bit_perfect: bool,
    pub sample_rate_hz: Option<u32>,
    pub bit_depth: Option<u8>,
    pub track_id: Option<i64>,
    pub album_id: Option<i64>,
    pub source_id: Option<i64>,
    pub artwork_path: Option<String>,
}

/// The history's names: as the listen recorded them, else as the library has
/// them now.
const TITLE: &str = "coalesce(nullif(c.title, ''), t.title, '')";
const ARTIST: &str = "coalesce(nullif(c.artist, ''), t.artist, '')";
const ALBUM: &str = "coalesce(nullif(c.album, ''), a.title, '')";
const JOINS: &str = "LEFT JOIN play_context c ON c.event_id = p.id
             LEFT JOIN tracks t ON t.id = p.track_id
             LEFT JOIN albums a ON a.id = coalesce(t.album_id, c.album_id)";

impl Library {
    /// How many artists and how many records the period's listens name.
    pub fn listening_counts(&self, filter: &ListenFilter) -> rusqlite::Result<(u64, u64)> {
        let offset = offset(filter)?;
        self.db.query_row(
            &format!("{} SELECT count(DISTINCT lower(nullif(artist, ''))), count(DISTINCT release_id) FROM listens", listens(filter)),
            params![filter.from, filter.until, filter.source_id, filter.genre, offset],
            |r| Ok((r.get::<_, i64>(0)?.max(0) as u64, r.get::<_, i64>(1)?.max(0) as u64)),
        )
    }

    /// The listens of the period, newest first, `limit` of them after the
    /// first `skip`, and how many there are in all. With `search`, only the
    /// listens whose track, artist or record, as the listen named them,
    /// hold every word of it.
    pub fn listening_history(&self, filter: &ListenFilter, search: &str, skip: usize, limit: usize) -> rusqlite::Result<(Vec<HistoryEntry>, u64)> {
        use rusqlite::types::Value;
        offset(filter)?;
        // Only the bounds the filter has, so the time index can be used.
        let mut conditions = vec!["1".to_string()];
        let mut values: Vec<Value> = Vec::new();
        for (bound, sql) in [(filter.from, "p.started_at >= ?"), (filter.until, "p.started_at < ?"), (filter.source_id, "p.source_id = ?")] {
            if let Some(value) = bound {
                conditions.push(sql.to_string());
                values.push(Value::Integer(value));
            }
        }
        let words = crate::store::find::patterns(search);
        for word in &words {
            conditions.push(format!("({TITLE} LIKE ? ESCAPE '\\' OR {ARTIST} LIKE ? ESCAPE '\\' OR {ALBUM} LIKE ? ESCAPE '\\')"));
            for _ in 0..3 {
                values.push(Value::Text(word.clone()));
            }
        }
        let clause = conditions.join(" AND ");
        // The names live in the context and the library; a count without a
        // search needs neither.
        let joined = if words.is_empty() { "" } else { JOINS };
        let total: i64 = self.db.query_row(
            &format!("SELECT count(*) FROM play_events p {joined} WHERE {clause}"),
            rusqlite::params_from_iter(values.iter()),
            |r| r.get(0),
        )?;
        let sql = format!(
            concat!("SELECT p.id, p.started_at,
                strftime('%Y-%m-%d %H:%M', p.started_at + coalesce(c.utc_offset_seconds, unixepoch(p.started_at, 'unixepoch', 'localtime') - p.started_at), 'unixepoch'),
                {TITLE}, {ARTIST}, {ALBUM}, p.played_ms, p.duration_ms, p.skipped, p.bit_perfect,
                p.sample_rate_hz, p.bit_depth, p.track_id, a.id, p.source_id, ", album_art!("a"), "
             FROM play_events p {JOINS}
             WHERE {clause}
             ORDER BY p.started_at DESC, p.id DESC
             LIMIT ? OFFSET ?"),
            clause = clause, TITLE = TITLE, ARTIST = ARTIST, ALBUM = ALBUM, JOINS = JOINS
        );
        values.push(Value::Integer(limit.min(1000) as i64));
        values.push(Value::Integer(skip as i64));
        let rows = self.db.prepare(&sql)?.query_map(
            rusqlite::params_from_iter(values.iter()),
            |r| Ok(HistoryEntry {
                event_id: r.get(0)?,
                started_at: r.get(1)?,
                local_time: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                title: r.get(3)?,
                artist: r.get(4)?,
                album: r.get(5)?,
                played_ms: r.get::<_, i64>(6)?.max(0) as u64,
                duration_ms: r.get::<_, i64>(7)?.max(0) as u64,
                skipped: r.get(8)?,
                bit_perfect: r.get(9)?,
                sample_rate_hz: r.get(10)?,
                bit_depth: r.get(11)?,
                track_id: r.get(12)?,
                album_id: r.get(13)?,
                source_id: r.get(14)?,
                artwork_path: r.get(15)?,
            }),
        )?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((rows, total.max(0) as u64))
    }

    /// Where a local date, "2026-10-09", began, in Unix seconds: the
    /// midnight of that day where the listens were heard.
    pub fn local_date_start(&self, date: &str) -> rusqlite::Result<i64> {
        self.db.query_row("SELECT unixepoch(?1, 'utc')", [date], |r| r.get(0))
    }

    /// Where the local day began `days_ago` days back from today, in Unix
    /// seconds: a period of days ends at a local midnight, not at UTC's.
    pub fn local_day_start(&self, days_ago: i64) -> rusqlite::Result<i64> {
        self.db.query_row(
            "SELECT unixepoch(date('now', 'localtime', printf('%d days', -?1)), 'utc')",
            [days_ago],
            |r| r.get(0),
        )
    }

    /// `count` local dates, "2026-10-09", the first `back` days before today.
    pub fn local_days(&self, back: i64, count: i64) -> rusqlite::Result<Vec<String>> {
        self.db
            .prepare(
                "WITH RECURSIVE d(n, day) AS (
                    SELECT 0, date('now', 'localtime', printf('%d days', -?1))
                    UNION ALL SELECT n + 1, date(day, '+1 day') FROM d WHERE n + 1 < ?2)
                 SELECT day FROM d ORDER BY n",
            )?
            .query_map(params![back, count.clamp(0, 400)], |r| r.get(0))?
            .collect()
    }

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
            ListenDimension::Decade => ("cast(year / 10 * 10 AS TEXT)", "cast(year / 10 * 10 AS TEXT)", "NULL", ""),
            ListenDimension::Month => ("substr(day, 1, 7)", "substr(day, 1, 7)", "NULL", ""),
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
