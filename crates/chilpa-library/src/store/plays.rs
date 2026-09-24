//! The play log and what is summed from it.
//!
//! The log is append-only and the stats come from summing it, not from
//! counters: a counter drifts out of sync and nobody notices; the log can
//! always be summed again.

use crate::store::Library;
use rusqlite::params;
use crate::store::{row_to_track, StoredTrack};

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
        self.db.query_row(
            "SELECT
                 COALESCE(sum(CASE WHEN duration_ms = 0 OR played_ms * 2 >= duration_ms
                                   THEN 1 ELSE 0 END), 0),
                 COALESCE(sum(played_ms), 0),
                 count(DISTINCT track_id),
                 COALESCE(sum(skipped), 0),
                 COALESCE(sum(bit_perfect), 0),
                 min(started_at)
             FROM play_events",
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
                    t.track_number, t.duration_ms, t.present,
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

    /// The last things played, without repeating a track.
    pub fn recently_played(&self, limit: usize) -> rusqlite::Result<Vec<TrackStat>> {
        // By `last_touched`, which counts a skip: what came up last is what
        // came up last. The number beside it is the play count, which does
        // not -- those are two different questions and the rollup keeps both.
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present,
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
        self.db.execute(
            "INSERT INTO play_events
             (track_id, source_id, started_at, played_ms, duration_ms, skipped,
              sample_rate_hz, bit_depth, bit_perfect)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                track_id,
                source_id,
                started_at,
                played_ms as i64,
                duration_ms as i64,
                i64::from(skipped),
                sample_rate_hz,
                bit_depth,
                i64::from(bit_perfect)
            ],
        )?;
        Ok(())
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
            out
        };

        let kept = snapshot(&lib);
        lib.rebuild_rollups().unwrap();
        let counted = snapshot(&lib);
        assert_eq!(kept, counted, "what the triggers kept is not what the log says");

        // And the numbers themselves are the ones the rules ask for.
        assert!(kept.contains(&"track 1 1 10 20".to_string()), "{kept:?}");
        assert!(kept.contains(&"album 1 2 30 3 3".to_string()), "{kept:?}");
    }

}
