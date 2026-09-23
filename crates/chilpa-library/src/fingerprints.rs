//! Con qué se empareja una pista local contra la de un source.
//!
//! Son el ISRC y el MBID de grabación que el escaneo saca de los tags. Es
//! lo único con lo que se puede decir "esta canción también está allá" sin
//! adivinar: emparejar por título junta covers, remasters y homónimos, que
//! es peor que no emparejar.
//!
//! Salen crudos a propósito. Quien empareja los normaliza con
//! `chilpa_plugin::messages`, que es donde vive la única regla; si esta
//! orilla normalizara por su cuenta, dos reglas parecidas no emparejarían
//! nada y nadie se daría cuenta.

use crate::store::Library;
use std::collections::HashMap;

impl Library {
    /// El ISRC y el MBID de varias pistas, en una consulta.
    ///
    /// Las que no traen ninguno de los dos no salen en el mapa: no hay con
    /// qué emparejarlas, y una entrada vacía solo daría trabajo al que lee.
    #[allow(clippy::type_complexity)]
    pub fn fingerprints_for(
        &self,
        track_ids: &[i64],
    ) -> rusqlite::Result<HashMap<i64, (Option<String>, Option<String>)>> {
        let mut output = HashMap::new();
        if track_ids.is_empty() {
            return Ok(output);
        }
        // Los ids son enteros que vienen de la propia base; aun así van como
        // parámetros, para no tener dos formas de armar una consulta.
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
