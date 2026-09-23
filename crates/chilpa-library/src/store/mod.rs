//! La base de datos abierta, y lo que se le pregunta.
//!
//! Cada tema tiene su archivo: la reja de álbumes, las listas, lo que el
//! análisis saca, el log de reproducciones y las huellas para emparejar.
//! Aquí quedan la conexión, los sources y roots, los géneros y la búsqueda.

pub mod albums;
pub mod features;
pub mod fingerprints;
pub mod playlists;
pub mod plays;

pub use albums::{AlbumSort, StoredAlbum, StoredArtist};
pub use playlists::{PlaylistEntry, StoredPlaylist};
pub use plays::{Stats, TrackStat};

use crate::schema;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// Un track como lo guarda la biblioteca.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredTrack {
    pub id: i64,
    pub external_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u16>,
    pub duration_ms: u64,
    pub present: bool,
}


/// Una carpeta que forma parte de la biblioteca.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredRoot {
    pub id: i64,
    pub path: std::path::PathBuf,
    /// Cuándo se escaneó por última vez, en segundos unix.
    pub last_scan: Option<i64>,
    pub reachable: bool,
}


/// La biblioteca abierta.
pub struct Library {
    pub(crate) db: Connection,
}

impl Library {
    /// La conexión, para las pruebas que necesitan sembrar datos de golpe.
    ///
    /// No es para la aplicación: lo que la aplicación necesita sale por un
    /// método con nombre, no por SQL suelto.
    #[doc(hidden)]
    pub fn connection(&self) -> &Connection {
        &self.db
    }

    /// Abre (o crea) la biblioteca en ese archivo.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let db = Connection::open(path)?;
        schema::apply_pragmas(&db)?;
        schema::migrate(&db)?;
        Ok(Self { db })
    }

    /// Una biblioteca en memoria, para pruebas.
    pub fn in_memory() -> rusqlite::Result<Self> {
        let db = Connection::open_in_memory()?;
        schema::migrate(&db)?;
        Ok(Self { db })
    }

    /// Dónde vive la biblioteca de este usuario.
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
            })
            .unwrap_or_else(std::env::temp_dir);
        base.join("chilpa").join("library.db")
    }

    /// El id del source, creándolo si hace falta.
    pub fn source_id(&self, provider: &str, instance: &str) -> rusqlite::Result<i64> {
        self.db.execute(
            "INSERT OR IGNORE INTO sources (provider, instance) VALUES (?1, ?2)",
            params![provider, instance],
        )?;
        self.db.query_row(
            "SELECT id FROM sources WHERE provider = ?1 AND instance = ?2",
            params![provider, instance],
            |r| r.get(0),
        )
    }

    /// El id del root, creándolo si hace falta.
    pub fn root_id(&self, source_id: i64, path: &Path) -> rusqlite::Result<i64> {
        let path = path.display().to_string();
        self.db.execute(
            "INSERT OR IGNORE INTO roots (source_id, path) VALUES (?1, ?2)",
            params![source_id, path],
        )?;
        self.db
            .query_row("SELECT id FROM roots WHERE path = ?1", params![path], |r| r.get(0))
    }

    /// Deja esos géneros como los de una pista, quitando los que ya no.
    ///
    /// Se reemplazan, no se suman: si un archivo se reetiquetó, los géneros
    /// viejos ya no son suyos.
    pub fn set_track_genres(&self, track_id: i64, genres: &[String]) -> rusqlite::Result<()> {
        let tx = self.db.unchecked_transaction()?;
        tx.execute("DELETE FROM track_genres WHERE track_id = ?1", params![track_id])?;
        {
            let mut insert = tx.prepare("INSERT OR IGNORE INTO genres (name) VALUES (?1)")?;
            let mut find = tx.prepare("SELECT id FROM genres WHERE name = ?1")?;
            let mut link = tx.prepare(
                "INSERT OR IGNORE INTO track_genres (track_id, genre_id) VALUES (?1, ?2)",
            )?;
            for name in genres {
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                insert.execute(params![name])?;
                let id: i64 = find.query_row(params![name], |r| r.get(0))?;
                link.execute(params![track_id, id])?;
            }
        }
        tx.commit()
    }

    /// Los géneros de la biblioteca, con cuántas pistas presentes tienen.
    pub fn genres(&self) -> rusqlite::Result<Vec<(String, usize)>> {
        let mut stmt = self.db.prepare(
            "SELECT g.name, count(t.id)
             FROM genres g
             JOIN track_genres tg ON tg.genre_id = g.id
             JOIN tracks t ON t.id = tg.track_id AND t.present = 1
             GROUP BY g.id
             ORDER BY count(t.id) DESC, g.name COLLATE NOCASE",
        )?;
        let rows =
            stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
        rows.collect()
    }

    /// Los géneros de una pista.
    pub fn track_genres(&self, track_id: i64) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.db.prepare(
            "SELECT g.name FROM genres g
             JOIN track_genres tg ON tg.genre_id = g.id
             WHERE tg.track_id = ?1
             ORDER BY g.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![track_id], |r| r.get(0))?;
        rows.collect()
    }

    /// Las carpetas que se han escaneado alguna vez, con cuándo fue.
    ///
    /// Es lo que hace falta para volver a escanear sin que el usuario tenga
    /// que acordarse de dónde está su música.
    pub fn roots(&self) -> rusqlite::Result<Vec<StoredRoot>> {
        let mut stmt = self.db.prepare(
            "SELECT id, path, last_scan, reachable FROM roots ORDER BY path COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredRoot {
                id: r.get(0)?,
                path: r.get::<_, String>(1)?.into(),
                last_scan: r.get(2)?,
                reachable: r.get::<_, i64>(3)? != 0,
            })
        })?;
        rows.collect()
    }

    /// Marca si un root está alcanzable. Un root ausente NO poda nada.
    pub fn set_root_reachable(&self, root_id: i64, reachable: bool) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE roots SET reachable = ?2 WHERE id = ?1",
            params![root_id, i64::from(reachable)],
        )?;
        if !reachable {
            // Lo suyo queda marcado como ausente, no borrado.
            self.db.execute(
                "UPDATE tracks SET present = 0 WHERE id IN
                 (SELECT track_id FROM scanned_files WHERE root_id = ?1 AND track_id IS NOT NULL)",
                params![root_id],
            )?;
        }
        Ok(())
    }

    /// Cuántos tracks hay, contando o no los ausentes.
    pub fn track_count(&self, include_absent: bool) -> rusqlite::Result<usize> {
        let sql = if include_absent {
            "SELECT count(*) FROM tracks"
        } else {
            "SELECT count(*) FROM tracks WHERE present = 1"
        };
        let n: i64 = self.db.query_row(sql, [], |r| r.get(0))?;
        Ok(n as usize)
    }


    /// Busca por título, artista o álbum.
    pub fn search(&self, text: &str, limit: usize) -> rusqlite::Result<Vec<StoredTrack>> {
        let patron = format!("%{text}%");
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present
             FROM tracks t LEFT JOIN albums a ON a.id = t.album_id
             WHERE t.present = 1
               AND (t.title LIKE ?1 OR t.artist LIKE ?1 OR a.title LIKE ?1
                    OR EXISTS (SELECT 1 FROM track_genres tg
                               JOIN genres g ON g.id = tg.genre_id
                               WHERE tg.track_id = t.id AND g.name LIKE ?1))
             ORDER BY t.artist COLLATE NOCASE, t.title COLLATE NOCASE
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![patron, limit as i64], row_to_track)?;
        rows.collect()
    }


    /// El track de esa ruta, si está en la biblioteca.
    pub fn track_by_path(&self, source_id: i64, path: &Path) -> rusqlite::Result<Option<i64>> {
        self.db
            .query_row(
                "SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2",
                params![source_id, path.display().to_string()],
                |r| r.get(0),
            )
            .optional()
    }
}

pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn row_to_track(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredTrack> {
    Ok(StoredTrack {
        id: r.get(0)?,
        external_id: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        track_number: r.get::<_, Option<i64>>(5)?.map(|n| n as u16),
        duration_ms: r.get::<_, i64>(6)? as u64,
        present: r.get::<_, i64>(7)? == 1,
    })
}

/// Lo que las pruebas de todos los temas siembran, en un solo lugar: si
/// cada archivo armara su biblioteca de muestra, al cambiar el esquema
/// habría que arreglar cinco.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Tres álbumes con distinto artista, año, fecha de alta y escuchas.
    pub(crate) fn sample_library() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        // (id, título, artista, año, added_at)
        for (id, title, artist, año, high) in [
            (1, "Bayou", "Creedence", 1969, 100),
            (2, "Aqualung", "Tull", 1971, 300),
            (3, "Cosmo", "Creedence", 1970, 200),
        ] {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist, year)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, title, artist, año],
                )
                .unwrap();
            lib.db
                .execute(
                    "INSERT INTO tracks (source_id, external_id, title, album_id, duration_ms, added_at)
                     VALUES (?1, ?2, 'Pista', ?3, 1000, ?4)",
                    params![source, format!("/{title}.flac"), id, high],
                )
                .unwrap();
        }
        // El 3 se escucha dos veces, y la última de todas; el 1, una.
        lib.log_play(Some(1), Some(source), 10, 1000, 1000, false, None, None, true).unwrap();
        lib.log_play(Some(3), Some(source), 20, 1000, 1000, false, None, None, true).unwrap();
        lib.log_play(Some(3), Some(source), 30, 1000, 1000, false, None, None, true).unwrap();
        lib
    }

    pub(crate) fn titles(lib: &Library, order: AlbumSort) -> Vec<String> {
        lib.albums_sorted(order, 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, _)| a.title)
            .collect()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use fixtures::*;

    #[test]
    fn the_source_and_the_root_are_created_once() {
        let lib = Library::in_memory().unwrap();
        let a = lib.source_id("local", "default").unwrap();
        let b = lib.source_id("local", "default").unwrap();
        assert_eq!(a, b);
        let r1 = lib.root_id(a, Path::new("/musica")).unwrap();
        let r2 = lib.root_id(a, Path::new("/musica")).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn an_unreachable_root_marks_its_tracks_but_does_not_delete_them() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let root = lib.root_id(source, Path::new("/nas")).unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (source_id, external_id, title, added_at)
                 VALUES (?1, '/nas/a.flac', 'Una', 0)",
                params![source],
            )
            .unwrap();
        lib.db
            .execute(
                "INSERT INTO scanned_files (root_id, path, mtime, size, track_id, scanned_at)
                 VALUES (?1, '/nas/a.flac', 0, 0, 1, 0)",
                params![root],
            )
            .unwrap();

        lib.set_root_reachable(root, false).unwrap();

        assert_eq!(lib.track_count(true).unwrap(), 1, "la fila sigue ahí");
        assert_eq!(lib.track_count(false).unwrap(), 0, "pero ya no cuenta como presente");
    }


    #[test]
    fn search_looks_at_the_genre_too() {
        let lib = sample_library();
        // La pista del álbum 1 es de blues; las demás, no.
        let id: i64 = lib
            .db
            .query_row("SELECT id FROM tracks WHERE album_id = 1", [], |r| r.get(0))
            .unwrap();
        lib.set_track_genres(id, &["Delta Blues".to_string()]).unwrap();

        let found = lib.search("blues", 50).unwrap();
        assert_eq!(found.len(), 1, "salió por su género, no por su título");
        assert_eq!(found[0].id, id);
    }

}
