//! I give playlists identities that survive renaming and cannot be recycled with a row id.
pub(super) const MIGRATION_V45: &str = r#"
CREATE TABLE playlist_sync_keys (
    playlist_id INTEGER PRIMARY KEY REFERENCES playlists(id) ON DELETE CASCADE,
    key TEXT NOT NULL UNIQUE
);
INSERT INTO playlist_sync_keys(playlist_id,key) SELECT id,lower(hex(randomblob(16))) FROM playlists;
CREATE TRIGGER playlist_sync_key_insert AFTER INSERT ON playlists BEGIN
    INSERT INTO playlist_sync_keys(playlist_id,key) VALUES (new.id,lower(hex(randomblob(16))));
END;
"#;
