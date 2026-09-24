//! What analysis pulls out of a track: tempo, key and loudness.
//!
//! Stored with the version of the analyzer that produced it, so analyzing
//! again with a newer version replaces the row instead of leaving two
//! truths.

use crate::store::Library;
use rusqlite::{params, OptionalExtension};
use crate::store::{now_unix, row_to_track, StoredTrack};

impl Library {
    /// Present tracks that are missing that version's analysis.
    ///
    /// Asked for in batches: analyzing a large library cannot start by loading
    /// its whole list into memory.
    pub fn tracks_pending_analysis(
        &self,
        analyzer: &str,
        version: i64,
        limit: usize,
    ) -> rusqlite::Result<Vec<StoredTrack>> {
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present
             FROM tracks t
             LEFT JOIN albums a ON a.id = t.album_id
             WHERE t.present = 1
               AND NOT EXISTS (
                   SELECT 1 FROM audio_features f
                   WHERE f.track_id = t.id AND f.analyzer = ?1 AND f.version = ?2
               )
             ORDER BY t.id
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![analyzer, version, limit as i64], row_to_track)?;
        rows.collect()
    }

    /// How many present tracks already have that version's analysis.
    pub fn analyzed_count(&self, analyzer: &str, version: i64) -> rusqlite::Result<usize> {
        let n: i64 = self.db.query_row(
            "SELECT count(*) FROM audio_features f
             JOIN tracks t ON t.id = f.track_id AND t.present = 1
             WHERE f.analyzer = ?1 AND f.version = ?2",
            params![analyzer, version],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Stores what analysis pulled out of a track.
    ///
    /// One row per analyzer and version: a better analyzer writes its own and
    /// does not disturb the old one.
    pub fn save_features(
        &self,
        track_id: i64,
        analyzer: &str,
        version: i64,
        bpm: Option<f32>,
        tempo_confidence: Option<f32>,
        musical_key: Option<String>,
        loudness_lufs: Option<f32>,
    ) -> rusqlite::Result<()> {
        self.db.execute(
            "INSERT INTO audio_features
                 (track_id, analyzer, version, bpm, tempo_confidence, musical_key,
                  loudness_lufs, analyzed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(track_id, analyzer, version) DO UPDATE SET
               bpm = excluded.bpm, tempo_confidence = excluded.tempo_confidence,
               musical_key = excluded.musical_key, loudness_lufs = excluded.loudness_lufs,
               analyzed_at = excluded.analyzed_at",
            params![
                track_id,
                analyzer,
                version,
                bpm,
                tempo_confidence,
                musical_key,
                loudness_lufs,
                now_unix()
            ],
        )?;
        Ok(())
    }

    /// What analysis attached to several tracks, in one query.
    ///
    /// One query per row of a list is exactly what made QBZ's library slow.
        pub fn features_for(
        &self,
        track_ids: &[i64],
        analyzer: &str,
    ) -> rusqlite::Result<std::collections::HashMap<i64, (Option<f32>, Option<String>, Option<f32>)>>
    {
        let mut output = std::collections::HashMap::new();
        if track_ids.is_empty() {
            return Ok(output);
        }
        // The ids are integers that came from the database itself; even so
        // they go as parameters, to avoid two ways of building a query.
        let gaps = vec!["?"; track_ids.len()].join(",");
        let sql = format!(
            "SELECT track_id, bpm, musical_key, loudness_lufs FROM audio_features
             WHERE analyzer = ?1 AND track_id IN ({gaps})
             ORDER BY version",
        );
        let mut stmt = self.db.prepare(&sql)?;
        let mut values: Vec<rusqlite::types::Value> =
            vec![rusqlite::types::Value::Text(analyzer.to_string())];
        values.extend(track_ids.iter().map(|i| rusqlite::types::Value::Integer(*i)));
        let rows = stmt.query_map(rusqlite::params_from_iter(values), |r| {
            Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
        })?;
        for row in rows {
            let (id, data) = row?;
            // Ordered by version, so the last one in is the newest.
                        output.insert(id, data);
        }
        Ok(output)
    }

    /// What analysis attached to a track: tempo, key and loudness.
    #[allow(clippy::type_complexity)]
    pub fn track_features(
        &self,
        track_id: i64,
        analyzer: &str,
    ) -> rusqlite::Result<Option<(Option<f32>, Option<String>, Option<f32>)>> {
        self.db
            .query_row(
                "SELECT bpm, musical_key, loudness_lufs FROM audio_features
                 WHERE track_id = ?1 AND analyzer = ?2
                 ORDER BY version DESC LIMIT 1",
                params![track_id, analyzer],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
    }

    /// The BPM analysis attached to a track, if it attached one.
    pub fn track_bpm(&self, track_id: i64, analyzer: &str) -> rusqlite::Result<Option<f32>> {
        self.db
            .query_row(
                "SELECT bpm FROM audio_features
                 WHERE track_id = ?1 AND analyzer = ?2
                 ORDER BY version DESC LIMIT 1",
                params![track_id, analyzer],
                |r| r.get(0),
            )
            .optional()
            .map(|v| v.flatten())
    }
}

#[cfg(test)]
mod tests {
    use crate::store::fixtures::*;

    #[test]
    fn features_for_several_tracks_come_out_in_one_query() {
        let lib = sample_library();
        lib.save_features(1, "chilpa", 1, Some(128.0), None, Some("C · 8B".into()), Some(-9.5))
            .unwrap();
        lib.save_features(2, "chilpa", 1, Some(90.0), None, None, None).unwrap();
        // A newer version of the same track wins.
        lib.save_features(1, "chilpa", 2, Some(129.0), None, Some("G · 9B".into()), Some(-8.0))
            .unwrap();

        let map = lib.features_for(&[1, 2, 3], "chilpa").unwrap();
        assert_eq!(map.len(), 2, "la 3 no tiene análisis");
        assert_eq!(map[&1].0, Some(129.0), "la versión más nueva");
        assert_eq!(map[&1].1.as_deref(), Some("G · 9B"));
        assert_eq!(map[&1].2, Some(-8.0), "la sonoridad también viene");
        assert_eq!(map[&2].1, None);
        assert!(lib.features_for(&[], "chilpa").unwrap().is_empty());
    }

}
