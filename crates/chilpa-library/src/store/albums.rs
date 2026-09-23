//! La reja de álbumes y los artistas.
//!
//! Todo sale de una consulta con `GROUP BY`: en QBZ cada página volvía a
//! agregar la biblioteca entera, y ese era el hallazgo alto de la auditoría.

use crate::store::Library;
use rusqlite::{params, OptionalExtension};
use crate::store::{row_to_track, StoredTrack};

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fixtures::*;

    #[test]
    fn each_view_sorts_the_grid_its_own_way() {
        let lib = sample_library();
        assert_eq!(
            titles(&lib, AlbumSort::Artist),
            ["Bayou", "Cosmo", "Aqualung"],
            "artista, luego año"
        );
        assert_eq!(titles(&lib, AlbumSort::Title), ["Aqualung", "Bayou", "Cosmo"]);
        assert_eq!(titles(&lib, AlbumSort::Year), ["Aqualung", "Cosmo", "Bayou"], "del más nuevo");
        assert_eq!(titles(&lib, AlbumSort::Added), ["Aqualung", "Cosmo", "Bayou"], "lo último que entró");
        assert_eq!(titles(&lib, AlbumSort::Played), ["Cosmo", "Bayou", "Aqualung"]);
        assert_eq!(
            titles(&lib, AlbumSort::LastPlayed),
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

}
