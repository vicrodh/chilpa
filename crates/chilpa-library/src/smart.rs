//! Listas inteligentes: reglas sobre la biblioteca, no una lista a mano.
//!
//! Una lista inteligente no guarda pistas, guarda condiciones. Se vuelve a
//! resolver cada vez que se mira, así que una pista nueva que cumple entra
//! sola y una que dejó de cumplir sale sola.
//!
//! Las reglas de hoy salen de lo que la biblioteca ya sabe: cuándo entró una
//! pista, cuántas veces sonó, con qué formato y cuánto dura. Las de mood y
//! ritmo van a ser reglas más sobre `audio_features`, que ya está en el
//! esquema y versionada por analizador; por eso esto se arma como una lista
//! de reglas y no como seis consultas escritas a mano.
//!
//! Todo lo que viene del usuario viaja como parámetro. Lo único que se
//! interpola en el SQL son las plantillas de este archivo.

use crate::store::{Library, StoredTrack};
use rusqlite::types::Value;

/// Una condición que una pista tiene que cumplir.
#[derive(Clone, PartialEq, Debug)]
pub enum Rule {
    /// Nunca ha sonado completa.
    NeverPlayed,
    /// Ha sonado completa por lo menos tantas veces.
    PlayedAtLeast(usize),
    /// Entró a la biblioteca en los últimos tantos días.
    AddedWithinDays(u32),
    /// Sonó en los últimos tantos días.
    PlayedWithinDays(u32),
    /// Más de 16 bits o más de 48 kHz.
    HiRes,
    /// Dura más que esto.
    LongerThanMs(u64),
    /// Dura menos que esto.
    ShorterThanMs(u64),
    /// De ese artista, sin distinguir mayúsculas.
    ArtistIs(String),
    /// De ese género.
    GenreIs(String),
    /// Con un tempo analizado dentro de ese rango.
    BpmBetween(f32, f32),
}

/// Cómo se ordena lo que sale.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Order {
    /// Artista y título, como una repisa.
    #[default]
    Artist,
    /// Lo último que entró primero.
    NewestFirst,
    /// Lo más escuchado primero.
    MostPlayed,
    /// Al azar, distinto cada vez que se mira.
    Random,
}

/// Una lista inteligente: un nombre y sus condiciones.
#[derive(Clone, PartialEq, Debug)]
pub struct SmartList {
    pub name: String,
    /// Todas se tienen que cumplir. Un "o" se arma con dos listas.
    pub rules: Vec<Rule>,
    pub order: Order,
    pub limit: usize,
}

impl SmartList {
    pub fn new(name: &str, rules: Vec<Rule>, order: Order, limit: usize) -> Self {
        Self { name: name.to_string(), rules, order, limit }
    }
}

/// Las listas que Chilpa trae de fábrica.
///
/// Son las que se pueden calcular sin análisis de audio. Las de mood y ritmo
/// se suman aquí en cuanto haya `audio_features`.
pub fn builtins() -> Vec<SmartList> {
    vec![
        SmartList::new("Nunca escuchadas", vec![Rule::NeverPlayed], Order::Random, 200),
        SmartList::new(
            "Agregadas este mes",
            vec![Rule::AddedWithinDays(30)],
            Order::NewestFirst,
            200,
        ),
        SmartList::new(
            "Lo que más escuchas",
            vec![Rule::PlayedAtLeast(2)],
            Order::MostPlayed,
            100,
        ),
        SmartList::new("Alta resolución", vec![Rule::HiRes], Order::Artist, 500),
        SmartList::new(
            "De esta semana",
            vec![Rule::PlayedWithinDays(7)],
            Order::MostPlayed,
            100,
        ),
        SmartList::new(
            "Para un rato largo",
            vec![Rule::LongerThanMs(8 * 60 * 1000)],
            Order::Artist,
            200,
        ),
        // Estas dos necesitan el análisis; sin él salen vacías, que es lo
        // honesto: no hay con qué llenarlas todavía.
        SmartList::new("Tranquilo", vec![Rule::BpmBetween(50.0, 95.0)], Order::Random, 200),
        SmartList::new(
            "Para moverse",
            vec![Rule::BpmBetween(118.0, 145.0)],
            Order::Random,
            200,
        ),
    ]
}

/// El pedazo de `WHERE` de una regla, con sus parámetros.
fn condition(rule: &Rule, now: i64) -> (String, Vec<Value>) {
    let days_ago = |d: u32| Value::Integer(now - i64::from(d) * 86_400);
    match rule {
        Rule::NeverPlayed => ("COALESCE(e.escuchas, 0) = 0".into(), vec![]),
        Rule::PlayedAtLeast(n) => (
            "COALESCE(e.escuchas, 0) >= ?".into(),
            vec![Value::Integer(*n as i64)],
        ),
        Rule::AddedWithinDays(d) => ("t.added_at >= ?".into(), vec![days_ago(*d)]),
        Rule::PlayedWithinDays(d) => ("e.ultima >= ?".into(), vec![days_ago(*d)]),
        // Alta resolución es cualquier cosa por encima del CD.
        Rule::HiRes => (
            "(t.bit_depth > 16 OR t.sample_rate_hz > 48000)".into(),
            vec![],
        ),
        Rule::LongerThanMs(ms) => (
            "t.duration_ms > ?".into(),
            vec![Value::Integer(*ms as i64)],
        ),
        Rule::ShorterThanMs(ms) => (
            // Una duración de cero quiere decir que no se sabe; no cuenta
            // como "corta".
            "(t.duration_ms > 0 AND t.duration_ms < ?)".into(),
            vec![Value::Integer(*ms as i64)],
        ),
        Rule::ArtistIs(name) => (
            "t.artist = ? COLLATE NOCASE".into(),
            vec![Value::Text(name.clone())],
        ),
        Rule::BpmBetween(min, max) => (
            // Cualquier versión del analizador sirve: la lista dice qué
            // cumple hoy, no con qué analizador se midió.
            "EXISTS (SELECT 1 FROM audio_features f
                     WHERE f.track_id = t.id AND f.bpm BETWEEN ? AND ?)"
                .into(),
            vec![Value::Real(f64::from(*min)), Value::Real(f64::from(*max))],
        ),
        Rule::GenreIs(name) => (
            "EXISTS (SELECT 1 FROM track_genres tg
                     JOIN genres g ON g.id = tg.genre_id
                     WHERE tg.track_id = t.id AND g.name = ?)"
                .into(),
            vec![Value::Text(name.clone())],
        ),
    }
}

fn sql_order(order: Order) -> &'static str {
    match order {
        Order::Artist => "t.artist COLLATE NOCASE, t.title COLLATE NOCASE",
        Order::NewestFirst => "t.added_at DESC, t.title COLLATE NOCASE",
        Order::MostPlayed => "COALESCE(e.escuchas, 0) DESC, t.title COLLATE NOCASE",
        Order::Random => "RANDOM()",
    }
}

impl Library {
    /// Resuelve una lista inteligente contra la biblioteca.
    ///
    /// Solo pistas presentes: una lista que ofrece lo que no se puede poner
    /// a sonar no sirve de nada.
    pub fn smart_tracks(&self, list: &SmartList) -> rusqlite::Result<Vec<StoredTrack>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut conditions = vec!["t.present = 1".to_string()];
        let mut values: Vec<Value> = Vec::new();
        for rule in &list.rules {
            let (sql, params) = condition(rule, now);
            conditions.push(sql);
            values.extend(params);
        }
        values.push(Value::Integer(list.limit.max(1) as i64));

        let sql = format!(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present
             FROM tracks t
             LEFT JOIN albums a ON a.id = t.album_id
             LEFT JOIN (
                 SELECT p.track_id AS tid,
                        count(*) AS escuchas,
                        max(p.started_at) AS ultima
                 FROM play_events p
                 WHERE p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms
                 GROUP BY p.track_id
             ) e ON e.tid = t.id
             WHERE {}
             ORDER BY {}
             LIMIT ?",
            conditions.join(" AND "),
            sql_order(list.order)
        );

        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), crate::store::row_to_track)?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// Cuatro pistas con historias distintas.
    fn library() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        // (id, título, artista, duración, bits, rate, added_at)
        let rows = [
            (1, "Vieja", "Uno", 200_000i64, 16, 44_100, now - 400 * 86_400),
            (2, "Nueva", "Dos", 200_000, 24, 96_000, now - 2 * 86_400),
            (3, "Larga", "Uno", 10 * 60_000, 16, 44_100, now - 2 * 86_400),
            (4, "Ausente", "Tres", 100_000, 16, 44_100, now - 2 * 86_400),
        ];
        for (id, title, artist, dur, bits, rate, high) in rows {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, artist, duration_ms,
                                         bit_depth, sample_rate_hz, added_at, present)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        id,
                        source,
                        format!("/{title}.flac"),
                        title,
                        artist,
                        dur,
                        bits,
                        rate,
                        high,
                        i64::from(id != 4)
                    ],
                )
                .unwrap();
        }
        // La vieja sonó tres veces, hace mucho; la nueva, dos, ayer.
        for i in 0..3 {
            lib.log_play(Some(1), Some(source), now - 300 * 86_400 + i, 200_000, 200_000, false, None, None, true)
                .unwrap();
        }
        for i in 0..2 {
            lib.log_play(Some(2), Some(source), now - 86_400 + i, 200_000, 200_000, false, None, None, true)
                .unwrap();
        }
        lib
    }

    fn titles(lib: &Library, list: &SmartList) -> Vec<String> {
        lib.smart_tracks(list).unwrap().into_iter().map(|t| t.title).collect()
    }

    #[test]
    fn never_played_means_the_ones_that_never_played() {
        let lib = library();
        let mut come_out = titles(
            &lib,
            &SmartList::new("x", vec![Rule::NeverPlayed], Order::Artist, 50),
        );
        come_out.sort();
        assert_eq!(come_out, ["Larga"], "la ausente no cuenta aunque nunca sonó");
    }

    #[test]
    fn recently_added_leaves_the_old_ones_out() {
        let lib = library();
        let mut come_out = titles(
            &lib,
            &SmartList::new("x", vec![Rule::AddedWithinDays(30)], Order::NewestFirst, 50),
        );
        come_out.sort();
        assert_eq!(come_out, ["Larga", "Nueva"]);
    }

    #[test]
    fn this_week_looks_at_when_it_played_not_when_it_arrived() {
        let lib = library();
        assert_eq!(
            titles(&lib, &SmartList::new("x", vec![Rule::PlayedWithinDays(7)], Order::MostPlayed, 50)),
            ["Nueva"],
            "la vieja sonó hace trescientos días"
        );
    }

    #[test]
    fn high_resolution_is_whatever_goes_beyond_cd() {
        let lib = library();
        assert_eq!(
            titles(&lib, &SmartList::new("x", vec![Rule::HiRes], Order::Artist, 50)),
            ["Nueva"]
        );
    }

    #[test]
    fn every_rule_adds_up() {
        let lib = library();
        let list = SmartList::new(
            "x",
            vec![Rule::AddedWithinDays(30), Rule::LongerThanMs(8 * 60_000)],
            Order::Artist,
            50,
        );
        assert_eq!(titles(&lib, &list), ["Larga"], "nueva y larga a la vez");
    }

    #[test]
    fn the_limit_is_honored() {
        let lib = library();
        let list = SmartList::new("x", vec![], Order::Artist, 2);
        assert_eq!(lib.smart_tracks(&list).unwrap().len(), 2);
    }

    #[test]
    fn bpm_filters_on_what_the_analysis_found() {
        let lib = library();
        lib.save_features(1, "chilpa", 1, Some(128.0), None, None, None).unwrap();
        lib.save_features(2, "chilpa", 1, Some(72.0), None, None, None).unwrap();
        // La 3 se analizó y no se le encontró pulso.
        lib.save_features(3, "chilpa", 1, None, None, None, None).unwrap();

        let fast = SmartList::new("x", vec![Rule::BpmBetween(118.0, 145.0)], Order::Artist, 50);
        assert_eq!(titles(&lib, &fast), ["Vieja"]);

        let slow = SmartList::new("x", vec![Rule::BpmBetween(50.0, 95.0)], Order::Artist, 50);
        assert_eq!(titles(&lib, &slow), ["Nueva"]);

        // Y sin análisis, la lista sale vacía en vez de traerlo todo.
        let clean = Library::in_memory().unwrap();
        assert!(clean.smart_tracks(&fast).unwrap().is_empty());
    }

    #[test]
    fn analyzing_again_with_the_same_version_replaces_its_row() {
        let lib = library();
        lib.save_features(1, "chilpa", 1, Some(100.0), None, None, None).unwrap();
        lib.save_features(1, "chilpa", 1, Some(128.0), None, None, None).unwrap();
        assert_eq!(lib.track_bpm(1, "chilpa").unwrap(), Some(128.0));
        assert_eq!(lib.analyzed_count("chilpa", 1).unwrap(), 1, "una fila, no dos");
    }

    #[test]
    fn pending_means_whatever_lacks_that_version() {
        let lib = library();
        assert_eq!(lib.tracks_pending_analysis("chilpa", 1, 50).unwrap().len(), 3, "sin la ausente");
        lib.save_features(1, "chilpa", 1, Some(128.0), None, None, None).unwrap();
        assert_eq!(lib.tracks_pending_analysis("chilpa", 1, 50).unwrap().len(), 2);
        // Un analizador nuevo las vuelve a pedir todas.
        assert_eq!(lib.tracks_pending_analysis("chilpa", 2, 50).unwrap().len(), 3);
    }

    #[test]
    fn an_odd_artist_name_is_just_a_name() {
        let lib = library();
        let odd = "Uno'; DROP TABLE tracks; --";
        let list = SmartList::new("x", vec![Rule::ArtistIs(odd.into())], Order::Artist, 50);
        assert!(lib.smart_tracks(&list).unwrap().is_empty());
        assert_eq!(lib.track_count(true).unwrap(), 4, "la tabla sigue");
    }

    #[test]
    fn the_built_in_ones_all_run() {
        let lib = library();
        for list in builtins() {
            lib.smart_tracks(&list)
                .unwrap_or_else(|e| panic!("la lista \"{}\" no corrió: {e}", list.name));
        }
    }
}
