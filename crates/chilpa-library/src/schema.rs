//! El esquema de la biblioteca.
//!
//! Cuatro decisiones vienen de la auditoría de QBZ (CHD-0001), donde la base
//! de datos era el cuello de botella y la fuente de las pérdidas de datos:
//!
//! 1. **La identidad de un track es `(source, id externo)`.** Un entero
//!    autoincremental como identidad hacía que reindexar cambiara los ids y
//!    rompiera las playlists.
//! 2. **El log de reproducciones es append-only y existe desde el día uno.**
//!    Las stats y las smart lists son consultas sobre él. Si no se guarda
//!    desde el principio, esa historia no se puede recuperar después.
//! 3. **Las features de audio llevan la versión del analizador.** Un
//!    analizador mejor puede recalcular sin revolver lo viejo.
//! 4. **Nada se borra por no encontrarse.** Un root desmontado marca sus
//!    tracks como ausentes; borrarlos fue el hallazgo alto de la auditoría.

use rusqlite::Connection;

/// La versión del esquema que entiende este código.
pub const VERSION: i64 = 5;

/// Los pasos, en orden. El índice es la versión a la que llevan: el primero
/// lleva de 0 a 1, el segundo de 1 a 2.
///
/// Cada paso corre una sola vez y nunca se edita después de salir: una base
/// que ya lo aplicó no lo va a volver a aplicar, así que cambiarlo solo
/// crearía dos formas distintas de la misma versión.
const PASOS: &[&str] =
    &[SCHEMA_V1, MIGRACION_V2, MIGRACION_V3, MIGRACION_V4, MIGRACION_V5];

/// Crea o actualiza el esquema.
pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let actual: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if actual > VERSION {
        // Una base de una versión más nueva no se toca: bajar de versión
        // pierde datos en silencio. Se dice qué pasó, porque el error
        // genérico de rusqlite ("Query is not read-only") no le dice nada a
        // nadie y esto sí ocurre: un Chilpa viejo contra una biblioteca que
        // ya abrió uno nuevo.
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISMATCH),
            Some(format!(
                "esta biblioteca es de la versión {actual} y este Chilpa entiende hasta la \
                 {VERSION}; actualiza Chilpa en vez de abrirla con uno viejo"
            )),
        ));
    }
    for (i, paso) in PASOS.iter().enumerate() {
        let destino = i as i64 + 1;
        if actual >= destino {
            continue;
        }
        // Cada paso va en su transacción: si truena a la mitad, la versión
        // no avanza y la base queda como estaba.
        db.execute_batch(&format!("BEGIN; {paso} PRAGMA user_version = {destino}; COMMIT;"))?;
    }
    Ok(())
}

/// Los pragmas con los que se abre. WAL deja que la interfaz lea mientras el
/// escaneo escribe, que es justo lo que en QBZ daba "database is locked".
pub fn apply_pragmas(db: &Connection) -> rusqlite::Result<()> {
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "synchronous", "NORMAL")?;
    db.pragma_update(None, "foreign_keys", "ON")?;
    // Una espera antes de rendirse: un escaneo largo no debe tirar una
    // lectura de la interfaz.
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

const SCHEMA_V1: &str = r#"
-- De dónde sale el contenido: proveedor más instancia. La biblioteca local
-- es un source como cualquier otro.
CREATE TABLE sources (
    id          INTEGER PRIMARY KEY,
    provider    TEXT NOT NULL,
    instance    TEXT NOT NULL,
    label       TEXT NOT NULL DEFAULT '',
    UNIQUE (provider, instance)
);

-- Las carpetas que se escanean. Un root con su mountpoint ausente no se
-- poda: se marca y ya.
CREATE TABLE roots (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL REFERENCES sources(id),
    path        TEXT NOT NULL UNIQUE,
    last_scan   INTEGER,
    reachable   INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE artists (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    mbid        TEXT,
    UNIQUE (name, mbid)
);

CREATE TABLE albums (
    id            INTEGER PRIMARY KEY,
    title         TEXT NOT NULL,
    album_artist  TEXT NOT NULL DEFAULT '',
    mbid          TEXT,
    year          INTEGER,
    artwork_path  TEXT,
    UNIQUE (title, album_artist, mbid)
);

-- La identidad es (source_id, external_id). external_id de un archivo local
-- es su ruta.
CREATE TABLE tracks (
    id             INTEGER PRIMARY KEY,
    source_id      INTEGER NOT NULL REFERENCES sources(id),
    external_id    TEXT NOT NULL,
    title          TEXT NOT NULL,
    artist         TEXT NOT NULL DEFAULT '',
    album_id       INTEGER REFERENCES albums(id),
    track_number   INTEGER,
    disc_number    INTEGER,
    duration_ms    INTEGER NOT NULL DEFAULT 0,
    sample_rate_hz INTEGER,
    bit_depth      INTEGER,
    channels       INTEGER,
    codec          TEXT,
    isrc           TEXT,
    recording_mbid TEXT,
    -- 1 mientras el archivo esté donde dijo estar. Un source inalcanzable
    -- pone 0 y conserva la fila.
    present        INTEGER NOT NULL DEFAULT 1,
    added_at       INTEGER NOT NULL,
    UNIQUE (source_id, external_id)
);

CREATE INDEX tracks_album ON tracks(album_id);
CREATE INDEX tracks_artist ON tracks(artist);
CREATE INDEX tracks_present ON tracks(present);
CREATE INDEX tracks_isrc ON tracks(isrc) WHERE isrc IS NOT NULL;
CREATE INDEX tracks_mbid ON tracks(recording_mbid) WHERE recording_mbid IS NOT NULL;

-- Lo que el escaneo vio en disco, para poder ser incremental. Se guarda el
-- inodo además de la fecha: renombrar un archivo no debe re-extraer sus tags.
CREATE TABLE scanned_files (
    id          INTEGER PRIMARY KEY,
    root_id     INTEGER NOT NULL REFERENCES roots(id),
    path        TEXT NOT NULL UNIQUE,
    -- En nanosegundos: en segundos, dos cambios dentro del mismo segundo
    -- se ven iguales.
    mtime       INTEGER NOT NULL,
    size        INTEGER NOT NULL,
    inode       INTEGER,
    track_id    INTEGER REFERENCES tracks(id),
    scanned_at  INTEGER NOT NULL
);

CREATE INDEX scanned_root ON scanned_files(root_id);
CREATE INDEX scanned_inode ON scanned_files(inode) WHERE inode IS NOT NULL;

-- El log de reproducciones. Append-only: de aquí salen las stats, las smart
-- lists y el sync de play counts con un DAP.
CREATE TABLE play_events (
    id            INTEGER PRIMARY KEY,
    track_id      INTEGER REFERENCES tracks(id),
    source_id     INTEGER REFERENCES sources(id),
    started_at    INTEGER NOT NULL,
    -- Cuánto sonó de verdad, en milisegundos entregados.
    played_ms     INTEGER NOT NULL,
    -- La duración que tenía la pista, para saber si se escuchó completa.
    duration_ms   INTEGER NOT NULL DEFAULT 0,
    -- 1 si el usuario la saltó.
    skipped       INTEGER NOT NULL DEFAULT 0,
    -- Con qué salió: formato real y si fue bit-perfect.
    sample_rate_hz INTEGER,
    bit_depth      INTEGER,
    bit_perfect    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX play_events_track ON play_events(track_id);
CREATE INDEX play_events_time ON play_events(started_at);

-- Lo que el análisis saca de un track. La versión del analizador va en la
-- fila: cambiarlo no obliga a tirar lo viejo.
CREATE TABLE audio_features (
    track_id      INTEGER NOT NULL REFERENCES tracks(id),
    analyzer      TEXT NOT NULL,
    version       INTEGER NOT NULL,
    bpm           REAL,
    musical_key   TEXT,
    energy        REAL,
    loudness_lufs REAL,
    intro_ms      INTEGER,
    outro_ms      INTEGER,
    analyzed_at   INTEGER NOT NULL,
    PRIMARY KEY (track_id, analyzer, version)
);
"#;

/// De 1 a 2: las listas de reproducción.
///
/// Una lista puede ser de aquí o traída por un source; por eso lleva su id
/// externo, para poder volver a sincronizarla sin duplicarla. Y una entrada
/// guarda la ruta además del track: una lista no se rompe porque algo
/// todavía no esté escaneado.
const MIGRACION_V2: &str = r#"
CREATE TABLE playlists (
    id           INTEGER PRIMARY KEY,
    source_id    INTEGER NOT NULL REFERENCES sources(id),
    external_id  TEXT,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    artwork_path TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    UNIQUE (source_id, external_id)
);

CREATE TABLE playlist_tracks (
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    track_id    INTEGER REFERENCES tracks(id),
    -- Lo que la lista dice que va aquí, aunque el track no esté en la
    -- biblioteca: una ruta local, o el id que le da su source.
    external_id TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (playlist_id, position)
);

CREATE INDEX playlist_tracks_track ON playlist_tracks(track_id);
"#;

/// De 2 a 3: los géneros.
///
/// En una tabla aparte porque un archivo declara varios y juntarlos en una
/// cadena hace que "Rock; Blues" no sea ni Rock ni Blues a la hora de
/// filtrar. `COLLATE NOCASE` en el nombre para que "rock" y "Rock" sean el
/// mismo género, que es lo que quiere decir quien los escribió.
const MIGRACION_V3: &str = r#"
CREATE TABLE genres (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE track_genres (
    track_id INTEGER NOT NULL REFERENCES tracks(id),
    genre_id INTEGER NOT NULL REFERENCES genres(id),
    PRIMARY KEY (track_id, genre_id)
);

CREATE INDEX track_genres_genre ON track_genres(genre_id);
"#;

/// De 3 a 4: qué tan seguro está el analizador del tempo.
///
/// Va en su propia columna y no dentro de `energy`, que es otra cosa.
/// Guardar un número bajo el nombre de otro es la clase de mentira
/// silenciosa que después nadie encuentra.
const MIGRACION_V4: &str = r#"
ALTER TABLE audio_features ADD COLUMN tempo_confidence REAL;
"#;

/// De 4 a 5: un índice sobre `(album_id, present)`.
///
/// Sin él, SQLite elegía `tracks_present` para `album_id = ? AND present = 1`
/// y, como todas las pistas están presentes, recorría la tabla entera por
/// cada álbum. Medido con cinco mil álbumes: la reja pasó de 30 segundos a
/// 7 milisegundos. Es el mismo hallazgo de la auditoría de QBZ, con otra
/// cara.
const MIGRACION_V5: &str = r#"
CREATE INDEX tracks_album_present ON tracks(album_id, present);
ANALYZE;
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn memoria() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db
    }

    #[test]
    fn el_esquema_se_crea_y_queda_marcado_con_su_version() {
        let db = memoria();
        let v: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, VERSION);
    }

    #[test]
    fn migrar_dos_veces_no_hace_nada_la_segunda() {
        let db = memoria();
        migrate(&db).unwrap();
        let tablas: i64 = db
            .query_row("SELECT count(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
            .unwrap();
        assert!(tablas >= 8, "hay {tablas} tablas");
    }

    #[test]
    fn una_base_de_la_version_uno_sube_sin_perder_nada() {
        let db = Connection::open_in_memory().unwrap();
        // Una base como la dejaba la versión 1.
        db.execute_batch(SCHEMA_V1).unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
        db.execute(
            "INSERT INTO tracks (source_id, external_id, title, added_at)
             VALUES (1, '/musica/a.flac', 'Una', 0)",
            [],
        )
        .unwrap();

        migrate(&db).unwrap();

        let v: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, VERSION, "quedó en la versión de hoy");
        let n: i64 = db.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "lo que había sigue ahí");
        // Y la tabla nueva existe.
        db.execute(
            "INSERT INTO playlists (source_id, name, created_at, updated_at)
             VALUES (1, 'Una lista', 0, 0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn la_confianza_del_tempo_tiene_su_propia_columna() {
        let db = memoria();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
        db.execute(
            "INSERT INTO tracks (id, source_id, external_id, title, added_at)
             VALUES (1, 1, '/a.flac', 'Una', 0)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO audio_features (track_id, analyzer, version, bpm, tempo_confidence, analyzed_at)
             VALUES (1, 'chilpa', 1, 128.0, 0.8, 0)",
            [],
        )
        .unwrap();
        let (bpm, conf): (f64, f64) = db
            .query_row("SELECT bpm, tempo_confidence FROM audio_features", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(bpm, 128.0);
        assert_eq!(conf, 0.8);
    }

    #[test]
    fn un_genero_es_el_mismo_sin_importar_como_se_escriba() {
        let db = memoria();
        db.execute("INSERT INTO genres (name) VALUES ('Rock')", []).unwrap();
        assert!(
            db.execute("INSERT INTO genres (name) VALUES ('rock')", []).is_err(),
            "mayúsculas distintas no son dos géneros"
        );
    }

    #[test]
    fn una_lista_no_repite_posicion() {
        let db = memoria();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
        db.execute(
            "INSERT INTO playlists (source_id, name, created_at, updated_at)
             VALUES (1, 'Mía', 0, 0)",
            [],
        )
        .unwrap();
        let poner = |pos: i64| {
            db.execute(
                "INSERT INTO playlist_tracks (playlist_id, position, external_id)
                 VALUES (1, ?1, '/a.flac')",
                [pos],
            )
        };
        poner(0).unwrap();
        assert!(poner(0).is_err(), "el orden de una lista no se duplica");
        poner(1).unwrap();
    }

    #[test]
    fn una_base_mas_nueva_no_se_toca() {
        let db = Connection::open_in_memory().unwrap();
        db.pragma_update(None, "user_version", VERSION + 1).unwrap();
        let e = migrate(&db).expect_err("bajar de versión perdería datos");
        let dicho = e.to_string();
        assert!(dicho.contains("versión"), "el error tiene que explicarse: {dicho}");
        assert!(dicho.contains("actualiza"), "y decir qué hacer: {dicho}");
    }

    #[test]
    fn dos_tracks_no_pueden_compartir_source_e_id_externo() {
        let db = memoria();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
        let insertar = |external: &str| {
            db.execute(
                "INSERT INTO tracks (source_id, external_id, title, added_at)
                 VALUES (1, ?1, 'x', 0)",
                [external],
            )
        };
        insertar("/musica/a.flac").unwrap();
        assert!(insertar("/musica/a.flac").is_err(), "la identidad no se repite");
        insertar("/musica/b.flac").unwrap();
    }

    #[test]
    fn el_log_de_reproducciones_acepta_lo_que_las_stats_necesitan() {
        let db = memoria();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
        db.execute(
            "INSERT INTO tracks (source_id, external_id, title, added_at)
             VALUES (1, '/a.flac', 'Una', 0)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO play_events
             (track_id, source_id, started_at, played_ms, duration_ms, skipped,
              sample_rate_hz, bit_depth, bit_perfect)
             VALUES (1, 1, 1700000000, 180000, 200000, 0, 44100, 16, 1)",
            [],
        )
        .unwrap();
        let (ms, bp): (i64, i64) = db
            .query_row("SELECT played_ms, bit_perfect FROM play_events", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((ms, bp), (180_000, 1));
    }
}
