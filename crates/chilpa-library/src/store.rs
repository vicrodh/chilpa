//! La base de datos abierta, y lo que se le pregunta.

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

/// Un álbum, ya juntado.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredAlbum {
    pub id: i64,
    pub title: String,
    pub album_artist: String,
    pub year: Option<i32>,
    pub artwork_path: Option<String>,
    pub track_count: usize,
}

/// La biblioteca abierta.
pub struct Library {
    pub(crate) db: Connection,
}

impl Library {
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
        let ruta = path.display().to_string();
        self.db.execute(
            "INSERT OR IGNORE INTO roots (source_id, path) VALUES (?1, ?2)",
            params![source_id, ruta],
        )?;
        self.db
            .query_row("SELECT id FROM roots WHERE path = ?1", params![ruta], |r| r.get(0))
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

    /// Los álbumes, ordenados por artista y título, con su cuenta de pistas.
    ///
    /// Una sola consulta con un `GROUP BY`: en QBZ cada página volvía a
    /// agregar la biblioteca entera, y ese era el hallazgo alto de la
    /// auditoría.
    pub fn albums(&self, limit: usize, offset: usize) -> rusqlite::Result<Vec<StoredAlbum>> {
        let mut stmt = self.db.prepare(
            "SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path, count(t.id)
             FROM albums a
             JOIN tracks t ON t.album_id = a.id AND t.present = 1
             GROUP BY a.id
             ORDER BY a.album_artist COLLATE NOCASE, a.year, a.title COLLATE NOCASE
             LIMIT ?1 OFFSET ?2",
        )?;
        let filas = stmt.query_map(params![limit as i64, offset as i64], |r| {
            Ok(StoredAlbum {
                id: r.get(0)?,
                title: r.get(1)?,
                album_artist: r.get(2)?,
                year: r.get(3)?,
                artwork_path: r.get(4)?,
                track_count: r.get::<_, i64>(5)? as usize,
            })
        })?;
        filas.collect()
    }

    /// Las pistas de un álbum, en el orden del disco.
    pub fn album_tracks(&self, album_id: i64) -> rusqlite::Result<Vec<StoredTrack>> {
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present
             FROM tracks t LEFT JOIN albums a ON a.id = t.album_id
             WHERE t.album_id = ?1
             ORDER BY t.disc_number, t.track_number, t.title COLLATE NOCASE",
        )?;
        let filas = stmt.query_map(params![album_id], fila_a_track)?;
        filas.collect()
    }

    /// Busca por título, artista o álbum.
    pub fn search(&self, texto: &str, limit: usize) -> rusqlite::Result<Vec<StoredTrack>> {
        let patron = format!("%{texto}%");
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present
             FROM tracks t LEFT JOIN albums a ON a.id = t.album_id
             WHERE t.present = 1
               AND (t.title LIKE ?1 OR t.artist LIKE ?1 OR a.title LIKE ?1)
             ORDER BY t.artist COLLATE NOCASE, t.title COLLATE NOCASE
             LIMIT ?2",
        )?;
        let filas = stmt.query_map(params![patron, limit as i64], fila_a_track)?;
        filas.collect()
    }

    /// Guarda una reproducción. El log es append-only.
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

    /// Cuántas veces se ha reproducido un track, contando solo lo que sonó
    /// más de la mitad. Es la definición que usan las stats.
    pub fn play_count(&self, track_id: i64) -> rusqlite::Result<usize> {
        let n: i64 = self.db.query_row(
            "SELECT count(*) FROM play_events
             WHERE track_id = ?1 AND (duration_ms = 0 OR played_ms * 2 >= duration_ms)",
            params![track_id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
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

fn fila_a_track(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredTrack> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_source_y_el_root_se_crean_una_sola_vez() {
        let lib = Library::in_memory().unwrap();
        let a = lib.source_id("local", "default").unwrap();
        let b = lib.source_id("local", "default").unwrap();
        assert_eq!(a, b);
        let r1 = lib.root_id(a, Path::new("/musica")).unwrap();
        let r2 = lib.root_id(a, Path::new("/musica")).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn un_root_inalcanzable_marca_sus_tracks_pero_no_los_borra() {
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
    fn una_reproduccion_a_medias_no_cuenta_como_escucha() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (source_id, external_id, title, duration_ms, added_at)
                 VALUES (?1, '/a.flac', 'Una', 200000, 0)",
                params![source],
            )
            .unwrap();
        lib.log_play(Some(1), Some(source), 1, 20_000, 200_000, true, Some(44_100), Some(16), true)
            .unwrap();
        assert_eq!(lib.play_count(1).unwrap(), 0, "20 s de 200 no es una escucha");
        lib.log_play(Some(1), Some(source), 2, 150_000, 200_000, false, Some(44_100), Some(16), true)
            .unwrap();
        assert_eq!(lib.play_count(1).unwrap(), 1);
    }
}
