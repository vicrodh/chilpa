//! Albums taken out of the library without touching their files. Each of
//! their tracks is kept here while it is hidden, with whether it was there
//! when it was taken out, so bringing it back leaves it as it was. A scan or
//! a server's sync that finds the track again keeps it hidden.
pub(super) const MIGRATION_V58: &str = r#"
CREATE TABLE removed_tracks (
    track_id    INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    was_present INTEGER NOT NULL,
    removed_at  INTEGER NOT NULL
);
"#;
