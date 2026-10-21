//! The play log and what is summed from it.
//!
//! The log is append-only and the stats come from summing it, not from
//! counters: a counter drifts out of sync and nobody notices; the log can
//! always be summed again.

use crate::store::Library;
use rusqlite::params;
use crate::store::{row_to_track, StoredTrack};

/// A record played lately: the library's by its id, a source's by the
/// source and the record's id there, as the play log names it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RecentRelease {
    pub album_id: Option<i64>,
    pub provider: String,
    pub instance: String,
    pub album_external_id: String,
    pub last_played: i64,
}

/// What the play log says taken together.
///
/// It comes from the log, not from counters: a counter drifts out of sync
/// and nobody notices; the log can always be summed again.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Stats {
    /// Full plays: the ones that played more than halfway.
    pub plays: usize,
    /// Everything delivered, in milliseconds, full plays or not.
    pub listened_ms: u64,
    /// How many distinct tracks have been played.
    pub distinct_tracks: usize,
    /// How many times something was skipped.
    pub skips: usize,
    /// How many of the plays came out bit-perfect.
    pub bit_perfect: usize,
    /// When the first recorded play was, in unix seconds.
    pub since: Option<i64>,
}

/// A track with its count, for the tops and the history.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TrackStat {
    pub track: StoredTrack,
    /// Plays, or the moment of the last one, depending on which list it is.
    pub count: usize,
    pub at: Option<i64>,
}

impl Library {
    /// The summary of everything that has been played.
    pub fn stats(&self) -> rusqlite::Result<Stats> {
        // One row, kept by a trigger. Summing the log grew with how long
        // somebody had been using Chilpa, which is the one thing a query
        // must never grow with. See `MIGRATION_V10`.
        self.db.query_row(
            "SELECT plays, listened_ms, tracks, skips, bit_perfect, since
             FROM library_totals WHERE id = 1",
            [],
            |r| {
                Ok(Stats {
                    plays: r.get::<_, i64>(0)? as usize,
                    listened_ms: r.get::<_, i64>(1)?.max(0) as u64,
                    distinct_tracks: r.get::<_, i64>(2)? as usize,
                    skips: r.get::<_, i64>(3)? as usize,
                    bit_perfect: r.get::<_, i64>(4)? as usize,
                    since: r.get(5)?,
                })
            },
        )
    }

    /// The most played tracks.
    pub fn top_tracks(&self, limit: usize) -> rusqlite::Result<Vec<TrackStat>> {
        // The order comes from an index, so it reads as many rows as it
        // returns. Grouping the whole log to take twenty was the shape that
        // made this grow with the library instead of with the answer.
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, (t.present = 1 AND t.reachable = 1),
                    r.plays, r.last_played
             FROM track_rollup r
             JOIN tracks t ON t.id = r.track_id
             LEFT JOIN albums a ON a.id = t.album_id
             WHERE r.plays > 0
             ORDER BY r.plays DESC, r.last_played DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_track_stat)?;
        rows.collect()
    }

    /// The HiFi Wizard's test track for one format: the most played file at
    /// exactly `bits` and `rate`, lossless, local and playable now; any one
    /// of them when none has been played.
    pub fn most_played_at(&self, bits: u8, rate: u32) -> rusqlite::Result<Option<StoredTrack>> {
        use rusqlite::OptionalExtension;
        self.db
            .query_row(
                "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                        t.track_number, t.duration_ms, 1
                 FROM tracks t
                 JOIN sources s ON s.id = t.source_id
                 LEFT JOIN albums a ON a.id = t.album_id
                 LEFT JOIN track_rollup r ON r.track_id = t.id
                 WHERE t.present = 1 AND t.reachable = 1 AND s.provider = 'local'
                   AND t.bit_depth = ?1 AND t.sample_rate_hz = ?2
                   AND (lower(t.external_id) LIKE '%.flac' OR lower(t.external_id) LIKE '%.wav'
                        OR lower(t.external_id) LIKE '%.aif' OR lower(t.external_id) LIKE '%.aiff'
                        OR lower(t.external_id) LIKE '%.wv' OR lower(t.external_id) LIKE '%.ape'
                        OR lower(t.external_id) LIKE '%.m4a')
                 ORDER BY COALESCE(r.plays, 0) DESC, t.id
                 LIMIT 1",
                params![i64::from(bits), i64::from(rate)],
                row_to_track,
            )
            .optional()
    }

    /// The last things played, without repeating a track.
    pub fn recently_played(&self, limit: usize) -> rusqlite::Result<Vec<TrackStat>> {
        // By `last_touched`, which counts a skip: what came up last is what
        // came up last. The number beside it is the play count, which does
        // not -- those are two different questions and the rollup keeps both.
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, (t.present = 1 AND t.reachable = 1),
                    r.plays, r.last_touched
             FROM track_rollup r
             JOIN tracks t ON t.id = r.track_id
             LEFT JOIN albums a ON a.id = t.album_id
             WHERE r.last_touched IS NOT NULL
             ORDER BY r.last_touched DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_track_stat)?;
        rows.collect()
    }

    /// Records a play. The log is append-only.
    #[allow(clippy::too_many_arguments)]
    pub fn log_play(
        &self,
        track_id: Option<i64>,
        source_id: Option<i64>,
        started_at: i64,
        played_ms: u64,
        duration_ms: u64,
        skipped: bool,
        sample_rate_hz: Option<u32>,
        bit_depth: Option<u8>,
        bit_perfect: bool,
    ) -> rusqlite::Result<()> {
        let context = match track_id {
            Some(id) => self.listen_context(id)?,
            None => super::listening::ListenContext { source_id, ..Default::default() },
        };
        if source_id.is_some() && source_id != context.source_id {
            return Err(rusqlite::Error::InvalidParameterName("source does not own the track".into()));
        }
        self.record_listen(&super::listening::ListenEvent {
            context, started_at, played_ms, duration_ms, skipped, sample_rate_hz, bit_depth, bit_perfect,
        }).map(|_| ())
    }

    /// The records played last, wherever they played, the latest first:
    /// one line per record, not per play.
    pub fn recent_releases(&self, limit: usize) -> rusqlite::Result<Vec<RecentRelease>> {
        let mut stmt = self.db.prepare(
            "SELECT coalesce(t.album_id, c.album_id), coalesce(s.provider, ''), coalesce(s.instance, ''),
                    coalesce(c.album_external_id, ''), max(p.started_at) AS last
             FROM play_events p
             LEFT JOIN play_context c ON c.event_id = p.id
             LEFT JOIN tracks t ON t.id = p.track_id
             LEFT JOIN sources s ON s.id = p.source_id
             WHERE coalesce(t.album_id, c.album_id) IS NOT NULL
                OR (c.album_external_id <> '' AND p.source_id IS NOT NULL)
             GROUP BY CASE WHEN coalesce(t.album_id, c.album_id) IS NOT NULL
                           THEN 'album:' || coalesce(t.album_id, c.album_id)
                           ELSE p.source_id || ':' || c.album_external_id END
             ORDER BY last DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(RecentRelease { album_id: r.get(0)?, provider: r.get(1)?, instance: r.get(2)?, album_external_id: r.get(3)?, last_played: r.get(4)? })
        })?;
        rows.collect()
    }

    /// I read the same counts as `plays:`, once for the whole grouping.
    pub fn track_play_counts(&self) -> rusqlite::Result<std::collections::HashMap<i64, i64>> {
        let mut statement = self.db.prepare("SELECT track_id, plays FROM track_rollup")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }

    /// I sum those track counts per record in one query, including absent tracks.
    pub fn album_play_counts(&self) -> rusqlite::Result<std::collections::HashMap<i64, i64>> {
        let mut statement = self.db.prepare("SELECT t.album_id, sum(r.plays)
            FROM track_rollup r JOIN tracks t ON t.id = r.track_id
            WHERE t.album_id IS NOT NULL GROUP BY t.album_id")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }

    /// How many times a track has been played, counting only what played more
    /// than halfway. It is the definition the stats use.
    pub fn play_count(&self, track_id: i64) -> rusqlite::Result<usize> {
        let n: i64 = self.db.query_row(
            "SELECT count(*) FROM play_events
             WHERE track_id = ?1 AND (duration_ms = 0 OR played_ms * 2 >= duration_ms)",
            params![track_id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }
}

fn row_to_track_stat(r: &rusqlite::Row<'_>) -> rusqlite::Result<TrackStat> {
    Ok(TrackStat {
        track: row_to_track(r)?,
        count: r.get::<_, i64>(8)? as usize,
        at: r.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fixtures::*;

    #[test]
    fn the_stats_come_from_the_whole_log() {
        let lib = sample_library();
        let s = lib.stats().unwrap();
        assert_eq!(s.plays, 3, "three complete plays");
        assert_eq!(s.distinct_tracks, 2, "of two distinct tracks");
        assert_eq!(s.listened_ms, 3_000);
        assert_eq!(s.bit_perfect, 3);
        assert_eq!(s.skips, 0);
        assert_eq!(s.since, Some(10), "the first was at second 10");
    }

    #[test]
    fn without_a_single_play_the_stats_are_zeros() {
        let lib = Library::in_memory().unwrap();
        let s = lib.stats().unwrap();
        assert_eq!(s, Stats::default(), "zeros, not an error");
    }

    #[test]
    fn the_top_and_the_history_look_at_the_same_thing_from_different_sides() {
        let lib = sample_library();
        // Album 3's track plays twice; album 1's once, and earlier.
        let top = lib.top_tracks(10).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].count, 2, "the most played first");
        assert_eq!(top[1].count, 1);

        let recent = lib.recently_played(10).unwrap();
        assert_eq!(recent[0].at, Some(30), "the last one played first");
        assert_eq!(recent[1].at, Some(10));
        assert_eq!(recent.len(), 2, "one track per line, not one per play");
    }

    #[test]
    fn the_records_played_last_come_from_the_library_and_from_the_sources() {
        use crate::store::listening::{ListenContext, ListenEvent};
        let lib = sample_library();
        let service = lib.source_id("plugin", "app.example.music").unwrap();
        let heard = |at: i64, external: &str, record: &str| lib.record_listen(&ListenEvent {
            context: ListenContext { source_id: Some(service), external_id: external.into(), album_external_id: record.into(), ..Default::default() },
            started_at: at, played_ms: 1000, duration_ms: 1000, skipped: false, sample_rate_hz: None, bit_depth: None, bit_perfect: false,
        }).unwrap();
        heard(25, "t-1", "rec-9");
        heard(40, "t-2", "rec-9");
        heard(5, "t-3", "");
        let recent = lib.recent_releases(10).unwrap();
        let named: Vec<String> = recent.iter().map(|r| r.album_id.map_or(format!("{}:{}", r.instance, r.album_external_id), |id| format!("album:{id}"))).collect();
        assert_eq!(named, ["app.example.music:rec-9", "album:3", "album:1"], "the latest first, one line per record");
        assert_eq!(recent[0].last_played, 40);
        assert_eq!(recent[0].provider, "plugin");
        assert_eq!(lib.recent_releases(1).unwrap().len(), 1);
    }


    #[test]
    fn the_wizard_s_track_for_a_format_is_the_most_played_lossless_file_at_it() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (id, path, bits, rate) in [(1, "/a.flac", 24, 96_000), (2, "/b.flac", 24, 96_000), (3, "/c.mp3", 16, 44_100), (4, "/d.flac", 16, 48_000)] {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, duration_ms, added_at, bit_depth, sample_rate_hz)
                     VALUES (?1, ?2, ?3, 'T', 200000, 0, ?4, ?5)",
                    params![id, source, path, bits, rate],
                )
                .unwrap();
        }
        assert_eq!(lib.most_played_at(24, 96_000).unwrap().map(|t| t.id), Some(1), "any one before anything is played");
        lib.log_play(Some(2), Some(source), 1, 200_000, 200_000, false, None, None, true).unwrap();
        assert_eq!(lib.most_played_at(24, 96_000).unwrap().map(|t| t.external_id), Some("/b.flac".into()));
        assert_eq!(lib.most_played_at(16, 44_100).unwrap(), None, "a lossy file is no test of the format");
        assert_eq!(lib.most_played_at(24, 192_000).unwrap(), None);
    }

    #[test]
    fn a_half_played_track_does_not_count_as_a_play() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (source_id, external_id, title, duration_ms, added_at)
                 VALUES (?1, '/a.flac', 'One', 200000, 0)",
                params![source],
            )
            .unwrap();
        lib.log_play(Some(1), Some(source), 1, 20_000, 200_000, true, Some(44_100), Some(16), true)
            .unwrap();
        assert_eq!(lib.play_count(1).unwrap(), 0, "20 s out of 200 is not a play");
        lib.log_play(Some(1), Some(source), 2, 150_000, 200_000, false, Some(44_100), Some(16), true)
            .unwrap();
        assert_eq!(lib.play_count(1).unwrap(), 1);
    }
    #[test]
    fn what_is_kept_is_what_counting_would_give() {
        // The rollups are the whole reason the grid and the stats are fast,
        // and the whole risk: a number maintained by a trigger is wrong
        // without saying so. This drives every path that writes -- a play, a
        // half play, a track arriving, a track going missing and coming back
        // -- and then rebuilds from the log and demands the same answer.
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db
            .execute(
                "INSERT INTO albums (id, title, album_artist) VALUES (1, 'An album', 'Someone')",
                [],
            )
            .unwrap();
        for id in 1..=3i64 {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, album_id,
                                         duration_ms, added_at)
                     VALUES (?1, ?2, ?3, 'Track', 1, 200000, ?1)",
                    params![id, source, format!("/music/{id}.flac")],
                )
                .unwrap();
        }
        // A listen, a skip, and a listen of something else.
        lib.log_play(Some(1), Some(source), 10, 200_000, 200_000, false, None, None, true).unwrap();
        lib.log_play(Some(1), Some(source), 20, 10_000, 200_000, true, None, None, true).unwrap();
        lib.log_play(Some(2), Some(source), 30, 200_000, 200_000, false, None, None, true).unwrap();
        // One goes missing and comes back, which is what a scan does.
        lib.db.execute("UPDATE tracks SET present = 0 WHERE id = 3", []).unwrap();
        lib.db.execute("UPDATE tracks SET present = 1 WHERE id = 3", []).unwrap();

        let snapshot = |lib: &Library| {
            let mut out = Vec::new();
            let mut stmt = lib
                .db
                .prepare(
                    "SELECT album_id, plays, COALESCE(last_played, -1), added_at, present_tracks
                     FROM album_rollup ORDER BY album_id",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok(format!(
                        "album {} {} {} {} {}",
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, i64>(4)?
                    ))
                })
                .unwrap();
            out.extend(rows.map(Result::unwrap));
            let mut stmt = lib
                .db
                .prepare(
                    "SELECT track_id, plays, COALESCE(last_played, -1), COALESCE(last_touched, -1)
                     FROM track_rollup ORDER BY track_id",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok(format!(
                        "track {} {} {} {}",
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?
                    ))
                })
                .unwrap();
            out.extend(rows.map(Result::unwrap));
            let t = lib
                .db
                .query_row(
                    "SELECT plays, listened_ms, tracks, skips, bit_perfect,
                            COALESCE(since, -1)
                     FROM library_totals WHERE id = 1",
                    [],
                    |r| {
                        Ok(format!(
                            "totals {} {} {} {} {} {}",
                            r.get::<_, i64>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, i64>(5)?
                        ))
                    },
                )
                .unwrap();
            out.push(t);
            out
        };

        let kept = snapshot(&lib);
        lib.rebuild_rollups().unwrap();
        let counted = snapshot(&lib);
        assert_eq!(kept, counted, "what the triggers kept is not what the log says");

        // And the numbers themselves are the ones the rules ask for.
        assert!(kept.contains(&"track 1 1 10 20".to_string()), "{kept:?}");
        assert!(kept.contains(&"album 1 2 30 3 3".to_string()), "{kept:?}");
        // Two plays and a skip, 410 s listened, two different tracks, one
        // skip, all three bit-perfect, and the log starts at 10.
        assert!(kept.contains(&"totals 2 410000 2 1 3 10".to_string()), "{kept:?}");

        // And the view that reads them says the same.
        let stats = lib.stats().unwrap();
        assert_eq!(stats.plays, 2);
        assert_eq!(stats.distinct_tracks, 2);
        assert_eq!(stats.skips, 1);
        assert_eq!(stats.bit_perfect, 3);
        assert_eq!(stats.since, Some(10));
    }

}
