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

mod steps;
mod credits;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use steps::*;

/// The schema version this code understands.
pub const VERSION: i64 = 25;

/// The steps, in order. The index is the version they lead to: the first
/// goes from 0 to 1, the second from 1 to 2.
///
/// Each step runs once and is never edited after shipping: a database that
/// already applied it will not apply it again, so changing it would only
/// create two different shapes of the same version.
const STEPS: &[&str] = &[SCHEMA_V1, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5, MIGRATION_V6, MIGRATION_V7, MIGRATION_V8, MIGRATION_V9, MIGRATION_V10, MIGRATION_V11, MIGRATION_V12, MIGRATION_V13, MIGRATION_V14, MIGRATION_V15, MIGRATION_V16, MIGRATION_V17, MIGRATION_V18, MIGRATION_V19, MIGRATION_V20, MIGRATION_V21, MIGRATION_V22, MIGRATION_V23, MIGRATION_V24, MIGRATION_V25];

/// Where the copy of a library about to be upgraded goes.
///
/// Beside the library, so it is on the same filesystem -- a backup on a
/// different disk that fails halfway is worse than none -- and named after
/// it, so whoever finds it knows what it is.
pub fn backup_path(library: &Path) -> PathBuf {
    let mut name = library.file_name().unwrap_or_default().to_os_string();
    name.push(".before-upgrade");
    library.with_file_name(name)
}

/// Copies the library before changing its shape.
///
/// A migration rewrites the file in place and there is no way back from it.
/// Every schema change in here has been walked forward in a test, but a test
/// covers the cases somebody thought of, and what is at stake is a library
/// somebody spent years building. So: one copy, taken before the first step
/// runs, replaced by the next upgrade, in the same folder as the library.
///
/// `VACUUM INTO` and not a file copy: a copy of the file alone misses the
/// write-ahead log and produces a library missing its most recent writes,
/// which is exactly the shape of backup that looks fine until it is needed.
/// It costs about half a second and five per cent less room than the
/// original on two hundred thousand tracks, and it is paid only when there
/// is really something to upgrade.
///
/// If it cannot be taken, the upgrade does not happen. Chilpa opening at all
/// costs less than a library that cannot be gone back to.
pub fn back_up_before_upgrading(db: &Connection, library: &Path) -> rusqlite::Result<()> {
    let actual: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    // Zero is a file that was just created: there is nothing in it yet.
    if actual == 0 || actual >= VERSION {
        return Ok(());
    }
    let backup = backup_path(library);
    // VACUUM INTO refuses to write over a file that is already there.
    let _ = std::fs::remove_file(&backup);
    db.execute("VACUUM INTO ?1", [&backup.to_string_lossy().as_ref()]).map_err(|e| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(chilpa_i18n::t_args(
                "your library needs updating and a copy of it could not be kept first, \
                 at {}: {}. Make room there, or move the library somewhere with room, \
                 and open Chilpa again",
                &[&backup.display().to_string(), &e.to_string()],
            )),
        )
    })?;
    Ok(())
}

/// Builds a database stopped at an older version, as a shipped Chilpa would
/// have left it.
///
/// It is here and not in a test module because two of them need it: the one
/// that walks every version forward, and the one that checks a library is
/// copied before it is upgraded.
#[cfg(test)]
pub(crate) fn stop_at(db: &Connection, version: usize) {
    for step in STEPS.iter().take(version) {
        db.execute_batch(step).unwrap();
    }
    db.execute_batch(&format!("PRAGMA user_version = {version}")).unwrap();
}

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
            Some(chilpa_i18n::t_args(
                "this library is version {} and this Chilpa understands up to {}; \
                 update Chilpa instead of opening it with an old one",
                &[&actual.to_string(), &VERSION.to_string()],
            )),
        ));
    }
    for (i, step) in STEPS.iter().enumerate() {
        let target = i as i64 + 1;
        if actual >= target {
            continue;
        }
        // Each step goes in its own transaction: if it blows up halfway, the
        // version does not advance and the database stays as it was.
        let tx = db.unchecked_transaction()?;
        if target == 23 { credits::upgrade(&tx)?; }
        tx.execute_batch(step)?;
        tx.pragma_update(None, "user_version", target)?;
        tx.commit()?;
    }
    Ok(())
}

/// The pragmas it opens with. WAL lets the interface read while the scan
/// writes, which is exactly what gave "database is locked" in QBZ.
///
/// **What is not here was measured and left out.** `temp_store = MEMORY`,
/// `cache_size = -32000` and `mmap_size = 256 MB` are on every SQLite
/// performance list. Measured against a fifty-thousand-track library on
/// disk, each of them and all three together moved the grid, the search and
/// the stats by less than the noise -- the file is sixteen megabytes and the
/// operating system is already holding it. A pragma that does nothing is not
/// free: it is a promise in the code that nobody will re-check. If the
/// library ever grows past what the machine will cache, this is the place
/// to measure again, not to assume.
///
/// `synchronous = NORMAL` with WAL is the deliberate trade: a power cut can
/// lose the last transactions, and cannot corrupt the file. For a library
/// that is rebuilt by rescanning, losing the last few seconds is cheap and
/// paying `FULL` on every write is not.
/// Lets the planner see the library it actually has.
///
/// Every query plan in this file was chosen by the planner out of
/// `sqlite_stat1`, and until this existed that table held the statistics of
/// an **empty** database: the migrations end in `ANALYZE`, and a migration
/// runs before there is a single track. The planner then spent the rest of
/// the library's life guessing, and it guessed the same way every time --
/// drive from `tracks`, probe everything else once per row.
///
/// What that cost, measured on two hundred thousand tracks on disk:
///
/// | | guessing | knowing |
/// | --- | --- | --- |
/// | "What you play most", counted | 72 ms | 44 us |
/// | "Played this week", counted | 73 ms | 41 us |
/// | search | 48-76 ms | 18 ms |
/// | search, short prefix | 67 ms | 13 ms |
///
/// The eight smart lists counted together, which is what the panel does
/// every time its tab opens, went from 365 ms to 2 ms. Part of that is these
/// statistics and part is `MIGRATION_V9`; the numbers above are the part
/// that is only this.
///
/// This is `ANALYZE` and not `PRAGMA optimize`, which is the pragma written
/// for the job and is wrong for it: optimize samples two thousand rows per
/// index and will not be talked out of it -- `analysis_limit` at zero means
/// "unset", and it re-imposes its own cap; at a billion it caps anyway
/// (SQLite 3.53). Two thousand rows of `track_rollup(plays)` say that about
/// two thousand tracks share a play count when forty thousand do, and on a
/// hundredfold underestimate the planner decides the index is worth driving
/// from. Counting "What you play most" is 72 ms on the sampled estimate and
/// 44 us on the true one, so the sampling costs more than it saves.
///
/// The full pass is about 200 ms on two hundred thousand tracks. A scan
/// pays it, because a scan already took seconds and is the only thing that
/// changes the shape of the library.
pub fn update_statistics(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("ANALYZE;")
}

/// The same, but only when what the planner believes is far from what is
/// there.
///
/// Opening cannot pay 200 ms every time, and it does not have to: the
/// statistics only go wrong when the library changes size, and a scan
/// updates them itself. What this is for is the library that already exists
/// -- filled before any of this was written, carrying the statistics of an
/// empty database, and with no reason to ever scan again.
///
/// The check is one count, about 2 ms at two hundred thousand tracks. The
/// factor is four rather than something tighter because a plan does not turn
/// on a rounding error: what ruins one is believing there are no tracks when
/// there are two hundred thousand.
pub fn update_statistics_if_stale(db: &Connection) -> rusqlite::Result<()> {
    use rusqlite::OptionalExtension;
    // The first field of `stat` is the table's row count as last analyzed.
    let recorded: Option<String> = db
        .query_row("SELECT stat FROM sqlite_stat1 WHERE tbl = 'tracks' LIMIT 1", [], |r| r.get(0))
        .optional()
        // No `sqlite_stat1` at all: nothing has ever been analyzed.
        .unwrap_or(None);
    let believed: i64 = recorded.and_then(|s| s.split_whitespace().next().and_then(|n| n.parse().ok())).unwrap_or(0);
    let real: i64 = db.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))?;
    if real > believed.saturating_mul(4) || believed > real.saturating_mul(4) {
        update_statistics(db)?;
    }
    Ok(())
}

pub fn apply_pragmas(db: &Connection) -> rusqlite::Result<()> {
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "synchronous", "NORMAL")?;
    db.pragma_update(None, "foreign_keys", "ON")?;
    // A wait before giving up: a long scan must not knock over a read
    // from the interface.
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db
    }

    #[test]
    fn an_older_library_is_brought_up_and_keeps_what_it_had() {
        // Nothing tested the upgrade path, which is the one every library
        // that already exists takes. This one starts where a shipped Chilpa
        // left it and walks it to today.
        let db = Connection::open_in_memory().unwrap();
        stop_at(&db, 8);
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", []).unwrap();
        db.execute(
            "INSERT INTO tracks (id, source_id, external_id, title, added_at)
             VALUES (1, 1, '/a.flac', 'One', 0)",
            [],
        )
        .unwrap();
        // The same track measured twice by the same analyzer, and once by
        // another. Version 8 held all three.
        for (analyzer, version, bpm) in [("chilpa", 1, 130.0), ("chilpa", 2, 90.0), ("other", 1, 77.0)] {
            db.execute(
                "INSERT INTO audio_features (track_id, analyzer, version, bpm, analyzed_at)
                 VALUES (1, ?1, ?2, ?3, 0)",
                rusqlite::params![analyzer, version, bpm],
            )
            .unwrap();
        }
        assert_eq!(db.query_row("SELECT count(*) FROM audio_features", [], |r| r.get::<_, i64>(0)).unwrap(), 3);

        migrate(&db).unwrap();

        assert_eq!(db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), VERSION);
        let rows: Vec<(String, i64, f64)> = db.prepare("SELECT analyzer, version, bpm FROM audio_features ORDER BY analyzer").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(rows, vec![("chilpa".to_string(), 2, 90.0), ("other".to_string(), 1, 77.0)], "the newest of each analyzer, and the superseded one gone");
        assert_eq!(db.query_row("SELECT title FROM tracks WHERE id = 1", [], |r| r.get::<_, String>(0)).unwrap(), "One", "and the track it was about is still there");

        // And what the old key allowed cannot happen again.
        assert!(
            db.execute(
                "INSERT INTO audio_features (track_id, analyzer, version, bpm, analyzed_at)
                 VALUES (1, 'chilpa', 3, 999.0, 0)",
                [],
            )
            .is_err(),
            "a second row for the same analyzer is refused"
        );
    }

    #[test]
    fn the_albums_a_scan_duplicated_are_made_one_again() {
        // What every library written before version 14 looks like: a row per
        // track of each album, with the tracks all pointing at the first.
        let db = Connection::open_in_memory().unwrap();
        stop_at(&db, 13);
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'd')", []).unwrap();
        for id in 1..=3 {
            db.execute(
                "INSERT INTO albums (id, title, album_artist, year, artwork_path)
                 VALUES (?1, 'Kind of Blue', 'Miles', ?2, ?3)",
                rusqlite::params![
                    id,
                    // Only the third one got a year, and only the second a cover.
                    if id == 3 { Some(1959) } else { None },
                    if id == 2 { Some("/covers/kob.jpg") } else { None }
                ],
            )
            .unwrap();
        }
        // One track on the survivor, one that a later scan left on a duplicate.
        db.execute(
            "INSERT INTO tracks (id, source_id, external_id, title, album_id, added_at)
             VALUES (1, 1, '/a.flac', 'So What', 1, 0),
                    (2, 1, '/b.flac', 'Blue in Green', 3, 0)",
            [],
        )
        .unwrap();

        migrate(&db).unwrap();

        let (rows, id, year, cover): (i64, i64, Option<i64>, Option<String>) = db.query_row("SELECT (SELECT count(*) FROM albums), id, year, artwork_path FROM albums", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
        assert_eq!(rows, 1, "one album");
        assert_eq!(id, 1, "the first one is the one that stays");
        assert_eq!(year, Some(1959), "and it kept what the others knew");
        assert_eq!(cover, Some("/covers/kob.jpg".to_string()));

        let on_it: i64 = db.query_row("SELECT count(*) FROM tracks WHERE album_id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(on_it, 2, "both tracks point at it");

        // And it cannot happen again.
        assert!(db.execute("INSERT INTO albums (title, album_artist) VALUES ('Kind of Blue', 'Miles')", [],).is_err(), "a second row for the same album is refused");
    }

    #[test]
    fn every_version_in_between_can_be_walked_forward() {
        // A library can have stopped at any of them.
        for v in 0..STEPS.len() {
            let db = Connection::open_in_memory().unwrap();
            stop_at(&db, v);
            migrate(&db).unwrap_or_else(|e| panic!("from version {v}: {e}"));
            assert_eq!(db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), VERSION, "from version {v}");
        }
    }

    #[test]
    fn the_schema_is_created_and_stamped_with_its_version() {
        let db = in_memory();
        let v: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, VERSION);
    }

    #[test]
    fn migrating_twice_does_nothing_the_second_time() {
        let db = in_memory();
        migrate(&db).unwrap();
        let tables: i64 = db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0)).unwrap();
        assert!(tables >= 8, "{tables} tables");
    }

    #[test]
    fn a_version_one_database_upgrades_without_losing_anything() {
        let db = Connection::open_in_memory().unwrap();
        // A database as version 1 left it.
        db.execute_batch(SCHEMA_V1).unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", []).unwrap();
        db.execute(
            "INSERT INTO tracks (source_id, external_id, title, added_at)
             VALUES (1, '/music/a.flac', 'One', 0)",
            [],
        )
        .unwrap();

        migrate(&db).unwrap();

        let v: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, VERSION, "it ended up on today's version");
        let n: i64 = db.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "what was there is still there");
        // And the new table is there.
        db.execute(
            "INSERT INTO playlists (source_id, name, created_at, updated_at)
             VALUES (1, 'A list', 0, 0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn tempo_confidence_has_a_column_of_its_own() {
        let db = in_memory();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", []).unwrap();
        db.execute(
            "INSERT INTO tracks (id, source_id, external_id, title, added_at)
             VALUES (1, 1, '/a.flac', 'One', 0)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO audio_features (track_id, analyzer, version, bpm, tempo_confidence, analyzed_at)
             VALUES (1, 'chilpa', 1, 128.0, 0.8, 0)",
            [],
        )
        .unwrap();
        let (bpm, conf): (f64, f64) = db.query_row("SELECT bpm, tempo_confidence FROM audio_features", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(bpm, 128.0);
        assert_eq!(conf, 0.8);
    }

    #[test]
    fn a_genre_is_the_same_however_it_is_spelled() {
        let db = in_memory();
        db.execute("INSERT INTO genres (name) VALUES ('Rock')", []).unwrap();
        assert!(db.execute("INSERT INTO genres (name) VALUES ('rock')", []).is_err(), "different case is not two genres");
    }

    #[test]
    fn a_playlist_never_repeats_a_position() {
        let db = in_memory();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", []).unwrap();
        db.execute(
            "INSERT INTO playlists (source_id, name, created_at, updated_at)
             VALUES (1, 'Mine', 0, 0)",
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
        assert!(set(0).is_err(), "a list's order is not duplicated");
        set(1).unwrap();
    }

    #[test]
    fn a_newer_database_is_left_alone() {
        let db = Connection::open_in_memory().unwrap();
        db.pragma_update(None, "user_version", VERSION + 1).unwrap();
        let e = migrate(&db).expect_err("going down a version would lose data");
        let said = e.to_string();
        assert!(said.contains("version"), "the error has to explain itself: {said}");
        assert!(said.contains("update"), "and say what to do: {said}");
    }

    #[test]
    fn two_tracks_cannot_share_a_source_and_an_external_id() {
        let db = in_memory();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", []).unwrap();
        let insert = |external: &str| {
            db.execute(
                "INSERT INTO tracks (source_id, external_id, title, added_at)
                 VALUES (1, ?1, 'x', 0)",
                [external],
            )
        };
        insert("/music/a.flac").unwrap();
        assert!(insert("/music/a.flac").is_err(), "the identity does not repeat");
        insert("/music/b.flac").unwrap();
    }

    #[test]
    fn the_play_log_accepts_what_the_stats_need() {
        let db = in_memory();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", []).unwrap();
        db.execute(
            "INSERT INTO tracks (source_id, external_id, title, added_at)
             VALUES (1, '/a.flac', 'One', 0)",
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
        let (ms, bp): (i64, i64) = db.query_row("SELECT played_ms, bit_perfect FROM play_events", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((ms, bp), (180_000, 1));
    }
}
