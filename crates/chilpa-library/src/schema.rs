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
pub const VERSION: i64 = 9;

/// The steps, in order. The index is the version they lead to: the first
/// goes from 0 to 1, the second from 1 to 2.
///
/// Each step runs once and is never edited after shipping: a database that
/// already applied it will not apply it again, so changing it would only
/// create two different shapes of the same version.
const STEPS: &[&str] =
    &[SCHEMA_V1, MIGRATION_V2, MIGRATION_V3, MIGRATION_V4, MIGRATION_V5, MIGRATION_V6, MIGRATION_V7, MIGRATION_V8, MIGRATION_V9];

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
        db.execute_batch(&format!("BEGIN; {step} PRAGMA user_version = {target}; COMMIT;"))?;
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
    let believed: i64 = recorded
        .and_then(|s| s.split_whitespace().next().and_then(|n| n.parse().ok()))
        .unwrap_or(0);
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

const SCHEMA_V1: &str = r#"
-- Where the content comes from: provider plus instance. The local library
-- is a source like any other.
CREATE TABLE sources (
    id          INTEGER PRIMARY KEY,
    provider    TEXT NOT NULL,
    instance    TEXT NOT NULL,
    label       TEXT NOT NULL DEFAULT '',
    UNIQUE (provider, instance)
);

-- The folders that get scanned. A root whose mountpoint is missing is not
-- pruned: it is marked and that is all.
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

-- The identity is (source_id, external_id). A local file's external_id is
-- its path.
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
    -- 1 while the file is where it said it was. An unreachable source sets
    -- 0 and keeps the row.
    present        INTEGER NOT NULL DEFAULT 1,
    added_at       INTEGER NOT NULL,
    UNIQUE (source_id, external_id)
);

CREATE INDEX tracks_album ON tracks(album_id);
CREATE INDEX tracks_artist ON tracks(artist);
CREATE INDEX tracks_present ON tracks(present);
CREATE INDEX tracks_isrc ON tracks(isrc) WHERE isrc IS NOT NULL;
CREATE INDEX tracks_mbid ON tracks(recording_mbid) WHERE recording_mbid IS NOT NULL;

-- What the scan saw on disk, so it can be incremental. The inode is kept as
-- well as the date: renaming a file must not re-extract its tags.
CREATE TABLE scanned_files (
    id          INTEGER PRIMARY KEY,
    root_id     INTEGER NOT NULL REFERENCES roots(id),
    path        TEXT NOT NULL UNIQUE,
    -- In nanoseconds: in seconds, two changes inside the same second look
    -- identical.
    mtime       INTEGER NOT NULL,
    size        INTEGER NOT NULL,
    inode       INTEGER,
    track_id    INTEGER REFERENCES tracks(id),
    scanned_at  INTEGER NOT NULL
);

CREATE INDEX scanned_root ON scanned_files(root_id);
CREATE INDEX scanned_inode ON scanned_files(inode) WHERE inode IS NOT NULL;

-- The play log. Append-only: the stats, the smart lists and the play count
-- sync with a DAP all come out of here.
CREATE TABLE play_events (
    id            INTEGER PRIMARY KEY,
    track_id      INTEGER REFERENCES tracks(id),
    source_id     INTEGER REFERENCES sources(id),
    started_at    INTEGER NOT NULL,
    -- How much actually played, in delivered milliseconds.
    played_ms     INTEGER NOT NULL,
    -- The duration the track had, so whether it was listened to whole is known.
    duration_ms   INTEGER NOT NULL DEFAULT 0,
    -- 1 if the user skipped it.
    skipped       INTEGER NOT NULL DEFAULT 0,
    -- What it went out as: the real format and whether it was bit-perfect.
    sample_rate_hz INTEGER,
    bit_depth      INTEGER,
    bit_perfect    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX play_events_track ON play_events(track_id);
CREATE INDEX play_events_time ON play_events(started_at);

-- What the analysis pulls out of a track. The analyser's version goes in the
-- row: changing it does not force throwing the old away.
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
    -- What the list says goes here, even when the track is not in the
    -- library: a local path, or the id its source gives it.
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

/// From 5 to 6: what the grid needs about an album, kept instead of
/// recomputed.
///
/// Three of the six orderings -- by plays, by when it was last played, by
/// when it came in -- could not trim the page before aggregating, because
/// the order *is* the aggregate. So every page walked the whole play log
/// joined to the whole track table. Measured with fifty thousand tracks and
/// a play per track: 135 to 155 ms per page, against 7 for the orderings
/// that sort on an `albums` column. At a hundred thousand that is a stutter
/// someone can feel while scrolling.
///
/// The fix is not another index: there is nothing to index, because the
/// number does not exist until it is counted. So it is kept. `album_rollup`
/// carries the three numbers, the triggers keep them current, and the grid
/// goes back to trimming the page first and reading three columns after.
///
/// **Kept means it can drift**, and a number that drifts is worse than a
/// number that is slow. Two things guard it: the triggers cover every way a
/// row gets in (there is no path that writes a play or a track without
/// them), and `Library::rebuild_rollup` recomputes the whole thing from the
/// log, which is what a repair runs and what the tests compare against.
const MIGRATION_V6: &str = r#"
CREATE TABLE album_rollup (
    album_id    INTEGER PRIMARY KEY REFERENCES albums(id) ON DELETE CASCADE,
    -- Plays that count as a listen, by the same rule the queries used:
    -- a track with no known duration, or one played at least halfway.
    plays       INTEGER NOT NULL DEFAULT 0,
    last_played INTEGER,
    -- The newest `added_at` of any of its tracks, which is what "recently
    -- added" means to someone looking at a grid of albums.
    added_at    INTEGER NOT NULL DEFAULT 0,
    -- How many of its tracks are where they said they were. It is here so
    -- the grid does not run a subquery per row and the artists view
    -- aggregates over albums instead of over every track in the library.
    present_tracks INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX album_rollup_plays ON album_rollup(plays DESC);
CREATE INDEX album_rollup_last ON album_rollup(last_played DESC);
CREATE INDEX album_rollup_added ON album_rollup(added_at DESC);
-- The grid and the artists view both skip an album with nothing present.
CREATE INDEX album_rollup_present ON album_rollup(present_tracks);

-- A track arriving gives its album a row, moves its date forward and counts.
CREATE TRIGGER album_rollup_track_in AFTER INSERT ON tracks
WHEN NEW.album_id IS NOT NULL
BEGIN
    INSERT INTO album_rollup (album_id, added_at, present_tracks)
    VALUES (NEW.album_id, NEW.added_at, CASE WHEN NEW.present = 1 THEN 1 ELSE 0 END)
    ON CONFLICT(album_id) DO UPDATE
      SET added_at = max(added_at, NEW.added_at),
          present_tracks = present_tracks + CASE WHEN NEW.present = 1 THEN 1 ELSE 0 END;
END;

CREATE TRIGGER album_rollup_track_out AFTER DELETE ON tracks
WHEN OLD.album_id IS NOT NULL AND OLD.present = 1
BEGIN
    UPDATE album_rollup SET present_tracks = max(0, present_tracks - 1)
     WHERE album_id = OLD.album_id;
END;

-- A track moving album, or going missing and coming back, is the one case
-- where counting again beats adding and subtracting: `present` flips in
-- bulk during a scan, and a count over `tracks_album_present` is an index
-- range for the one album that changed.
--
-- The date is a maximum and is only pushed forward. Lowering it would mean
-- walking the rest of an album's tracks on every move, for a column that
-- exists to sort a grid.
CREATE TRIGGER album_rollup_track_moved AFTER UPDATE OF album_id, added_at, present ON tracks
BEGIN
    INSERT INTO album_rollup (album_id, added_at) VALUES (NEW.album_id, NEW.added_at)
    ON CONFLICT(album_id) DO UPDATE SET added_at = max(added_at, NEW.added_at);
    UPDATE album_rollup
       SET present_tracks = (SELECT count(*) FROM tracks t
                             WHERE t.album_id = album_rollup.album_id AND t.present = 1)
     WHERE album_id IN (OLD.album_id, NEW.album_id);
END;

-- The same for a track. `top_tracks`, `recently_played` and every smart
-- list that looks at plays used to group the whole log; now they read a
-- column and stop at the limit.
--
-- Two dates, not one, because they answer different questions: `last_played`
-- is the last time it was really listened to, and `last_touched` the last
-- time it came up at all, skip included. "The last thing that played" means
-- the second one; the count next to it means the first.
CREATE TABLE track_rollup (
    track_id     INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    plays        INTEGER NOT NULL DEFAULT 0,
    last_played  INTEGER,
    last_touched INTEGER
);

CREATE INDEX track_rollup_plays ON track_rollup(plays DESC);
CREATE INDEX track_rollup_touched ON track_rollup(last_touched DESC);

CREATE TRIGGER track_rollup_play AFTER INSERT ON play_events
WHEN NEW.track_id IS NOT NULL
BEGIN
    INSERT INTO track_rollup (track_id, plays, last_played, last_touched)
    VALUES (
        NEW.track_id,
        CASE WHEN NEW.duration_ms = 0 OR NEW.played_ms * 2 >= NEW.duration_ms THEN 1 ELSE 0 END,
        CASE WHEN NEW.duration_ms = 0 OR NEW.played_ms * 2 >= NEW.duration_ms
             THEN NEW.started_at END,
        NEW.started_at
    )
    ON CONFLICT(track_id) DO UPDATE
      SET plays = plays + CASE WHEN NEW.duration_ms = 0
                                 OR NEW.played_ms * 2 >= NEW.duration_ms THEN 1 ELSE 0 END,
          last_played = CASE WHEN NEW.duration_ms = 0 OR NEW.played_ms * 2 >= NEW.duration_ms
                             THEN max(COALESCE(last_played, 0), NEW.started_at)
                             ELSE last_played END,
          last_touched = max(COALESCE(last_touched, 0), NEW.started_at);
END;

-- A play counts by the same rule the queries used, and nowhere else is that
-- rule written any more.
CREATE TRIGGER album_rollup_play AFTER INSERT ON play_events
WHEN NEW.track_id IS NOT NULL
 AND (NEW.duration_ms = 0 OR NEW.played_ms * 2 >= NEW.duration_ms)
BEGIN
    INSERT INTO album_rollup (album_id, plays, last_played)
    SELECT t.album_id, 1, NEW.started_at
    FROM tracks t WHERE t.id = NEW.track_id AND t.album_id IS NOT NULL
    ON CONFLICT(album_id) DO UPDATE
      SET plays = plays + 1,
          last_played = max(COALESCE(last_played, 0), NEW.started_at);
END;

-- What is already there, counted once.
INSERT INTO album_rollup (album_id, plays, last_played, added_at, present_tracks)
SELECT a.id,
       COALESCE(e.plays, 0),
       e.last_at,
       COALESCE((SELECT max(t.added_at) FROM tracks t WHERE t.album_id = a.id), 0),
       (SELECT count(*) FROM tracks t WHERE t.album_id = a.id AND t.present = 1)
FROM albums a
LEFT JOIN (
    SELECT t2.album_id AS aid, count(*) AS plays, max(p.started_at) AS last_at
    FROM play_events p
    JOIN tracks t2 ON t2.id = p.track_id
    WHERE p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms
    GROUP BY t2.album_id
) e ON e.aid = a.id;

INSERT INTO track_rollup (track_id, plays, last_played, last_touched)
SELECT p.track_id,
       sum(CASE WHEN p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms THEN 1 ELSE 0 END),
       max(CASE WHEN p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms
                THEN p.started_at END),
       max(p.started_at)
FROM play_events p
WHERE p.track_id IS NOT NULL
GROUP BY p.track_id;

ANALYZE;
"#;

/// From 6 to 7: the search stops reading the whole library.
///
/// It was four `LIKE '%text%'` in an `OR`, over title, artist, album title
/// and genre. `LIKE` with a leading wildcard cannot use an index, so every
/// keystroke walked every track and, for the genre, ran a subquery per row.
/// Measured with fifty thousand tracks: 80 ms, growing straight with the
/// library.
///
/// FTS5 turns that into an index lookup. What it costs is a copy of the four
/// pieces of text -- a few megabytes at a hundred thousand tracks -- and the
/// triggers that keep it current. What it buys is a search that answers in
/// the time it takes to draw the result, at any size.
///
/// `remove_diacritics 2` is what makes "Bjork" find "Björk" and "naive"
/// find "naïve", which is the difference between a search someone trusts
/// and one they learn to work around. It matters most in a library that is
/// not in English, which is most libraries.
const MIGRATION_V7: &str = r#"
CREATE VIRTUAL TABLE track_search USING fts5(
    title, artist, album, genres,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- A track arriving is indexed with its album's title. Its genres come
-- later, in their own table, and their trigger refreshes this row.
CREATE TRIGGER track_search_in AFTER INSERT ON tracks
BEGIN
    INSERT INTO track_search (rowid, title, artist, album, genres)
    VALUES (
        NEW.id,
        NEW.title,
        NEW.artist,
        COALESCE((SELECT a.title FROM albums a WHERE a.id = NEW.album_id), ''),
        ''
    );
END;

CREATE TRIGGER track_search_out AFTER DELETE ON tracks
BEGIN
    DELETE FROM track_search WHERE rowid = OLD.id;
END;

-- An FTS5 row is replaced, not edited: delete and insert is the documented
-- way, and it keeps the index and the text from disagreeing.
CREATE TRIGGER track_search_changed AFTER UPDATE OF title, artist, album_id ON tracks
BEGIN
    DELETE FROM track_search WHERE rowid = NEW.id;
    INSERT INTO track_search (rowid, title, artist, album, genres)
    VALUES (
        NEW.id,
        NEW.title,
        NEW.artist,
        COALESCE((SELECT a.title FROM albums a WHERE a.id = NEW.album_id), ''),
        COALESCE((SELECT group_concat(g.name, ' ') FROM track_genres tg
                  JOIN genres g ON g.id = tg.genre_id
                  WHERE tg.track_id = NEW.id), '')
    );
END;

-- The genres of a track are rewritten wholesale (delete then insert), so
-- both ends refresh the same row from what is in the table at that moment.
CREATE TRIGGER track_search_genre_in AFTER INSERT ON track_genres
BEGIN
    UPDATE track_search
       SET genres = COALESCE((SELECT group_concat(g.name, ' ') FROM track_genres tg
                              JOIN genres g ON g.id = tg.genre_id
                              WHERE tg.track_id = NEW.track_id), '')
     WHERE rowid = NEW.track_id;
END;

CREATE TRIGGER track_search_genre_out AFTER DELETE ON track_genres
BEGIN
    UPDATE track_search
       SET genres = COALESCE((SELECT group_concat(g.name, ' ') FROM track_genres tg
                              JOIN genres g ON g.id = tg.genre_id
                              WHERE tg.track_id = OLD.track_id), '')
     WHERE rowid = OLD.track_id;
END;

-- What is already there, indexed once.
INSERT INTO track_search (rowid, title, artist, album, genres)
SELECT t.id,
       t.title,
       t.artist,
       COALESCE(a.title, ''),
       COALESCE((SELECT group_concat(g.name, ' ') FROM track_genres tg
                 JOIN genres g ON g.id = tg.genre_id
                 WHERE tg.track_id = t.id), '')
FROM tracks t LEFT JOIN albums a ON a.id = t.album_id;

ANALYZE;
"#;

/// From 7 to 8: the indexes a two-hundred-thousand-track library asks for.
///
/// V6 and V7 were measured against fifty thousand, and at that size these
/// did not show. At two hundred thousand, on disk, three things did, and
/// `EXPLAIN QUERY PLAN` named all three:
///
/// - **Most played, 78 ms.** `ORDER BY plays DESC, last_played DESC` used
///   the index for the first term and a temporary B-tree for the second.
///   Most played tracks have been played once, so tens of thousands tie on
///   the first term and every one of them got sorted. A composite index
///   gives the whole ordering.
/// - **The smart lists, 365 ms to count eight.** The rules filter on
///   `added_at` and `duration_ms`, and neither had an index, so a rule that
///   matches little walked the library to find out.
/// - The third was the search, and it is not an index: see `search`.
const MIGRATION_V8: &str = r#"
CREATE INDEX track_rollup_top ON track_rollup(plays DESC, last_played DESC);
CREATE INDEX album_rollup_top ON album_rollup(plays DESC, last_played DESC);

-- What the smart lists filter on. `present` goes first in both because
-- every list starts with it, so the index answers the whole condition
-- instead of narrowing it.
CREATE INDEX tracks_present_added ON tracks(present, added_at);
CREATE INDEX tracks_present_duration ON tracks(present, duration_ms);

-- "Played in the last week" asks the rollup, not the log.
CREATE INDEX track_rollup_last_played ON track_rollup(last_played DESC);

-- What the tempo lists filter on. Without it, a library with no analysis
-- yet walks itself to find out it has none.
CREATE INDEX audio_features_bpm ON audio_features(bpm);

ANALYZE;
"#;

/// From 8 to 9: one row of analysis per track and analyzer.
///
/// The table was keyed by `(track_id, analyzer, version)`, so bumping the
/// analyzer's version left the old row behind. Nothing read it on purpose --
/// every reader in `features.rs` already ends in `ORDER BY version DESC
/// LIMIT 1` -- but the tempo rules did read it by accident: they ask whether
/// **any** row for that track falls in the band, so a track whose tempo was
/// measured at 130 and later corrected to 90 was in "Calm" and in "To move
/// to" at the same time, forever.
///
/// Keyed by `(track_id, analyzer)` that cannot happen, and one more thing
/// follows from it: with at most one row per track, a tempo rule can be a
/// join instead of an `EXISTS`, and a join can be driven from the tempo
/// index. On two hundred thousand tracks that is 35 ms to 2 ms with the
/// analysis done, and 18 ms to 0.07 ms with none of it done -- which is the
/// state every library is in the first time it opens this tab.
///
/// Superseded rows are dropped, keeping the newest version of each.
const MIGRATION_V9: &str = r#"
CREATE TABLE audio_features_one (
    track_id         INTEGER NOT NULL REFERENCES tracks(id),
    analyzer         TEXT NOT NULL,
    version          INTEGER NOT NULL,
    bpm              REAL,
    tempo_confidence REAL,
    musical_key      TEXT,
    energy           REAL,
    loudness_lufs    REAL,
    intro_ms         INTEGER,
    outro_ms         INTEGER,
    analyzed_at      INTEGER NOT NULL,
    PRIMARY KEY (track_id, analyzer)
);

INSERT INTO audio_features_one
       (track_id, analyzer, version, bpm, tempo_confidence, musical_key,
        energy, loudness_lufs, intro_ms, outro_ms, analyzed_at)
SELECT f.track_id, f.analyzer, f.version, f.bpm, f.tempo_confidence,
       f.musical_key, f.energy, f.loudness_lufs, f.intro_ms, f.outro_ms,
       f.analyzed_at
  FROM audio_features f
 WHERE f.version = (SELECT max(o.version) FROM audio_features o
                     WHERE o.track_id = f.track_id AND o.analyzer = f.analyzer);

DROP TABLE audio_features;
ALTER TABLE audio_features_one RENAME TO audio_features;

-- The index V8 added went with the old table.
CREATE INDEX audio_features_bpm ON audio_features(bpm);

ANALYZE;
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db
    }

    /// A database stopped at an older version, with whatever was in it then.
    fn at_version(v: usize) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        for step in STEPS.iter().take(v) {
            db.execute_batch(step).unwrap();
        }
        db.execute_batch(&format!("PRAGMA user_version = {v}")).unwrap();
        db
    }

    #[test]
    fn an_older_library_is_brought_up_and_keeps_what_it_had() {
        // Nothing tested the upgrade path, which is the one every library
        // that already exists takes. This one starts where a shipped Chilpa
        // left it and walks it to today.
        let db = at_version(8);
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
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
        assert_eq!(
            db.query_row("SELECT count(*) FROM audio_features", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );

        migrate(&db).unwrap();

        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(),
            VERSION
        );
        let rows: Vec<(String, i64, f64)> = db
            .prepare("SELECT analyzer, version, bpm FROM audio_features ORDER BY analyzer")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            rows,
            vec![("chilpa".to_string(), 2, 90.0), ("other".to_string(), 1, 77.0)],
            "the newest of each analyzer, and the superseded one gone"
        );
        assert_eq!(
            db.query_row("SELECT title FROM tracks WHERE id = 1", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "One",
            "and the track it was about is still there"
        );

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
    fn every_version_in_between_can_be_walked_forward() {
        // A library can have stopped at any of them.
        for v in 0..STEPS.len() {
            let db = at_version(v);
            migrate(&db).unwrap_or_else(|e| panic!("from version {v}: {e}"));
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(),
                VERSION,
                "from version {v}"
            );
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
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
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
        let db = in_memory();
        db.execute("INSERT INTO genres (name) VALUES ('Rock')", []).unwrap();
        assert!(
            db.execute("INSERT INTO genres (name) VALUES ('rock')", []).is_err(),
            "different case is not two genres"
        );
    }

    #[test]
    fn a_playlist_never_repeats_a_position() {
        let db = in_memory();
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
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
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
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
        db.execute("INSERT INTO sources (provider, instance) VALUES ('local', 'default')", [])
            .unwrap();
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
        let (ms, bp): (i64, i64) = db
            .query_row("SELECT played_ms, bit_perfect FROM play_events", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((ms, bp), (180_000, 1));
    }
}
