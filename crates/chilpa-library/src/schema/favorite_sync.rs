//! From 54 to 55: favorites kept in step with the server that holds the item,
//! as ratings are (54). The hearts themselves stay in `track_favorites` and
//! `album_favorites`; here is what each server said last and the listener's
//! changes on their way to it, with a clock of their own.
pub(super) const MIGRATION_V55: &str = r#"
CREATE TABLE favorite_origins (
    origin    TEXT PRIMARY KEY,
    enabled   INTEGER NOT NULL DEFAULT 1,
    tracks_at INTEGER,
    albums_at INTEGER
);
CREATE TABLE remote_favorites (
    origin      TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK(kind IN ('track', 'album')),
    external_id TEXT NOT NULL,
    favorite    INTEGER NOT NULL CHECK(favorite IN (0, 1)),
    read_at     INTEGER NOT NULL DEFAULT 0,
    sent_ms     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (origin, kind, external_id)
);
CREATE TABLE favorite_outbox (
    origin      TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK(kind IN ('track', 'album')),
    external_id TEXT NOT NULL,
    favorite    INTEGER NOT NULL CHECK(favorite IN (0, 1)),
    asked       INTEGER NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0,
    next_at     INTEGER NOT NULL DEFAULT 0,
    error       TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (origin, kind, external_id)
);
CREATE INDEX favorite_outbox_due ON favorite_outbox(next_at);
CREATE TABLE favorite_clock (id INTEGER PRIMARY KEY CHECK(id = 1), revision INTEGER NOT NULL);
INSERT INTO favorite_clock VALUES (1, 0);
"#;
