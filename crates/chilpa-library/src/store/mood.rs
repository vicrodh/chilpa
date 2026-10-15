//! Local features supplied by an analyzer plugin, and the playlists built
//! from those features when the user runs Mood generation.

use super::{now_unix, row_to_track, Library, PlaylistEntry, StoredTrack};
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

pub const ANALYZER: &str = "app.chilpa.mood";
pub const VECTOR_LENGTH: usize = 1280;
const TRACKS_PER_LIST: usize = 25;
const TRACKS_PER_ARTIST: usize = 4;

#[derive(Clone, Debug)]
pub struct MoodTrack {
    pub id: i64,
    pub source_id: i64,
    pub external_id: String,
    pub title: String,
    pub artist: String,
    pub plays: i64,
    pub scores: [f32; 6],
    pub embedding: Vec<f32>,
}

impl Library {
    pub fn tracks_pending_mood_under(&self, version: i64, folder: Option<&str>) -> rusqlite::Result<Vec<StoredTrack>> {
        let under = folder.map(|path| format!("{}/", path.trim_end_matches('/')));
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, (t.present = 1 AND t.reachable = 1)
             FROM tracks t LEFT JOIN albums a ON a.id = t.album_id
             WHERE t.present = 1 AND t.reachable = 1
               AND t.source_id IN (SELECT id FROM sources WHERE kind = 'local')
               AND NOT EXISTS (SELECT 1 FROM mood_features f WHERE f.track_id = t.id
                               AND f.analyzer = ?1 AND f.version >= ?2)
               AND (?3 IS NULL OR substr(t.external_id, 1, length(?3)) = ?3)
             ORDER BY t.id",
        )?;
        let rows = stmt.query_map(params![ANALYZER, version, under], row_to_track)?;
        rows.collect()
    }

    pub fn save_mood_features(&self, track_id: i64, version: i64, scores: [f32; 6], embedding: &[f32]) -> rusqlite::Result<()> {
        if embedding.len() != VECTOR_LENGTH || !embedding.iter().all(|value| value.is_finite())
            || !scores.iter().all(|value| value.is_finite() && (0.0..=1.0).contains(value)) {
            return Err(rusqlite::Error::InvalidParameterName("invalid analyzer features".into()));
        }
        let mut bytes = Vec::with_capacity(VECTOR_LENGTH * 4);
        for value in embedding { bytes.extend_from_slice(&value.to_le_bytes()); }
        self.db.execute(
            "INSERT INTO mood_features
              (track_id, analyzer, version, mood_happy, mood_sad, mood_relaxed,
               mood_aggressive, mood_party, danceability, embedding, analyzed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(track_id, analyzer) DO UPDATE SET
               version = excluded.version, mood_happy = excluded.mood_happy,
               mood_sad = excluded.mood_sad, mood_relaxed = excluded.mood_relaxed,
               mood_aggressive = excluded.mood_aggressive, mood_party = excluded.mood_party,
               danceability = excluded.danceability, embedding = excluded.embedding,
               analyzed_at = excluded.analyzed_at",
            params![track_id, ANALYZER, version, scores[0], scores[1], scores[2],
                    scores[3], scores[4], scores[5], bytes, now_unix()],
        )?;
        Ok(())
    }

    pub fn mood_tracks(&self, version: i64) -> rusqlite::Result<Vec<MoodTrack>> {
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.source_id, t.external_id, t.title, t.artist,
                    COALESCE(r.plays, 0), f.mood_happy, f.mood_sad, f.mood_relaxed,
                    f.mood_aggressive, f.mood_party, f.danceability, f.embedding
             FROM mood_features f JOIN tracks t ON t.id = f.track_id
             LEFT JOIN track_rollup r ON r.track_id = t.id
             WHERE f.analyzer = ?1 AND f.version >= ?2 AND t.present = 1 AND t.reachable = 1
               AND t.source_id IN (SELECT id FROM sources WHERE kind = 'local')",
        )?;
        let rows = stmt.query_map(params![ANALYZER, version], |row| {
            let bytes: Vec<u8> = row.get(12)?;
            if bytes.len() != VECTOR_LENGTH * 4 {
                return Err(rusqlite::Error::InvalidColumnType(12, "embedding".into(), rusqlite::types::Type::Blob));
            }
            let embedding = bytes.chunks_exact(4).map(|part| f32::from_le_bytes(part.try_into().unwrap())).collect();
            Ok(MoodTrack { id: row.get(0)?, source_id: row.get(1)?, external_id: row.get(2)?,
                title: row.get(3)?, artist: row.get(4)?, plays: row.get(5)?,
                scores: [row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?],
                embedding })
        })?;
        rows.collect()
    }

    /// Rebuild generated lists without changing their IDs or any hand-made list.
    pub fn rebuild_mood_playlists(&self, version: i64) -> rusqlite::Result<usize> {
        let tracks = self.mood_tracks(version)?;
        if tracks.is_empty() { return Ok(0); }
        let folder = match self.db.query_row(
            "SELECT id FROM playlist_folders WHERE name = 'Mood' ORDER BY id LIMIT 1", [], |row| row.get::<_, i64>(0)).optional()? {
            Some(id) => id,
            None => self.create_playlist_folder("Mood", "accent")?,
        };
        let source = self.source_id("local", "default")?;
        let mut lists: Vec<(&str, String, Vec<&MoodTrack>)> = vec![
            ("calm", "Calm".into(), ranked(&tracks, |track| track.scores[2])),
            ("melancholy", "Melancholy".into(), ranked(&tracks, |track| track.scores[1])),
            ("happy", "Happy".into(), ranked(&tracks, |track| track.scores[0])),
            ("party", "Party".into(), ranked(&tracks, |track| (track.scores[4] + track.scores[5]) * 0.5)),
            ("aggressive", "Aggressive".into(), ranked(&tracks, |track| track.scores[3])),
        ];
        let mut seeds: Vec<&MoodTrack> = tracks.iter().filter(|track| track.plays > 0).collect();
        seeds.sort_by(|a, b| b.plays.cmp(&a.plays).then(a.id.cmp(&b.id)));
        for (position, seed) in seeds.into_iter().take(2).enumerate() {
            lists.push((if position == 0 { "like-1" } else { "like-2" },
                        format!("Like {}", seed.title), similar(&tracks, seed)));
        }
        for (key, name, selected) in &lists {
            let id = self.create_playlist(source, name, &format!("{ANALYZER}:{key}"))?;
            self.move_playlist_to_folder(id, Some(folder))?;
            self.db.execute("UPDATE playlists SET description = ?2 WHERE id = ?1",
                params![id, "Generated from local audio by the Mood plugin."])?;
            let entries = selected.iter().map(|track| PlaylistEntry { track_id: Some(track.id),
                source_id: Some(track.source_id), external_id: track.external_id.clone() }).collect::<Vec<_>>();
            self.set_playlist_entries(id, &entries)?;
        }
        Ok(lists.len())
    }
}

fn capped<'a>(ordered: impl IntoIterator<Item = &'a MoodTrack>) -> Vec<&'a MoodTrack> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut selected = Vec::new();
    for track in ordered {
        let count = counts.entry(track.artist.to_lowercase()).or_default();
        if *count >= TRACKS_PER_ARTIST { continue; }
        *count += 1;
        selected.push(track);
        if selected.len() == TRACKS_PER_LIST { break; }
    }
    selected
}

fn ranked(tracks: &[MoodTrack], score: impl Fn(&MoodTrack) -> f32) -> Vec<&MoodTrack> {
    let mut ordered = tracks.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| score(b).total_cmp(&score(a)).then(a.id.cmp(&b.id)));
    capped(ordered)
}

fn similar<'a>(tracks: &'a [MoodTrack], seed: &'a MoodTrack) -> Vec<&'a MoodTrack> {
    let norm = |values: &[f32]| values.iter().map(|value| value * value).sum::<f32>().sqrt();
    let seed_norm = norm(&seed.embedding);
    let mut ordered = tracks.iter().filter(|track| track.id != seed.id).collect::<Vec<_>>();
    ordered.sort_by(|a, b| {
        let cosine = |track: &MoodTrack| {
            let denominator = norm(&track.embedding) * seed_norm;
            if denominator <= f32::EPSILON { return -1.0; }
            track.embedding.iter().zip(&seed.embedding).map(|(x, y)| x * y).sum::<f32>() / denominator
        };
        cosine(b).total_cmp(&cosine(a)).then(a.id.cmp(&b.id))
    });
    capped(std::iter::once(seed).chain(ordered))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        let lib = Library::in_memory().unwrap();
        let local = lib.source_id("local", "default").unwrap();
        let remote = lib.source_id("remote", "account").unwrap();
        for id in 1..=8 {
            let source = if id == 8 { remote } else { local };
            let artist = if id <= 5 { "First" } else { "Second" };
            lib.db.execute(
                "INSERT INTO tracks (id, source_id, external_id, title, artist, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                params![id, source, format!("/music/{id}.flac"), format!("Track {id}"), artist],
            ).unwrap();
        }
        lib.db.execute("INSERT INTO track_rollup (track_id, plays) VALUES (1, 12), (6, 8)", []).unwrap();
        lib
    }

    #[test]
    fn pending_tracks_respect_folder_version_and_local_files() {
        let lib = library();
        assert_eq!(lib.tracks_pending_mood_under(1, Some("/music")).unwrap().len(), 7);
        let vector = vec![1.0; VECTOR_LENGTH];
        lib.save_mood_features(1, 1, [0.5; 6], &vector).unwrap();
        assert_eq!(lib.tracks_pending_mood_under(1, None).unwrap().len(), 6);
        assert_eq!(lib.tracks_pending_mood_under(2, None).unwrap().len(), 7);
        assert!(lib.tracks_pending_mood_under(1, Some("/mus")).unwrap().is_empty());
        assert!(lib.save_mood_features(2, 1, [0.5; 6], &vector[..1]).is_err());
    }

    #[test]
    fn generated_playlists_keep_ids_and_do_not_replace_manual_lists() {
        let lib = library();
        let local = lib.source_id("local", "default").unwrap();
        let manual = lib.create_playlist(local, "My Mood", "").unwrap();
        for id in 1..=7 {
            let mut vector = vec![0.0; VECTOR_LENGTH];
            vector[id as usize] = 1.0;
            lib.save_mood_features(id, 1, [id as f32 / 10.0; 6], &vector).unwrap();
        }
        assert_eq!(lib.rebuild_mood_playlists(1).unwrap(), 7);
        // I exercise a changed recency order without depending on a clock boundary.
        lib.db.execute("UPDATE playlists SET updated_at = id WHERE external_id LIKE ?1",
            params![format!("{ANALYZER}:%")]).unwrap();
        let first = lib.playlists().unwrap();
        let generated = first.iter().filter(|item| item.external_id.starts_with(ANALYZER)).collect::<Vec<_>>();
        assert_eq!(generated.len(), 7);
        assert!(generated.iter().all(|item| item.folder_id.is_some()));
        let calm = generated.iter().find(|item| item.name == "Calm").unwrap();
        let mut stmt = lib.db.prepare("SELECT t.artist FROM playlist_tracks p JOIN tracks t ON t.id = p.track_id
                                      WHERE p.playlist_id = ?1 ORDER BY p.position").unwrap();
        let artists = stmt.query_map(params![calm.id], |row| row.get::<_, String>(0)).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(artists.iter().filter(|artist| *artist == "First").count(), 4);
        let identities = generated.iter().map(|item| (item.external_id.clone(), item.id))
            .collect::<std::collections::BTreeMap<_, _>>();
        lib.rebuild_mood_playlists(1).unwrap();
        let second = lib.playlists().unwrap();
        assert_eq!(second.iter().filter(|item| item.external_id.starts_with(ANALYZER))
                   .map(|item| (item.external_id.clone(), item.id)).collect::<std::collections::BTreeMap<_, _>>(), identities);
        assert!(second.iter().any(|item| item.id == manual && item.name == "My Mood"));
    }
}
