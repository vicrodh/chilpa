//! From 53 to 54: ratings kept in step with the source that holds the item.
//!
//! An origin is a server instance (`server:<instance>`) or a plugin
//! (`plugin:<id>`): never a source row id, which SQLite may hand to a server
//! added after another was removed.
//!
//! - `rating_origins`: whether the origin syncs its ratings (on unless a row
//!   says off), and when each kind of its ratings was first read whole.
//!   Before that, a rating the source holds is new to Chilpa, not a change.
//! - `remote_ratings`: what the source said last, raw on its own scale. A
//!   server's unrated items have no row once its kind was read whole; a
//!   plugin's have one, since its lists are never whole. `sent_ms` is when
//!   Chilpa last wrote it there: a listing asked for before then may be
//!   older than that write.
//! - `rating_outbox`: a change the listener made here, waiting to reach its
//!   source. One per item: a newer change replaces it. `asked` is the rating
//!   clock's revision of that change.
pub(super) const MIGRATION_V54: &str = r#"
CREATE TABLE rating_origins (
    origin      TEXT PRIMARY KEY,
    enabled     INTEGER NOT NULL DEFAULT 1,
    tracks_at   INTEGER,
    albums_at   INTEGER
);
CREATE TABLE remote_ratings (
    origin      TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK(kind IN ('track', 'album')),
    external_id TEXT NOT NULL,
    raw         REAL NOT NULL,
    read_at     INTEGER NOT NULL DEFAULT 0,
    sent_ms     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (origin, kind, external_id)
);
CREATE TABLE rating_outbox (
    origin      TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK(kind IN ('track', 'album')),
    external_id TEXT NOT NULL,
    stars       INTEGER NOT NULL CHECK(stars BETWEEN 0 AND 5),
    asked       INTEGER NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0,
    next_at     INTEGER NOT NULL DEFAULT 0,
    error       TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (origin, kind, external_id)
);
CREATE INDEX rating_outbox_due ON rating_outbox(next_at);
"#;
