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
