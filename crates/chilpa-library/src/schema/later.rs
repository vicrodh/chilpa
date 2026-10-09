//! The steps from version 29 on: the file of the earlier ones is full.

/// From 28 to 29: playlists take stars, and playlists and artists can be
/// favorites.
///
/// An artist is a name here, not a row: the same artist written with other
/// capitals is one artist on screen, so the mark goes by the name folded to
/// lower case, and the name as it was written is kept beside it.
pub(super) const MIGRATION_V29: &str = r#"
CREATE TABLE playlist_ratings (
    playlist_id INTEGER PRIMARY KEY REFERENCES playlists(id) ON DELETE CASCADE,
    stars INTEGER NOT NULL CHECK(stars BETWEEN 0 AND 5),
    revision INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE playlist_favorites (
    playlist_id INTEGER PRIMARY KEY REFERENCES playlists(id) ON DELETE CASCADE,
    added_at INTEGER NOT NULL
);
CREATE TABLE artist_favorites (
    folded   TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    added_at INTEGER NOT NULL
);
"#;

/// From 29 to 30: what the listener keeps from a source that is browsed.
///
/// A station, a show: something a plugin listed and the listener wants at
/// hand. It is kept here and not by the plugin, so it survives the plugin
/// being reinstalled and can be listed with the network down. The row
/// carries what is needed to show it again without asking anybody; the
/// `external_id` is whatever the plugin wants back to open or play it.
pub(super) const MIGRATION_V30: &str = r#"
CREATE TABLE source_saved (
    source      TEXT NOT NULL,
    external_id TEXT NOT NULL,
    title       TEXT NOT NULL,
    subtitle    TEXT NOT NULL DEFAULT '',
    image_url   TEXT NOT NULL DEFAULT '',
    kind        TEXT NOT NULL,
    live        INTEGER NOT NULL DEFAULT 0,
    saved_at    INTEGER NOT NULL,
    PRIMARY KEY (source, external_id)
);
"#;

/// From 30 to 31: how far the listener got in something of a source that
/// has an end, like an episode.
///
/// The row is also the card of the entry -- its title, whose it is, its
/// picture -- because a queue names such an entry only by its id, and the
/// player has to say what is playing after a restart, with nobody to ask.
/// `played` is the listener's "I heard this one", which reaching the end
/// sets and which can be set by hand.
pub(super) const MIGRATION_V31: &str = r#"
CREATE TABLE source_progress (
    source      TEXT NOT NULL,
    external_id TEXT NOT NULL,
    title       TEXT NOT NULL,
    subtitle    TEXT NOT NULL DEFAULT '',
    image_url   TEXT NOT NULL DEFAULT '',
    duration_ms INTEGER NOT NULL DEFAULT 0,
    position_ms INTEGER NOT NULL DEFAULT 0,
    played      INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (source, external_id)
);
"#;

/// From 31 to 32: personal stars outlive a remote catalog cache or account session.
pub(super) const MIGRATION_V32: &str = r#"
CREATE TABLE external_ratings (
    source TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('track', 'album', 'playlist')),
    external_id TEXT NOT NULL,
    stars INTEGER NOT NULL CHECK(stars BETWEEN 0 AND 5),
    revision INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (source, kind, external_id)
);
"#;

/// From 32 to 33: who wrote each track and the label it came out on, for
/// the album page and the library's columns. Composers go in their own
/// table, as genres do: a track has several. Every file is read again at
/// the next scan, since nothing read so far kept either.
pub(super) const MIGRATION_V33: &str = r#"
CREATE TABLE composers (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE track_composers (
    track_id    INTEGER NOT NULL REFERENCES tracks(id),
    composer_id INTEGER NOT NULL REFERENCES composers(id),
    PRIMARY KEY (track_id, composer_id)
);

CREATE INDEX track_composers_composer ON track_composers(composer_id);

ALTER TABLE tracks ADD COLUMN label TEXT;
CREATE INDEX tracks_label ON tracks(label COLLATE NOCASE) WHERE label IS NOT NULL;

UPDATE tracks SET release_evidence_read = 0;
"#;

/// The covers and pictures the listener chose. A record's goes with it; an
/// artist's is by name folded to lower case, like a favorite; a record of a
/// plugin's is by the plugin and the identity it says is stable. Each names
/// a file the facade keeps.
pub(super) const MIGRATION_V34: &str = r#"
CREATE TABLE album_artwork (
    album_id INTEGER PRIMARY KEY REFERENCES albums(id) ON DELETE CASCADE,
    path     TEXT NOT NULL
);

CREATE TABLE artist_artwork (
    folded TEXT PRIMARY KEY,
    path   TEXT NOT NULL
);

CREATE TABLE remote_artwork (
    origin   TEXT NOT NULL,
    identity TEXT NOT NULL,
    path     TEXT NOT NULL,
    PRIMARY KEY (origin, identity)
);
"#;
