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

/// Lo que el log de reproducciones dice en conjunto.
///
/// Sale del log, no de contadores: un contador se desincroniza y nadie se
/// entera; el log se puede volver a sumar siempre.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Stats {
    /// Escuchas completas: las que sonaron más de la mitad.
    pub plays: usize,
    /// Todo lo que se ha entregado, en milisegundos, completas o no.
    pub listened_ms: u64,
    /// Cuántas pistas distintas se han escuchado.
    pub distinct_tracks: usize,
    /// Cuántas veces se saltó algo.
    pub skips: usize,
    /// Cuántas de las escuchas salieron bit-perfect.
    pub bit_perfect: usize,
    /// Cuándo fue la primera escucha registrada, en segundos unix.
    pub since: Option<i64>,
}

/// Una pista con su cuenta, para los tops y el historial.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TrackStat {
    pub track: StoredTrack,
    /// Escuchas, o el momento de la última, según qué lista sea.
    pub count: usize,
    pub at: Option<i64>,
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

/// Un artista de la biblioteca, con lo que tiene.
///
/// Sale de `albums.album_artist`, que es lo que el disco dice de sí mismo.
/// La tabla `artists` está para cuando haya identidades de MusicBrainz; hoy
/// inventar filas ahí no agregaría nada que el tag no diga.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredArtist {
    pub name: String,
    pub album_count: usize,
    pub track_count: usize,
    /// La portada de uno de sus discos, para no mostrar un hueco.
    pub artwork_path: Option<String>,
}

/// La biblioteca abierta.
pub struct Library {
    pub(crate) db: Connection,
}

/// Cómo ordenar la reja de álbumes.
///
/// Son las vistas rápidas: lo mismo de siempre, mirado de otra forma. Todas
/// salen de la misma consulta, con otro `ORDER BY`; ninguna agrega la
/// biblioteca entera dos veces.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AlbumSort {
    /// Artista, año y título: el orden de una repisa.
    #[default]
    Artist,
    Title,
    /// Del más nuevo al más viejo.
    Year,
    /// Lo último que entró a la biblioteca.
    Added,
    /// Lo más escuchado.
    Played,
    /// Lo último que sonó.
    LastPlayed,
}

impl AlbumSort {
    /// Cómo se llama esta vista, para guardarla y para leerla.
    pub fn name(&self) -> &'static str {
        match self {
            AlbumSort::Artist => "artist",
            AlbumSort::Title => "title",
            AlbumSort::Year => "year",
            AlbumSort::Added => "added",
            AlbumSort::Played => "played",
            AlbumSort::LastPlayed => "last_played",
        }
    }

    /// Lo contrario. Un nombre que no se reconoce da el orden de siempre, no
    /// un error: una preferencia vieja no debe dejar la reja vacía.
    pub fn from_name(name: &str) -> Self {
        match name {
            "title" => AlbumSort::Title,
            "year" => AlbumSort::Year,
            "added" => AlbumSort::Added,
            "played" => AlbumSort::Played,
            "last_played" => AlbumSort::LastPlayed,
            _ => AlbumSort::Artist,
        }
    }

    /// Todas, en el orden en que se ofrecen.
    pub fn all() -> [AlbumSort; 6] {
        [
            AlbumSort::Artist,
            AlbumSort::Title,
            AlbumSort::Year,
            AlbumSort::Added,
            AlbumSort::Played,
            AlbumSort::LastPlayed,
        ]
    }

    /// Cierto si el orden necesita agregar la biblioteca entera para
    /// decidir qué va primero.
    ///
    /// Por artista, título o año se ordena con columnas de `albums` y ya:
    /// se puede recortar la página antes de contar nada. Por escuchas o por
    /// fecha de alta no hay manera, porque el orden sale justo de eso.
    fn needs_adding(&self) -> bool {
        matches!(self, AlbumSort::Added | AlbumSort::Played | AlbumSort::LastPlayed)
    }

    /// El orden cuando solo se miran columnas de `albums`.
    fn cheap_order_by(&self) -> &'static str {
        match self {
            AlbumSort::Title => "a.title COLLATE NOCASE",
            AlbumSort::Year => "a.year DESC, a.title COLLATE NOCASE",
            _ => "a.album_artist COLLATE NOCASE, a.year, a.title COLLATE NOCASE",
        }
    }

    fn order_by(&self) -> &'static str {
        match self {
            AlbumSort::Artist => "a.album_artist COLLATE NOCASE, a.year, a.title COLLATE NOCASE",
            AlbumSort::Title => "a.title COLLATE NOCASE",
            AlbumSort::Year => "a.year DESC, a.title COLLATE NOCASE",
            AlbumSort::Added => "max(t.added_at) DESC, a.title COLLATE NOCASE",
            AlbumSort::Played => "escuchas DESC, a.title COLLATE NOCASE",
            AlbumSort::LastPlayed => "ultima DESC, a.title COLLATE NOCASE",
        }
    }
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

    /// Pistas presentes a las que les falta el análisis de esa versión.
    ///
    /// Se piden por tanda: analizar una biblioteca grande no puede empezar
    /// cargando su lista entera en memoria.
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

    /// Cuántas pistas presentes ya tienen análisis de esa versión.
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

    /// Guarda lo que el análisis sacó de una pista.
    ///
    /// Una fila por analizador y versión: un analizador mejor escribe la
    /// suya y no revuelve la vieja.
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

    /// Lo que el análisis le puso a varias pistas, en una sola consulta.
    ///
    /// Una consulta por renglón de una lista es justo lo que hacía lenta a
    /// la biblioteca de QBZ.
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
        // Los ids son enteros que vienen de la propia base; aun así van como
        // parámetros, para no tener dos formas de armar una consulta.
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
            // Ordenado por versión, así que la última que entra es la más
            // nueva.
            output.insert(id, data);
        }
        Ok(output)
    }

    /// Lo que el análisis le puso a una pista: tempo, tonalidad y sonoridad.
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

    /// El BPM que el análisis le puso a una pista, si se lo puso.
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

    /// El resumen de todo lo que se ha escuchado.
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

    /// Las pistas más escuchadas.
    pub fn top_tracks(&self, limit: usize) -> rusqlite::Result<Vec<TrackStat>> {
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present,
                    count(p.id), max(p.started_at)
             FROM play_events p
             JOIN tracks t ON t.id = p.track_id
             LEFT JOIN albums a ON a.id = t.album_id
             WHERE p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms
             GROUP BY t.id
             ORDER BY count(p.id) DESC, max(p.started_at) DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_track_stat)?;
        rows.collect()
    }

    /// Lo último que sonó, sin repetir pista.
    pub fn recently_played(&self, limit: usize) -> rusqlite::Result<Vec<TrackStat>> {
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present,
                    count(p.id), max(p.started_at) AS ultima
             FROM play_events p
             JOIN tracks t ON t.id = p.track_id
             LEFT JOIN albums a ON a.id = t.album_id
             GROUP BY t.id
             ORDER BY ultima DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_track_stat)?;
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
        let rows = stmt.query_map(params![limit as i64, offset as i64], |r| {
            Ok(StoredAlbum {
                id: r.get(0)?,
                title: r.get(1)?,
                album_artist: r.get(2)?,
                year: r.get(3)?,
                artwork_path: r.get(4)?,
                track_count: r.get::<_, i64>(5)? as usize,
            })
        })?;
        rows.collect()
    }

    /// Los álbumes con su cuenta de escuchas, en el orden que se pida.
    ///
    /// Una sola consulta: las escuchas salen de un `GROUP BY` sobre el log
    /// unido una vez, no de una consulta por álbum. Preguntarle a la base
    /// una vez por cada fila de una reja es justo lo que hacía lenta a la
    /// biblioteca de QBZ.
    pub fn albums_sorted(
        &self,
        order: AlbumSort,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        self.albums_filtered(order, None, limit, offset)
    }

    /// Lo mismo, pero solo los de un artista cuando se pide uno.
    pub fn albums_filtered(
        &self,
        order: AlbumSort,
        artist: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        if !order.needs_adding() {
            return self.albums_page_cheap(order, artist, limit, offset);
        }
        // El filtro va como parámetro, nunca interpolado: un nombre con
        // comillas no tiene por qué romper una consulta.
        let sql = format!(
            "SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path,
                    count(t.id),
                    COALESCE(e.escuchas, 0) AS escuchas,
                    e.ultima AS ultima
             FROM albums a
             JOIN tracks t ON t.album_id = a.id AND t.present = 1
             LEFT JOIN (
                 SELECT t2.album_id AS aid,
                        count(*) AS escuchas,
                        max(p.started_at) AS ultima
                 FROM play_events p
                 JOIN tracks t2 ON t2.id = p.track_id
                 WHERE p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms
                 GROUP BY t2.album_id
             ) e ON e.aid = a.id
             WHERE ?3 IS NULL OR a.album_artist = ?3 COLLATE NOCASE
             GROUP BY a.id
             ORDER BY {}
             LIMIT ?1 OFFSET ?2",
            order.order_by()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64, offset as i64, artist], |r| {
            Ok((
                StoredAlbum {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    album_artist: r.get(2)?,
                    year: r.get(3)?,
                    artwork_path: r.get(4)?,
                    track_count: r.get::<_, i64>(5)? as usize,
                },
                r.get::<_, i64>(6)? as usize,
            ))
        })?;
        rows.collect()
    }

    /// La reja cuando el orden no depende de agregar nada.
    ///
    /// Se recorta la página primero y solo después se cuentan pistas y
    /// escuchas, para las cien filas que van a salir. Contar la biblioteca
    /// entera en cada página era el hallazgo alto de la auditoría de QBZ, y
    /// aquí se nota: de 137 ms a unos pocos, con cincuenta mil pistas.
    fn albums_page_cheap(
        &self,
        order: AlbumSort,
        artist: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        let sql = format!(
            "WITH pagina AS MATERIALIZED (
                 SELECT a.id AS id
                 FROM albums a
                 WHERE (?3 IS NULL OR a.album_artist = ?3 COLLATE NOCASE)
                   AND EXISTS (SELECT 1 FROM tracks t WHERE t.album_id = a.id AND t.present = 1)
                 ORDER BY {}
                 LIMIT ?1 OFFSET ?2
             )
             SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path,
                    (SELECT count(*) FROM tracks t
                     WHERE t.album_id = a.id AND t.present = 1),
                    (SELECT count(*) FROM play_events p
                     JOIN tracks t2 ON t2.id = p.track_id
                     WHERE t2.album_id = a.id
                       AND (p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms))
             FROM pagina g
             JOIN albums a ON a.id = g.id
             ORDER BY {}",
            order.cheap_order_by(),
            order.cheap_order_by()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64, offset as i64, artist], |r| {
            Ok((
                StoredAlbum {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    album_artist: r.get(2)?,
                    year: r.get(3)?,
                    artwork_path: r.get(4)?,
                    track_count: r.get::<_, i64>(5)? as usize,
                },
                r.get::<_, i64>(6)? as usize,
            ))
        })?;
        rows.collect()
    }

    /// Los artistas de la biblioteca, por nombre.
    ///
    /// Un artista sin pistas presentes no aparece: la vista muestra lo que
    /// se puede poner a sonar, no lo que alguna vez estuvo.
    pub fn artists(&self, limit: usize, offset: usize) -> rusqlite::Result<Vec<StoredArtist>> {
        let mut stmt = self.db.prepare(
            "SELECT a.album_artist,
                    count(DISTINCT a.id),
                    count(t.id),
                    max(a.artwork_path)
             FROM albums a
             JOIN tracks t ON t.album_id = a.id AND t.present = 1
             WHERE a.album_artist <> ''
             GROUP BY a.album_artist COLLATE NOCASE
             ORDER BY a.album_artist COLLATE NOCASE
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], |r| {
            Ok(StoredArtist {
                name: r.get(0)?,
                album_count: r.get::<_, i64>(1)? as usize,
                track_count: r.get::<_, i64>(2)? as usize,
                artwork_path: r.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// Un álbum por su id, con su cuenta de escuchas.
    pub fn album(&self, album_id: i64) -> rusqlite::Result<Option<(StoredAlbum, usize)>> {
        let album = self
            .db
            .query_row(
                "SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path, count(t.id)
                 FROM albums a
                 JOIN tracks t ON t.album_id = a.id AND t.present = 1
                 WHERE a.id = ?1
                 GROUP BY a.id",
                params![album_id],
                |r| {
                    Ok(StoredAlbum {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        album_artist: r.get(2)?,
                        year: r.get(3)?,
                        artwork_path: r.get(4)?,
                        track_count: r.get::<_, i64>(5)? as usize,
                    })
                },
            )
            .optional()?;
        match album {
            Some(a) => {
                let listens = self.album_play_count(album_id)?;
                Ok(Some((a, listens)))
            }
            None => Ok(None),
        }
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
        let rows = stmt.query_map(params![album_id], row_to_track)?;
        rows.collect()
    }

    /// Los álbumes más escuchados, con su cuenta de reproducciones.
    ///
    /// Cuenta solo lo que sonó más de la mitad, igual que
    /// [`Library::play_count`]. Es la primera smart list: sale del log de
    /// eventos, no de un contador que haya que mantener a mano.
    pub fn most_played_albums(&self, limit: usize) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        let mut stmt = self.db.prepare(
            "SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path,
                    count(DISTINCT t.id), count(p.id)
             FROM albums a
             JOIN tracks t ON t.album_id = a.id
             JOIN play_events p ON p.track_id = t.id
                  AND (p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms)
             GROUP BY a.id
             ORDER BY count(p.id) DESC, a.title COLLATE NOCASE
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok((
                StoredAlbum {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    album_artist: r.get(2)?,
                    year: r.get(3)?,
                    artwork_path: r.get(4)?,
                    track_count: r.get::<_, i64>(5)? as usize,
                },
                r.get::<_, i64>(6)? as usize,
            ))
        })?;
        rows.collect()
    }

    /// Cuántas veces se ha escuchado cada pista de un álbum.
    pub fn album_play_count(&self, album_id: i64) -> rusqlite::Result<usize> {
        let n: i64 = self.db.query_row(
            "SELECT count(*) FROM play_events p
             JOIN tracks t ON t.id = p.track_id
             WHERE t.album_id = ?1
               AND (p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms)",
            params![album_id],
            |r| r.get(0),
        )?;
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

fn now_unix() -> i64 {
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

    /// Tres álbumes con distinto artista, año, fecha de alta y escuchas.
    fn sample_library() -> Library {
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

    fn titulos(lib: &Library, order: AlbumSort) -> Vec<String> {
        lib.albums_sorted(order, 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, _)| a.title)
            .collect()
    }

    #[test]
    fn each_view_sorts_the_grid_its_own_way() {
        let lib = sample_library();
        assert_eq!(
            titulos(&lib, AlbumSort::Artist),
            ["Bayou", "Cosmo", "Aqualung"],
            "artista, luego año"
        );
        assert_eq!(titulos(&lib, AlbumSort::Title), ["Aqualung", "Bayou", "Cosmo"]);
        assert_eq!(titulos(&lib, AlbumSort::Year), ["Aqualung", "Cosmo", "Bayou"], "del más nuevo");
        assert_eq!(titulos(&lib, AlbumSort::Added), ["Aqualung", "Cosmo", "Bayou"], "lo último que entró");
        assert_eq!(titulos(&lib, AlbumSort::Played), ["Cosmo", "Bayou", "Aqualung"]);
        assert_eq!(
            titulos(&lib, AlbumSort::LastPlayed),
            ["Cosmo", "Bayou", "Aqualung"],
            "lo último que sonó, y lo que nunca sonó al final"
        );
    }

    #[test]
    fn the_grid_brings_play_counts_in_the_same_query() {
        let lib = sample_library();
        let listens: Vec<(String, usize)> = lib
            .albums_sorted(AlbumSort::Played, 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, n)| (a.title, n))
            .collect();
        assert_eq!(
            listens,
            vec![("Cosmo".into(), 2), ("Bayou".into(), 1), ("Aqualung".into(), 0)]
        );
    }

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

        let leidas = lib.playlist_entries(id).unwrap();
        assert_eq!(leidas, entries, "el orden es el que se guardó");
        assert_eq!(leidas[1].track_id, None, "lo que no está sigue anotado");

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

    #[test]
    fn the_stats_come_from_the_whole_log() {
        let lib = sample_library();
        let s = lib.stats().unwrap();
        assert_eq!(s.plays, 3, "tres escuchas completas");
        assert_eq!(s.distinct_tracks, 2, "de dos pistas distintas");
        assert_eq!(s.listened_ms, 3_000);
        assert_eq!(s.bit_perfect, 3);
        assert_eq!(s.skips, 0);
        assert_eq!(s.since, Some(10), "la primera fue en el segundo 10");
    }

    #[test]
    fn without_a_single_play_the_stats_are_zeros() {
        let lib = Library::in_memory().unwrap();
        let s = lib.stats().unwrap();
        assert_eq!(s, Stats::default(), "ceros, no un error");
    }

    #[test]
    fn the_top_and_the_history_look_at_the_same_thing_from_different_sides() {
        let lib = sample_library();
        // La pista del álbum 3 suena dos veces; la del 1, una, y antes.
        let top = lib.top_tracks(10).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].count, 2, "primero la más escuchada");
        assert_eq!(top[1].count, 1);

        let recent = lib.recently_played(10).unwrap();
        assert_eq!(recent[0].at, Some(30), "primero la última que sonó");
        assert_eq!(recent[1].at, Some(10));
        assert_eq!(recent.len(), 2, "una pista por renglón, no una por escucha");
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

    #[test]
    fn features_for_several_tracks_come_out_in_one_query() {
        let lib = sample_library();
        lib.save_features(1, "chilpa", 1, Some(128.0), None, Some("C · 8B".into()), Some(-9.5))
            .unwrap();
        lib.save_features(2, "chilpa", 1, Some(90.0), None, None, None).unwrap();
        // Una versión más nueva de la misma pista gana.
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

    #[test]
    fn a_single_album_brings_its_own_and_its_plays() {
        let lib = sample_library();
        let (album, listens) = lib.album(3).unwrap().expect("el álbum existe");
        assert_eq!(album.title, "Cosmo");
        assert_eq!(album.album_artist, "Creedence");
        assert_eq!(album.track_count, 1);
        assert_eq!(listens, 2);
        assert!(lib.album(99).unwrap().is_none(), "uno que no existe no es un error");
    }

    #[test]
    fn artists_come_out_with_what_they_have() {
        let lib = sample_library();
        let artists = lib.artists(50, 0).unwrap();
        assert_eq!(
            artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["Creedence", "Tull"],
            "por nombre"
        );
        assert_eq!(artists[0].album_count, 2, "Creedence trae dos discos");
        assert_eq!(artists[0].track_count, 2, "con una pista cada uno");
        assert_eq!(artists[1].album_count, 1);
    }

    #[test]
    fn the_grid_can_be_filtered_by_artist() {
        let lib = sample_library();
        let own: Vec<String> = lib
            .albums_filtered(AlbumSort::Title, Some("Creedence"), 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, _)| a.title)
            .collect();
        assert_eq!(own, ["Bayou", "Cosmo"]);

        // Sin filtro, salen todos.
        assert_eq!(lib.albums_filtered(AlbumSort::Title, None, 50, 0).unwrap().len(), 3);
        // Y un nombre que no existe no trae nada, sin tronar.
        assert!(lib.albums_filtered(AlbumSort::Title, Some("Nadie"), 50, 0).unwrap().is_empty());
    }

    #[test]
    fn a_name_with_quotes_does_not_break_the_query() {
        let lib = sample_library();
        let odd = "O'Connor'; DROP TABLE albums; --";
        assert!(lib.albums_filtered(AlbumSort::Title, Some(odd), 50, 0).unwrap().is_empty());
        assert_eq!(lib.albums_sorted(AlbumSort::Title, 50, 0).unwrap().len(), 3, "la tabla sigue");
    }

    #[test]
    fn an_unknown_view_name_does_not_leave_the_grid_empty() {
        assert_eq!(AlbumSort::from_name("lo que sea"), AlbumSort::Artist);
        for v in AlbumSort::all() {
            assert_eq!(AlbumSort::from_name(v.name()), v, "{} va y viene", v.name());
        }
    }

    #[test]
    fn the_most_played_come_from_the_log_and_in_order() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (album, title) in [(1, "Uno"), (2, "Dos")] {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist) VALUES (?1, ?2, 'Artista')",
                    params![album, title],
                )
                .unwrap();
            lib.db
                .execute(
                    "INSERT INTO tracks (source_id, external_id, title, album_id, duration_ms, added_at)
                     VALUES (?1, ?2, 'Pista', ?3, 1000, 0)",
                    params![source, format!("/{title}.flac"), album],
                )
                .unwrap();
        }
        // El álbum 2 se escucha tres veces; el 1, una.
        lib.log_play(Some(1), Some(source), 1, 1000, 1000, false, None, None, true).unwrap();
        for i in 0..3 {
            lib.log_play(Some(2), Some(source), 10 + i, 1000, 1000, false, None, None, true)
                .unwrap();
        }
        // Y una a medias, que no cuenta.
        lib.log_play(Some(1), Some(source), 20, 100, 1000, true, None, None, true).unwrap();

        let top = lib.most_played_albums(10).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0.title, "Dos");
        assert_eq!(top[0].1, 3);
        assert_eq!(top[1].1, 1, "la escucha a medias no suma");
        assert_eq!(lib.album_play_count(2).unwrap(), 3);
    }

    #[test]
    fn a_half_played_track_does_not_count_as_a_play() {
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
