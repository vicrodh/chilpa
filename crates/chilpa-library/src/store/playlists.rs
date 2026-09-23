//! Las listas guardadas: las hechas a mano y las que trae un source.
//!
//! Una entrada guarda la ruta o el id externo además del track, para que
//! una lista no se rompa porque algo todavía no esté en la biblioteca.

use crate::store::Library;
use rusqlite::{params, OptionalExtension};
use crate::store::now_unix;

/// Una lista de reproducción, de aquí o traída por un source.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredPlaylist {
    pub id: i64,
    /// De dónde viene: "local" o el proveedor que la trajo.
    pub provider: String,
    /// El id que le da su source, vacío si es de aquí.
    pub external_id: String,
    pub name: String,
    pub description: String,
    pub artwork_path: Option<String>,
    pub track_count: usize,
    pub updated_at: i64,
}

/// Una entrada de una lista.
///
/// Guarda la ruta o el id externo además del track: una lista no se rompe
/// porque algo todavía no esté en la biblioteca.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlaylistEntry {
    pub track_id: Option<i64>,
    pub external_id: String,
}

impl Library {
    /// Crea una lista, o devuelve la que ya existe con ese id externo.
    ///
    /// `external_id` vacío quiere decir que la lista es de aquí: dos listas
    /// hechas a mano con el mismo nombre son dos listas distintas, como en
    /// cualquier reproductor.
    pub fn create_playlist(
        &self,
        source_id: i64,
        name: &str,
        external_id: &str,
    ) -> rusqlite::Result<i64> {
        let now = now_unix();
        if !external_id.is_empty() {
            if let Some(id) = self
                .db
                .query_row(
                    "SELECT id FROM playlists WHERE source_id = ?1 AND external_id = ?2",
                    params![source_id, external_id],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
            {
                self.db.execute(
                    "UPDATE playlists SET name = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, name, now],
                )?;
                return Ok(id);
            }
        }
        self.db.execute(
            "INSERT INTO playlists (source_id, external_id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![
                source_id,
                (!external_id.is_empty()).then_some(external_id),
                name,
                now
            ],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    /// Reemplaza el contenido de una lista, en ese orden.
    ///
    /// Se hace en una transacción: una lista a medio escribir es peor que
    /// una lista vieja.
    pub fn set_playlist_entries(
        &self,
        playlist_id: i64,
        entries: &[PlaylistEntry],
    ) -> rusqlite::Result<()> {
        let tx = self.db.unchecked_transaction()?;
        tx.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", params![playlist_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO playlist_tracks (playlist_id, position, track_id, external_id)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (pos, e) in entries.iter().enumerate() {
                stmt.execute(params![playlist_id, pos as i64, e.track_id, e.external_id])?;
            }
        }
        tx.execute(
            "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
            params![playlist_id, now_unix()],
        )?;
        tx.commit()
    }

    /// Las listas, con cuántas pistas trae cada una.
    pub fn playlists(&self) -> rusqlite::Result<Vec<StoredPlaylist>> {
        let mut stmt = self.db.prepare(
            "SELECT p.id, s.provider, COALESCE(p.external_id, ''), p.name, p.description,
                    p.artwork_path, count(pt.position), p.updated_at
             FROM playlists p
             JOIN sources s ON s.id = p.source_id
             LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id
             GROUP BY p.id
             ORDER BY p.updated_at DESC, p.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredPlaylist {
                id: r.get(0)?,
                provider: r.get(1)?,
                external_id: r.get(2)?,
                name: r.get(3)?,
                description: r.get(4)?,
                artwork_path: r.get(5)?,
                track_count: r.get::<_, i64>(6)? as usize,
                updated_at: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// Lo que trae una lista, en su orden.
    pub fn playlist_entries(&self, playlist_id: i64) -> rusqlite::Result<Vec<PlaylistEntry>> {
        let mut stmt = self.db.prepare(
            "SELECT track_id, external_id FROM playlist_tracks
             WHERE playlist_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map(params![playlist_id], |r| {
            Ok(PlaylistEntry { track_id: r.get(0)?, external_id: r.get(1)? })
        })?;
        rows.collect()
    }

    /// Cambia el nombre de una lista.
    pub fn rename_playlist(&self, playlist_id: i64, name: &str) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE playlists SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![playlist_id, name, now_unix()],
        )?;
        Ok(())
    }

    /// Borra una lista. Las pistas no se tocan: una lista es una vista.
    pub fn delete_playlist(&self, playlist_id: i64) -> rusqlite::Result<()> {
        self.db
            .execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", params![playlist_id])?;
        self.db.execute("DELETE FROM playlists WHERE id = ?1", params![playlist_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fixtures::*;

    #[test]
    fn a_playlist_keeps_its_order_and_survives_what_is_missing() {
        let lib = sample_library();
        let source = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(source, "De la mañana", "").unwrap();

        let entries = vec![
            PlaylistEntry { track_id: Some(2), external_id: "/Aqualung.flac".into() },
            // Algo que la lista menciona y todavía no se escanea.
            PlaylistEntry { track_id: None, external_id: "/pendiente.flac".into() },
            PlaylistEntry { track_id: Some(1), external_id: "/Bayou.flac".into() },
        ];
        lib.set_playlist_entries(id, &entries).unwrap();

        let read_back = lib.playlist_entries(id).unwrap();
        assert_eq!(read_back, entries, "el orden es el que se guardó");
        assert_eq!(read_back[1].track_id, None, "lo que no está sigue anotado");

        let lists = lib.playlists().unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].track_count, 3);
        assert_eq!(lists[0].provider, "local");
        assert!(lists[0].external_id.is_empty(), "una lista de aquí no tiene id externo");
    }

    #[test]
    fn fetching_a_source_playlist_again_updates_it_without_duplicating_it() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("servidor", "casa").unwrap();
        let a = lib.create_playlist(source, "Mezcla", "abc123").unwrap();
        let b = lib.create_playlist(source, "Mezcla renombrada", "abc123").unwrap();
        assert_eq!(a, b, "el mismo id externo es la misma lista");
        let lists = lib.playlists().unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].name, "Mezcla renombrada", "se quedó el nombre nuevo");
    }

    #[test]
    fn two_hand_made_playlists_with_the_same_name_are_two_playlists() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let a = lib.create_playlist(source, "Sin nombre", "").unwrap();
        let b = lib.create_playlist(source, "Sin nombre", "").unwrap();
        assert_ne!(a, b);
        assert_eq!(lib.playlists().unwrap().len(), 2);
    }

    #[test]
    fn replacing_the_contents_leaves_no_leftovers() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(source, "Mía", "").unwrap();
        let three: Vec<PlaylistEntry> = (0..3)
            .map(|i| PlaylistEntry { track_id: None, external_id: format!("/{i}.flac") })
            .collect();
        lib.set_playlist_entries(id, &three).unwrap();
        lib.set_playlist_entries(id, &three[..1]).unwrap();
        assert_eq!(lib.playlist_entries(id).unwrap().len(), 1, "no quedaron las viejas");
    }

    #[test]
    fn deleting_a_playlist_does_not_touch_the_tracks() {
        let lib = sample_library();
        let source = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(source, "Temporal", "").unwrap();
        lib.set_playlist_entries(
            id,
            &[PlaylistEntry { track_id: Some(1), external_id: "/Bayou.flac".into() }],
        )
        .unwrap();
        lib.delete_playlist(id).unwrap();
        assert!(lib.playlists().unwrap().is_empty());
        assert_eq!(lib.track_count(false).unwrap(), 3, "las pistas siguen");
    }

}
