//! What each version of the schema is, as the SQL that gets it there.
//!
//! Split out of `schema.rs` because that file crossed the fifteen-hundred
//! line lock in ADR-001. Nothing here is machinery: which step leads to
//! which version, what is done before one runs and what after, all stays
//! next door. This is the record of what the schema has been.
//!
//! **A step is never edited once it has shipped.** A database that already
//! applied it will not apply it again, so changing it makes two different
//! shapes of the same version. What one of them got wrong is fixed by
//! writing another, and several here do exactly that.

pub(super) const SCHEMA_V1: &str = r#"
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
pub(super) const MIGRATION_V2: &str = r#"
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
pub(super) const MIGRATION_V3: &str = r#"
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
pub(super) const MIGRATION_V4: &str = r#"
ALTER TABLE audio_features ADD COLUMN tempo_confidence REAL;
"#;

/// From 4 to 5: an index over `(album_id, present)`.
///
/// Without it, SQLite picked `tracks_present` for `album_id = ? AND
/// present = 1` and, since every track is present, walked the whole table
/// for every album. Measured with five thousand albums: the grid went from
/// 30 seconds to 7 milliseconds. It is the same finding as the QBZ audit,
/// wearing another face.
pub(super) const MIGRATION_V5: &str = r#"
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
pub(super) const MIGRATION_V6: &str = r#"
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
pub(super) const MIGRATION_V7: &str = r#"
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
pub(super) const MIGRATION_V8: &str = r#"
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
pub(super) const MIGRATION_V9: &str = r#"
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

/// From 9 to 10: the totals the stats view adds up.
///
/// `stats()` summed the whole play log. That was the last query in the
/// library that grew with something other than its answer, and the thing it
/// grew with was the worst one: not how much music somebody has, but how
/// long they have been using Chilpa. Forty thousand plays read in 33 ms, and
/// somebody who listens for five years has a log ten times that.
///
/// Six numbers, one row, kept by the same kind of trigger as the other
/// rollups. Five of them are running totals. The sixth, how many different
/// tracks have been played, is the one that is not a sum, and it is exact
/// anyway: a play adds one when the log holds no earlier play of that track,
/// which is an index probe. That keeps what the old query counted, including
/// a track that has since left the library -- the log remembers it was
/// played and so does this.
///
/// Nothing deletes from `play_events`, so there is no trigger for it. If
/// that ever changes, this needs one, and `rebuild_rollups` is the way back
/// in the meantime.
pub(super) const MIGRATION_V10: &str = r#"
CREATE TABLE library_totals (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    plays       INTEGER NOT NULL DEFAULT 0,
    listened_ms INTEGER NOT NULL DEFAULT 0,
    tracks      INTEGER NOT NULL DEFAULT 0,
    skips       INTEGER NOT NULL DEFAULT 0,
    bit_perfect INTEGER NOT NULL DEFAULT 0,
    since       INTEGER
);

INSERT INTO library_totals (id) VALUES (1);

CREATE TRIGGER library_totals_play AFTER INSERT ON play_events
BEGIN
    UPDATE library_totals
       SET plays = plays + (CASE WHEN NEW.duration_ms = 0
                                   OR NEW.played_ms * 2 >= NEW.duration_ms
                                 THEN 1 ELSE 0 END),
           listened_ms = listened_ms + COALESCE(NEW.played_ms, 0),
           skips = skips + COALESCE(NEW.skipped, 0),
           bit_perfect = bit_perfect + COALESCE(NEW.bit_perfect, 0),
           tracks = tracks + (CASE WHEN NEW.track_id IS NOT NULL
                                    AND NOT EXISTS (SELECT 1 FROM play_events p
                                                     WHERE p.track_id = NEW.track_id
                                                       AND p.rowid <> NEW.rowid)
                                   THEN 1 ELSE 0 END),
           -- min() over a NULL is NULL, and the first play is the one that
           -- finds it NULL.
           since = COALESCE(min(since, NEW.started_at), NEW.started_at)
     WHERE id = 1;
END;

-- What is already in the log.
UPDATE library_totals
   SET plays = COALESCE((SELECT sum(CASE WHEN duration_ms = 0
                                           OR played_ms * 2 >= duration_ms
                                         THEN 1 ELSE 0 END) FROM play_events), 0),
       listened_ms = COALESCE((SELECT sum(played_ms) FROM play_events), 0),
       tracks = (SELECT count(DISTINCT track_id) FROM play_events),
       skips = COALESCE((SELECT sum(skipped) FROM play_events), 0),
       bit_perfect = COALESCE((SELECT sum(bit_perfect) FROM play_events), 0),
       since = (SELECT min(started_at) FROM play_events)
 WHERE id = 1;

ANALYZE;
"#;

/// From 10 to 11: the grid stops sorting the whole library to show a page.
///
/// `albums` had no index on the columns the grid sorts by, so every page
/// walked all twenty thousand albums into a temporary B-tree and took a
/// hundred off the top. Three indexes, in the collation the ordering uses --
/// an index on `title` is no use to `ORDER BY title COLLATE NOCASE`.
///
/// The indexes alone changed nothing, and that is the part worth writing
/// down. The grid's inner query joins `album_rollup` to drop albums with no
/// present tracks, and the planner has no way to know that keeps nearly
/// everything: a range constraint it cannot see statistics for is guessed at
/// one in four. On that guess, walking the title index and probing the
/// rollup looks like four hundred probes per hundred rows and sorting
/// everything looks cheaper. `likely()` is the supported way to say what is
/// true -- almost every album has tracks -- and with it the planner takes
/// the index and stops at a hundred.
///
/// On two hundred thousand tracks in twenty thousand albums, the page:
///
/// | | before | after |
/// | --- | --- | --- |
/// | by title | 8.8 ms | 0.44 ms |
/// | by artist | 10.2 ms | 0.59 ms |
/// | by year | 9.6 ms | 0.58 ms |
/// | by date added | 15.1 ms | 0.78 ms |
/// | by last played | 15.4 ms | 0.61 ms |
/// | by plays | 11.0 ms | 2.9 ms |
/// | the artists | 20 ms | 6.8 ms |
///
/// Sorting by last played also stopped wrapping the column in a COALESCE to
/// push what never played to the end. SQLite already sorts a null last on
/// DESC, so it was the same order, and the COALESCE was what kept the index
/// from serving it.
///
/// Sorting by plays is the one that still ends in a temporary B-tree, and it
/// is clear about why: the tiebreak is the album's title, which lives in the
/// other table, so albums tied on plays get sorted. The first page -- the
/// most played -- has few ties and comes out in 2.9 ms. Page 190 of 200 is
/// the bottom of the ordering, where every album with the fewest plays is
/// one group, and that is 37 ms. It is the page nobody opens, and paging by
/// offset reads the rows it skips whatever the ordering, so the way out of
/// that one is not an index.
pub(super) const MIGRATION_V11: &str = r#"
CREATE INDEX albums_title ON albums(title COLLATE NOCASE);
CREATE INDEX albums_artist_year_title
    ON albums(album_artist COLLATE NOCASE, year, title COLLATE NOCASE);
CREATE INDEX albums_year_title ON albums(year DESC, title COLLATE NOCASE);

ANALYZE;
"#;

/// From 11 to 12: the search index learns short prefixes.
///
/// Typing is the whole point of a search box, and every keystroke is a
/// prefix query. FTS5 answers one by building the whole matching doclist
/// before it returns a row, so the cost is the term's reach and not the
/// LIMIT: `"alb"*` over two hundred thousand tracks took 11 ms whether it
/// was asked for five thousand rows or five hundred. Three keystrokes into
/// a search, that is a whole frame gone.
///
/// `prefix='2 3 4'` keeps a separate index for prefixes of those lengths,
/// which is exactly where typing lives. The match itself goes from 11.3 ms
/// to 0.66 ms, and a whole search -- two to four letters typed, two hundred
/// results out -- from 12 ms to 4.4 ms. What is left of those 4.4 is sorting
/// the matches it considers to take the alphabetically first two hundred,
/// and that cost is `CONSIDERED`, not the index. Prefixes longer than four
/// letters still walk the term index and are unchanged.
///
/// It costs room: the index goes from 18 MB to 33 MB on two hundred thousand
/// tracks, about 17 per cent of the library file. That is the trade, and it
/// is worth it -- what it buys happens on every keystroke, and what it does
/// not help happens once, when the word is fully typed.
///
/// An FTS5 table cannot be altered into another one, so it is dropped and
/// made again, and the triggers with it because they name it. Their text is
/// the same as in `MIGRATION_V7` and it is repeated rather than shared: a
/// migration is a record of what a version did and cannot be edited later,
/// so two copies is what honesty costs here. What keeps the rebuild honest
/// is `rebuilding_the_search_index_leaves_every_trigger_working`, which
/// drives all five of them after the migration has run.
pub(super) const MIGRATION_V12: &str = r#"
DROP TRIGGER track_search_in;
DROP TRIGGER track_search_out;
DROP TRIGGER track_search_changed;
DROP TRIGGER track_search_genre_in;
DROP TRIGGER track_search_genre_out;
DROP TABLE track_search;

CREATE VIRTUAL TABLE track_search USING fts5(
    title, artist, album, genres,
    tokenize = 'unicode61 remove_diacritics 2',
    prefix = '2 3 4'
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

/// From 12 to 13: a track with no album stops inventing one.
///
/// `album_rollup_track_moved` fires whenever a track's `present`, `added_at`
/// or `album_id` changes, and it began with an unguarded
/// `INSERT INTO album_rollup (album_id, added_at) VALUES (NEW.album_id, ...)`.
/// When the track has no album that inserts a NULL into a column that is
/// `INTEGER PRIMARY KEY`, so SQLite makes up a rowid, and the result is a
/// rollup row for an album that does not exist.
///
/// With `foreign_keys` on -- which is every library on disk -- that is not
/// junk, it is an error, and it comes out of the statement a scan runs on
/// every file it finds again: marking a track present. So a library holding
/// one loose file, not in any album, could not be rescanned at all.
///
/// Nothing caught it because `Library::in_memory` did not apply the pragmas,
/// so every test in this crate ran against a database with foreign keys
/// switched off. That is fixed in the same commit, and it is the more
/// important half: this bug was a consequence.
///
/// The sibling trigger on INSERT already had the guard. This gives the same
/// one to the UPDATE, as a `SELECT ... WHERE` rather than a trigger-level
/// `WHEN`, because the second half of the trigger still has to run when the
/// track moves out of an album and into none -- that is exactly when the
/// album it left has to be counted again.
pub(super) const MIGRATION_V13: &str = r#"
DROP TRIGGER album_rollup_track_moved;

CREATE TRIGGER album_rollup_track_moved AFTER UPDATE OF album_id, added_at, present ON tracks
BEGIN
    INSERT INTO album_rollup (album_id, added_at)
    SELECT NEW.album_id, NEW.added_at WHERE NEW.album_id IS NOT NULL
    ON CONFLICT(album_id) DO UPDATE SET added_at = max(added_at, NEW.added_at);
    UPDATE album_rollup
       SET present_tracks = (SELECT count(*) FROM tracks t
                             WHERE t.album_id = album_rollup.album_id AND t.present = 1)
     WHERE album_id IN (OLD.album_id, NEW.album_id);
END;

-- What the old one made up, in libraries that had foreign keys off.
DELETE FROM album_rollup
 WHERE album_id NOT IN (SELECT id FROM albums);

ANALYZE;
"#;

/// From 13 to 14: an album is one row, not one row per track in it.
///
/// `albums` carried `UNIQUE (title, album_artist, mbid)`, and the scan
/// writes an album with no mbid, because a file's tags rarely carry one. In
/// SQLite a unique index lets nulls through -- two nulls are not equal to
/// each other -- so that constraint rejected nothing, and the scan's
/// `INSERT OR IGNORE` never once ignored.
///
/// Every track of an album therefore wrote its own `albums` row. The lookup
/// right after it takes the lowest id, so the tracks all ended up pointing
/// at the same album and the library looked right; what was wrong was
/// underneath. A ten-track album left nine rows nothing points at, and a
/// hundred thousand tracks leave a hundred thousand of them.
///
/// They are also what hid `MIGRATION_V13`'s bug for a while: `album_rollup`
/// has `album_id INTEGER PRIMARY KEY`, so a row inserted with a null gets a
/// made-up id, and whether that is a foreign key error or silent corruption
/// comes down to whether the id it makes up happens to name one of these
/// leftovers.
///
/// The fix is an index that says what identity means with a null in it. The
/// old table constraint stays -- taking it off means rebuilding a table two
/// others point at -- and it is harmless: with an mbid it is a weaker form
/// of the same rule, and without one it never did anything.
///
/// Before that, the leftovers are merged: whatever a duplicate knew that the
/// one being kept did not -- its cover, its year -- moves over, then the
/// tracks are pointed at the survivor and the rest go. `album_rollup`
/// follows them by its own cascade.
pub(super) const MIGRATION_V14: &str = r#"
-- What a duplicate knew and the survivor did not.
UPDATE albums
   SET artwork_path = COALESCE(artwork_path,
         (SELECT other.artwork_path FROM albums other
           WHERE other.title = albums.title
             AND other.album_artist = albums.album_artist
             AND ifnull(other.mbid, '') = ifnull(albums.mbid, '')
             AND other.artwork_path IS NOT NULL
           LIMIT 1)),
       year = COALESCE(year,
         (SELECT other.year FROM albums other
           WHERE other.title = albums.title
             AND other.album_artist = albums.album_artist
             AND ifnull(other.mbid, '') = ifnull(albums.mbid, '')
             AND other.year IS NOT NULL
           LIMIT 1))
 WHERE id IN (SELECT min(id) FROM albums
               GROUP BY title, album_artist, ifnull(mbid, ''));

-- Every track onto the one that is being kept.
UPDATE tracks
   SET album_id = (SELECT min(other.id) FROM albums other
                    JOIN albums mine ON mine.id = tracks.album_id
                   WHERE other.title = mine.title
                     AND other.album_artist = mine.album_artist
                     AND ifnull(other.mbid, '') = ifnull(mine.mbid, ''))
 WHERE album_id IS NOT NULL;

DELETE FROM albums
 WHERE id NOT IN (SELECT min(id) FROM albums
                   GROUP BY title, album_artist, ifnull(mbid, ''));

CREATE UNIQUE INDEX albums_identity
    ON albums(title, album_artist, ifnull(mbid, ''));

ANALYZE;
"#;

/// From 14 to 15: the prefix index covers a whole word, not four letters.
///
/// `MIGRATION_V12` gave the search index `prefix='2 3 4'` and measured the
/// case it helps. Measuring the rest, on two hundred thousand tracks, says
/// the trade was the wrong way round -- a prefix index makes the term index
/// it is stored beside bigger, so a prefix it does **not** cover gets slower:
///
/// | typed | no prefix index | `2 3 4` | `2 3 4 5 6 7` |
/// | --- | --- | --- | --- |
/// | `A` | 38 ms | 84 ms | 35 ms |
/// | `Al` | 8.4 ms | 0.86 ms | 0.53 ms |
/// | `Albu` | 8.6 ms | 2.7 ms | 0.49 ms |
/// | `Album` | 7.3 ms | 15.8 ms | 0.43 ms |
/// | `Artist` | 7.6 ms | 30 ms | 0.44 ms |
///
/// The query that matters most is the last one, the whole word, whose
/// results somebody actually reads -- and `2 3 4` made exactly that one four
/// times slower than no index at all. Covering two through seven makes every
/// length anybody types come out in half a millisecond, and costs 4 MB more
/// than the narrow one: 18 MB with no prefix index, 33 with `2 3 4`, 37 with
/// this.
///
/// One letter is the one that stays slow, and no index fixes it: a single
/// character matches most of the library, and FTS5 builds the whole doclist
/// before it returns a row. That is answered by not searching on one
/// character, which is the window's job and not the schema's.
pub(super) const MIGRATION_V15: &str = r#"
DROP TRIGGER track_search_in;
DROP TRIGGER track_search_out;
DROP TRIGGER track_search_changed;
DROP TRIGGER track_search_genre_in;
DROP TRIGGER track_search_genre_out;
DROP TABLE track_search;

CREATE VIRTUAL TABLE track_search USING fts5(
    title, artist, album, genres,
    tokenize = 'unicode61 remove_diacritics 2',
    prefix = '2 3 4 5 6 7'
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

