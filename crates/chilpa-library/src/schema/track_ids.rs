//! From 56 to 57: a track's id is never given to another track.
//!
//! `INTEGER PRIMARY KEY` hands out one past the highest id there is, so
//! deleting the newest track and adding one gave the new track the deleted
//! one's id, and whatever still held that id (a selection, a snapshot) took
//! one track for the other. `AUTOINCREMENT` never reuses an id, for every
//! insert, present and future, and SQLite keeps it, not the code that
//! inserts. The table is rebuilt to get it; ids, rows, indexes and triggers
//! stay as they were.
//!
//! An insert that finds the track already there and updates it still takes
//! a number from the sequence, so a server's sync spends one per track it
//! writes again and ids grow sparse. That is expected: an id is a 64-bit
//! integer, and nothing reads meaning into how dense they are.

use super::rebuild::Rebuild;

pub(super) const TRACKS: Rebuild = Rebuild {
    table: "tracks",
    columns: &[
        "id", "source_id", "external_id", "title", "artist", "album_id", "track_number", "disc_number", "duration_ms",
        "sample_rate_hz", "bit_depth", "channels", "codec", "isrc", "recording_mbid", "present", "added_at",
        "album_credit", "album_title", "disc_total", "release_part", "release_disc", "release_year", "release_artwork",
        "release_evidence_read", "reachable", "sync_generation", "revision", "label", "explicit", "release_date",
        "disc_subtitle", "work",
    ],
    create: r#"
CREATE TABLE tracks_new (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
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
    present        INTEGER NOT NULL DEFAULT 1,
    added_at       INTEGER NOT NULL,
    album_credit   TEXT NOT NULL DEFAULT '',
    album_title    TEXT NOT NULL DEFAULT '',
    disc_total     INTEGER,
    release_part   TEXT NOT NULL DEFAULT '',
    release_disc   INTEGER,
    release_year   INTEGER,
    release_artwork TEXT,
    release_evidence_read INTEGER NOT NULL DEFAULT 0,
    reachable      INTEGER NOT NULL DEFAULT 1,
    sync_generation INTEGER,
    revision       TEXT,
    label          TEXT,
    explicit       INTEGER,
    release_date   TEXT,
    disc_subtitle  TEXT,
    work           TEXT,
    UNIQUE (source_id, external_id)
);
"#,
};
