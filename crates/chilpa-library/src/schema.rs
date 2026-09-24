//! The library's schema.
//!
//! Four decisions come from the QBZ audit (CHD-0001), where the database was
//! the bottleneck and the source of the data losses:
//!
//! 1. **A track's identity is `(source, external id)`.** An autoincrement
//!    integer as identity meant reindexing changed the ids and broke the
//!    playlists.
//! 2. **The play log is append-only and exists from day one.** The stats and
//!    the smart lists are queries over it. If it is not kept from the
//!    start, that history cannot be recovered later.
//! 3. **Audio features carry the analyzer's version.** A better analyzer can
//!    recompute without disturbing the old rows.
//! 4. **Nothing is deleted for not being found.** An unmounted root marks
//!    its tracks missing; deleting them was the audit's high finding.

use rusqlite::Connection;

/// The schema version this code understands.
pub const VERSION: i64 = 5;

/// The steps, in order. The index is the version they lead to: the first
/// goes from 0 to 1, the second from 1 to 2.
///
/// Each step runs once and is never edited after shipping: a database that
/// already applied it will not apply it again, so changing it would only
/// create two different shapes of the same version.
const STEPS: &[&str] =
    &[SCHEMA_V1, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5];

/// Creates or upgrades the schema.
pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let actual: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if actual > VERSION {
        // A database from a newer version is left alone: downgrading loses
        // data silently. What happened is said, because rusqlite's generic
        // error ("Query is not read-only") tells nobody anything and this
        // does happen: an old Chilpa against a library a newer one already
        // opened.
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISMATCH),
            Some(format!(
                "esta biblioteca es de la versión {actual} y este Chilpa entiende hasta la \
                 {VERSION}; actualiza Chilpa en vez de abrirla con uno viejo"
            )),
        ));
    }
    for (i, paso) in STEPS.iter().enumerate() {
        let target = i as i64 + 1;
        if actual >= target {
            continue;
        }
        // Each step goes in its own transaction: if it blows up halfway, the
        // version does not advance and the database stays as it was.
        db.execute_batch(&format!("BEGIN; {paso} PRAGMA user_version = {target}; COMMIT;"))?;
    }
    Ok(())
}

/// The pragmas it opens with. WAL lets the interface read while the scan
/// writes, which is exactly what gave "database is locked" in QBZ.
pub fn apply_pragmas(db: &Connection) -> rusqlite::Result<()> {
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "synchronous", "NORMAL")?;
    db.pragma_update(None, "foreign_keys", "ON")?;
    // A wait before giving up: a long scan must not knock over a read
    // from the interface.
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

/// From 1 to 2: the playlists.
///
/// A playlist can be local or brought in by a source; that is why it carries
/// its external id, so it can be synced again without duplicating it. And an
/// entry stores the path as well as the track: a playlist does not break
/// because something has not been scanned yet.
const MIGRATION_V2: &str = r#"
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

/// From 2 to 3: the genres.
///
/// In their own table because a file declares several and joining them into
/// one string makes "Rock; Blues" neither Rock nor Blues when filtering.
/// `COLLATE NOCASE` on the name so "rock" and "Rock" are the same genre,
/// which is what whoever wrote them meant.
const MIGRATION_V3: &str = r#"
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

/// From 3 to 4: how sure the analyzer is of the tempo.
///
/// It gets its own column and does not go inside `energy`, which is
/// something else. Storing one number under another's name is the kind of
/// silent lie nobody finds later.
const MIGRATION_V4: &str = r#"
ALTER TABLE audio_features ADD COLUMN tempo_confidence REAL;
"#;

/// From 4 to 5: an index over `(album_id, present)`.
///
/// Without it, SQLite picked `tracks_present` for `album_id = ? AND
/// present = 1` and, since every track is present, walked the whole table
/// for every album. Measured with five thousand albums: the grid went from
/// 30 seconds to 7 milliseconds. It is the same finding as the QBZ audit,
/// wearing another face.
const MIGRATION_V5: &str = r#"
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
    fn the_schema_is_created_and_stamped_with_its_version() {
        let db = memoria();
        let v: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, VERSION);
    }

    #[test]
    fn migrating_twice_does_nothing_the_second_time() {
        let db = memoria();
        migrate(&db).unwrap();
        let tablas: i64 = db
            .query_row("SELECT count(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
            .unwrap();
        assert!(tablas >= 8, "hay {tablas} tablas");
    }

    #[test]
    fn a_version_one_database_upgrades_without_losing_anything() {
        let db = Connection::open_in_memory().unwrap();
        // A database as version 1 left it.
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
        // And the new table is there.
        db.execute(
            "INSERT INTO playlists (source_id, name, created_at, updated_at)
             VALUES (1, 'Una lista', 0, 0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn tempo_confidence_has_a_column_of_its_own() {
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
    fn a_genre_is_the_same_however_it_is_spelled() {
        let db = memoria();
        db.execute("INSERT INTO genres (name) VALUES ('Rock')", []).unwrap();
        assert!(
            db.execute("INSERT INTO genres (name) VALUES ('rock')", []).is_err(),
            "mayúsculas distintas no son dos géneros"
        );
    }

    #[test]
    fn a_playlist_never_repeats_a_position() {
        let db = memoria();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
        db.execute(
            "INSERT INTO playlists (source_id, name, created_at, updated_at)
             VALUES (1, 'Mía', 0, 0)",
            [],
        )
        .unwrap();
        let set = |pos: i64| {
            db.execute(
                "INSERT INTO playlist_tracks (playlist_id, position, external_id)
                 VALUES (1, ?1, '/a.flac')",
                [pos],
            )
        };
        set(0).unwrap();
        assert!(set(0).is_err(), "el orden de una lista no se duplica");
        set(1).unwrap();
    }

    #[test]
    fn a_newer_database_is_left_alone() {
        let db = Connection::open_in_memory().unwrap();
        db.pragma_update(None, "user_version", VERSION + 1).unwrap();
        let e = migrate(&db).expect_err("bajar de versión perdería datos");
        let said = e.to_string();
        assert!(said.contains("versión"), "el error tiene que explicarse: {said}");
        assert!(said.contains("actualiza"), "y decir qué hacer: {said}");
    }

    #[test]
    fn two_tracks_cannot_share_a_source_and_an_external_id() {
        let db = memoria();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
        let insert = |external: &str| {
            db.execute(
                "INSERT INTO tracks (source_id, external_id, title, added_at)
                 VALUES (1, ?1, 'x', 0)",
                [external],
            )
        };
        insert("/musica/a.flac").unwrap();
        assert!(insert("/musica/a.flac").is_err(), "la identidad no se repite");
        insert("/musica/b.flac").unwrap();
    }

    #[test]
    fn the_play_log_accepts_what_the_stats_need() {
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
