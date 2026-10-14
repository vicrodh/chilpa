//! What analysis pulls out of a track: tempo, key and loudness.
//!
//! Stored with the version of the analyzer that produced it, so analyzing
//! again with a newer version replaces the row instead of leaving two
//! truths.

use crate::store::Library;
use rusqlite::{params, OptionalExtension};
use crate::store::{now_unix, row_to_track, StoredTrack};

/// One available local track and the analyses recorded for it.
pub struct AnalysisInventoryTrack {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub standard: bool,
    pub mood: bool,
    pub standard_at: Option<i64>,
    pub mood_at: Option<i64>,
    pub result: String,
    pub processed_at: Option<i64>,
}

/// A bounded page over the same local tracks the analyzers can read.
pub struct AnalysisInventory {
    pub total: usize,
    pub standard: usize,
    pub mood: usize,
    pub matched: usize,
    pub tracks: Vec<AnalysisInventoryTrack>,
}

const INVENTORY: &str = "WITH eligible AS (
    SELECT t.id, t.title, t.artist, COALESCE(a.title, '') AS album,
           COALESCE(f.version >= ?1, 0) AS standard,
           (m.track_id IS NOT NULL) AS mood,
           f.analyzed_at AS standard_at, m.analyzed_at AS mood_at
    FROM tracks t JOIN sources s ON s.id = t.source_id AND s.kind = 'local'
    LEFT JOIN albums a ON a.id = t.album_id
    LEFT JOIN audio_features f ON f.track_id = t.id AND f.analyzer = 'chilpa'
    LEFT JOIN mood_features m ON m.track_id = t.id AND m.analyzer = 'app.chilpa.mood'
    WHERE t.present = 1 AND t.reachable = 1
) ";
const INVENTORY_MATCH: &str = "(?2 = '' OR instr(lower(title), lower(?2)) > 0
    OR instr(lower(artist), lower(?2)) > 0 OR instr(lower(album), lower(?2)) > 0)
    AND (?3 = 'all' OR (?3 = 'standard' AND standard = 1)
    OR (?3 = 'mood' AND mood = 1)
    OR (?3 = 'standard-pending' AND standard = 0)
    OR (?3 = 'mood-pending' AND mood = 0))";

impl Library {
    /// Lists recorded analyses without loading an entire library into the UI.
    /// Mood means a stored result; the plugin owns its model version.
    pub fn analysis_inventory(&self, standard_version: i64, filter: &str, search: &str, offset: usize) -> rusqlite::Result<AnalysisInventory> {
        self.analysis_inventory_for_run(standard_version, filter, search, offset, 0)
    }

    pub fn analysis_inventory_for_run(&self, standard_version: i64, filter: &str, search: &str, offset: usize, run: i64) -> rusqlite::Result<AnalysisInventory> {
        let filter = match filter {
            "standard" | "mood" | "standard-pending" | "mood-pending" => filter,
            _ => "all",
        };
        let search: String = search.trim().chars().take(128).collect();
        let (total, standard, mood): (i64, i64, i64) = self.db.query_row(
            &(INVENTORY.to_owned() + "SELECT count(*), COALESCE(sum(standard), 0), COALESCE(sum(mood), 0) FROM eligible"),
            params![standard_version], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let matched: i64 = self.db.query_row(
            &(INVENTORY.to_owned() + "SELECT count(*) FROM eligible WHERE " + INVENTORY_MATCH
                + " AND (?5 = 0 OR EXISTS (SELECT 1 FROM analysis_results p WHERE p.run_id = ?5 AND p.track_id = eligible.id))"),
            params![standard_version, search, filter, offset as i64, run.max(0)], |row| row.get(0),
        )?;
        let mut stmt = self.db.prepare(&(INVENTORY.to_owned()
            + "SELECT id, title, artist, album, standard, mood, standard_at, mood_at,
                COALESCE(p.state, ''), p.analyzed_at FROM eligible
                LEFT JOIN analysis_results p ON p.track_id = eligible.id AND p.run_id = ?5 WHERE "
            + INVENTORY_MATCH
            + " AND (?5 = 0 OR p.track_id IS NOT NULL)
                ORDER BY p.analyzed_at DESC, p.rowid DESC, title COLLATE NOCASE, artist COLLATE NOCASE, id LIMIT 100 OFFSET ?4"))?;
        let tracks = stmt.query_map(params![standard_version, search, filter, offset.min(1_000_000) as i64, run.max(0)], |row| {
            Ok(AnalysisInventoryTrack { id: row.get(0)?, title: row.get(1)?, artist: row.get(2)?,
                album: row.get(3)?, standard: row.get::<_, i64>(4)? != 0, mood: row.get::<_, i64>(5)? != 0,
                standard_at:row.get(6)?, mood_at:row.get(7)?, result:row.get(8)?, processed_at:row.get(9)? })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(AnalysisInventory { total: total as usize, standard: standard as usize, mood: mood as usize,
            matched: matched as usize, tracks })
    }

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
        self.tracks_pending_analysis_under(analyzer, version, limit, None)
    }

    /// The same, only those under `folder` when one is given: what a folder
    /// just added holds, and nothing else of the library.
    pub fn tracks_pending_analysis_under(
        &self,
        analyzer: &str,
        version: i64,
        limit: usize,
        folder: Option<&str>,
    ) -> rusqlite::Result<Vec<StoredTrack>> {
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, (t.present = 1 AND t.reachable = 1)
             FROM tracks t
             LEFT JOIN albums a ON a.id = t.album_id
             WHERE t.present = 1 AND t.reachable = 1
               -- Analysis reads the file. A server's track has none here,
               -- and a folder that is not mounted cannot be read (ADR-017).
               AND t.source_id IN (SELECT id FROM sources WHERE kind = 'local')
               AND NOT EXISTS (
                   SELECT 1 FROM audio_features f
                   WHERE f.track_id = t.id AND f.analyzer = ?1 AND f.version >= ?2
               )
               AND (?4 IS NULL OR substr(t.external_id, 1, length(?4)) = ?4)
             ORDER BY t.id
             LIMIT ?3",
        )?;
        let under = folder.map(|f| format!("{}/", f.trim_end_matches('/')));
        let rows = stmt.query_map(params![analyzer, version, limit as i64, under], row_to_track)?;
        rows.collect()
    }

    /// How many present tracks already have that version's analysis.
    pub fn analyzed_count(&self, analyzer: &str, version: i64) -> rusqlite::Result<usize> {
        let n: i64 = self.db.query_row(
            "SELECT count(*) FROM audio_features f
             JOIN tracks t ON t.id = f.track_id AND t.present = 1
             WHERE f.analyzer = ?1 AND f.version >= ?2
               AND t.source_id IN (SELECT id FROM sources WHERE kind = 'local')",
            params![analyzer, version],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Stores what analysis pulled out of a track.
    ///
    /// One row per analyzer, holding the newest version that ran: a better
    /// analyzer replaces what the old one said about that track, and a
    /// different analyzer writes beside it.
    ///
    /// It used to keep a row per version as well. What that bought was
    /// nothing anybody read -- every query here already asked for the newest
    /// -- and what it cost is in `MIGRATION_V9`.
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
             ON CONFLICT(track_id, analyzer) DO UPDATE SET
               version = excluded.version, bpm = excluded.bpm,
               tempo_confidence = excluded.tempo_confidence,
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
             WHERE analyzer = ?1 AND track_id IN ({gaps})",
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
                 WHERE track_id = ?1 AND analyzer = ?2",
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
                 WHERE track_id = ?1 AND analyzer = ?2",
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
    fn analysis_inventory_uses_persisted_versions_and_filters_local_tracks() {
        let lib = sample_library();
        lib.save_features(1, "chilpa", 1, None, None, None, None).unwrap();
        lib.save_features(2, "chilpa", 2, Some(120.0), None, None, None).unwrap();
        lib.save_mood_features(3, 1, [0.5; 6], &[0.0; crate::store::mood::VECTOR_LENGTH]).unwrap();

        let all = lib.analysis_inventory(2, "all", "", 0).unwrap();
        assert_eq!((all.total, all.standard, all.mood, all.matched), (3, 1, 1, 3));
        assert_eq!(all.tracks.iter().filter(|track| track.standard).map(|track| track.id).collect::<Vec<_>>(), [2]);
        assert_eq!(all.tracks.iter().filter(|track| track.mood).map(|track| track.id).collect::<Vec<_>>(), [3]);

        let pending = lib.analysis_inventory(2, "standard-pending", "Cosmo", 0).unwrap();
        assert_eq!(pending.matched, 1);
        assert_eq!(pending.tracks[0].id, 3);
        let page = lib.analysis_inventory(2, "all", "", 2).unwrap();
        assert_eq!(page.tracks.len(), 1);
    }

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
        assert_eq!(map.len(), 2, "3 has no analysis");
        assert_eq!(map[&1].0, Some(129.0), "the newest version");
        assert_eq!(map[&1].1.as_deref(), Some("G · 9B"));
        assert_eq!(map[&1].2, Some(-8.0), "the loudness comes too");
        assert_eq!(map[&2].1, None);
        assert!(lib.features_for(&[], "chilpa").unwrap().is_empty());
    }

    #[test]
    fn what_a_folder_holds_is_pending_apart_from_the_rest() {
        let lib = sample_library();
        lib.db.execute("UPDATE tracks SET external_id = '/new%_/Aqualung.flac' WHERE id = 2", []).unwrap();
        let under = |folder| lib.tracks_pending_analysis_under("chilpa", 1, 50, folder).unwrap().len();
        assert_eq!(under(None), 3, "the whole library");
        assert_eq!(under(Some("/new%_")), 1, "only the folder, its name taken as it is");
        assert_eq!(under(Some("/new%_/")), 1, "with or without its slash");
        assert_eq!(under(Some("/ne")), 0, "a folder is not a prefix of another's name");
    }

}
