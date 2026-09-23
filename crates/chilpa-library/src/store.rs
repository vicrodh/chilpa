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
    pub fn from_name(nombre: &str) -> Self {
        match nombre {
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

    /// Los álbumes con su cuenta de escuchas, en el orden que se pida.
    ///
    /// Una sola consulta: las escuchas salen de un `GROUP BY` sobre el log
    /// unido una vez, no de una consulta por álbum. Preguntarle a la base
    /// una vez por cada fila de una reja es justo lo que hacía lenta a la
    /// biblioteca de QBZ.
    pub fn albums_sorted(
        &self,
        orden: AlbumSort,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        self.albums_filtered(orden, None, limit, offset)
    }

    /// Lo mismo, pero solo los de un artista cuando se pide uno.
    pub fn albums_filtered(
        &self,
        orden: AlbumSort,
        artist: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
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
            orden.order_by()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let filas = stmt.query_map(params![limit as i64, offset as i64, artist], |r| {
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
        filas.collect()
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
        let filas = stmt.query_map(params![limit as i64, offset as i64], |r| {
            Ok(StoredArtist {
                name: r.get(0)?,
                album_count: r.get::<_, i64>(1)? as usize,
                track_count: r.get::<_, i64>(2)? as usize,
                artwork_path: r.get(3)?,
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
        let filas = stmt.query_map(params![limit as i64], |r| {
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
        filas.collect()
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

    /// Tres álbumes con distinto artista, año, fecha de alta y escuchas.
    fn biblioteca_de_muestra() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        // (id, título, artista, año, added_at)
        for (id, titulo, artista, año, alta) in [
            (1, "Bayou", "Creedence", 1969, 100),
            (2, "Aqualung", "Tull", 1971, 300),
            (3, "Cosmo", "Creedence", 1970, 200),
        ] {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist, year)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, titulo, artista, año],
                )
                .unwrap();
            lib.db
                .execute(
                    "INSERT INTO tracks (source_id, external_id, title, album_id, duration_ms, added_at)
                     VALUES (?1, ?2, 'Pista', ?3, 1000, ?4)",
                    params![source, format!("/{titulo}.flac"), id, alta],
                )
                .unwrap();
        }
        // El 3 se escucha dos veces, y la última de todas; el 1, una.
        lib.log_play(Some(1), Some(source), 10, 1000, 1000, false, None, None, true).unwrap();
        lib.log_play(Some(3), Some(source), 20, 1000, 1000, false, None, None, true).unwrap();
        lib.log_play(Some(3), Some(source), 30, 1000, 1000, false, None, None, true).unwrap();
        lib
    }

    fn titulos(lib: &Library, orden: AlbumSort) -> Vec<String> {
        lib.albums_sorted(orden, 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, _)| a.title)
            .collect()
    }

    #[test]
    fn cada_vista_ordena_la_reja_a_su_manera() {
        let lib = biblioteca_de_muestra();
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
    fn la_reja_trae_las_escuchas_en_la_misma_consulta() {
        let lib = biblioteca_de_muestra();
        let escuchas: Vec<(String, usize)> = lib
            .albums_sorted(AlbumSort::Played, 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, n)| (a.title, n))
            .collect();
        assert_eq!(
            escuchas,
            vec![("Cosmo".into(), 2), ("Bayou".into(), 1), ("Aqualung".into(), 0)]
        );
    }

    #[test]
    fn los_artistas_salen_con_lo_que_tienen() {
        let lib = biblioteca_de_muestra();
        let artistas = lib.artists(50, 0).unwrap();
        assert_eq!(
            artistas.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["Creedence", "Tull"],
            "por nombre"
        );
        assert_eq!(artistas[0].album_count, 2, "Creedence trae dos discos");
        assert_eq!(artistas[0].track_count, 2, "con una pista cada uno");
        assert_eq!(artistas[1].album_count, 1);
    }

    #[test]
    fn la_reja_se_puede_filtrar_por_artista() {
        let lib = biblioteca_de_muestra();
        let suyos: Vec<String> = lib
            .albums_filtered(AlbumSort::Title, Some("Creedence"), 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, _)| a.title)
            .collect();
        assert_eq!(suyos, ["Bayou", "Cosmo"]);

        // Sin filtro, salen todos.
        assert_eq!(lib.albums_filtered(AlbumSort::Title, None, 50, 0).unwrap().len(), 3);
        // Y un nombre que no existe no trae nada, sin tronar.
        assert!(lib.albums_filtered(AlbumSort::Title, Some("Nadie"), 50, 0).unwrap().is_empty());
    }

    #[test]
    fn un_nombre_con_comillas_no_rompe_la_consulta() {
        let lib = biblioteca_de_muestra();
        let raro = "O'Connor'; DROP TABLE albums; --";
        assert!(lib.albums_filtered(AlbumSort::Title, Some(raro), 50, 0).unwrap().is_empty());
        assert_eq!(lib.albums_sorted(AlbumSort::Title, 50, 0).unwrap().len(), 3, "la tabla sigue");
    }

    #[test]
    fn un_nombre_de_vista_desconocido_no_deja_la_reja_vacia() {
        assert_eq!(AlbumSort::from_name("lo que sea"), AlbumSort::Artist);
        for v in AlbumSort::all() {
            assert_eq!(AlbumSort::from_name(v.name()), v, "{} va y viene", v.name());
        }
    }

    #[test]
    fn los_mas_escuchados_salen_del_log_y_en_orden() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (album, titulo) in [(1, "Uno"), (2, "Dos")] {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist) VALUES (?1, ?2, 'Artista')",
                    params![album, titulo],
                )
                .unwrap();
            lib.db
                .execute(
                    "INSERT INTO tracks (source_id, external_id, title, album_id, duration_ms, added_at)
                     VALUES (?1, ?2, 'Pista', ?3, 1000, 0)",
                    params![source, format!("/{titulo}.flac"), album],
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
