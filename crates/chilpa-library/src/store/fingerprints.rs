//! What a local track is matched against a source's with.
//!
//! The ISRC and the recording MBID the scan pulls out of the tags. They are
//! the only thing that can say "this song is also over there" without
//! guessing: matching by title lumps covers, remasters and namesakes
//! together, which is worse than not matching at all.
//!
//! They come out raw on purpose. Whoever matches normalizes them with
//! `chilpa_plugin::messages`, which is where the one rule lives; if this
//! side normalized on its own, two similar rules would match nothing and
//! nobody would notice.

use crate::store::Library;
use std::collections::HashMap;

impl Library {
    /// The ISRC and MBID of several tracks, in one query.
    ///
    /// The ones carrying neither do not appear in the map: there is nothing to
    /// match them with, and an empty entry would only give the reader work.
    #[allow(clippy::type_complexity)]
    pub fn fingerprints_for(
        &self,
        track_ids: &[i64],
    ) -> rusqlite::Result<HashMap<i64, (Option<String>, Option<String>)>> {
        let mut output = HashMap::new();
        if track_ids.is_empty() {
            return Ok(output);
        }
        // The ids are integers that came from the database itself; even so
        // they go as parameters, to avoid two ways of building a query.
        let gaps = vec!["?"; track_ids.len()].join(",");
        let sql = format!(
            "SELECT id, isrc, recording_mbid FROM tracks
             WHERE id IN ({gaps}) AND (isrc IS NOT NULL OR recording_mbid IS NOT NULL)",
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(track_ids), |r| {
            Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?)))
        })?;
        for row in rows {
            let (id, pair) = row?;
            output.insert(id, pair);
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn track(lib: &Library, source: i64, path: &str, isrc: Option<&str>, mbid: Option<&str>) {
        lib.db
            .execute(
                "INSERT INTO tracks (source_id, external_id, title, isrc, recording_mbid,
                                     present, added_at)
                 VALUES (?1, ?2, 'Pista', ?3, ?4, 1, 0)",
                params![source, path, isrc, mbid],
            )
            .unwrap();
    }

    #[test]
    fn the_fingerprints_of_several_tracks_come_out_in_one_query() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        track(&lib, source, "/a.flac", Some("USRC17607839"), None);
        track(&lib, source, "/b.flac", None, Some("b1a9c0e9-d987-4042-ae91-78d6a3267d69"));
        track(
            &lib,
            source,
            "/c.flac",
            Some("GBAYE0601498"),
            Some("9e53f34b-0b06-4c5c-9d67-9e8e36c3ff56"),
        );
        track(&lib, source, "/d.flac", None, None);

        let fp = lib.fingerprints_for(&[1, 2, 3, 4]).unwrap();
        assert_eq!(fp.len(), 3, "la que no trae ninguno no ocupa lugar");
        assert_eq!(fp[&1], (Some("USRC17607839".into()), None));
        assert_eq!(fp[&2].0, None, "esa solo trae MBID");
        assert!(fp[&2].1.is_some());
        assert_eq!(fp[&3].0.as_deref(), Some("GBAYE0601498"), "puede traer los dos");
        assert!(fp[&3].1.is_some());
        assert!(!fp.contains_key(&4), "sin ninguno de los dos no hay con qué emparejar");
    }

    #[test]
    fn without_ids_the_database_is_not_touched() {
        let lib = Library::in_memory().unwrap();
        assert!(lib.fingerprints_for(&[]).unwrap().is_empty());
    }
}
