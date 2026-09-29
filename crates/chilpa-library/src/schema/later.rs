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
