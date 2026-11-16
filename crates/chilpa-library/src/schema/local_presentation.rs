//! File evidence for explicit local album membership. Track identity is retained.
pub(super) const MIGRATION_V56: &str = r#"
CREATE TABLE local_release_members (
    track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    group_key TEXT NOT NULL,
    title TEXT NOT NULL,
    artist TEXT NOT NULL,
    evidence TEXT NOT NULL
);
CREATE INDEX local_release_group ON local_release_members(group_key,track_id);
CREATE TRIGGER version_catalog_local_presentation_update AFTER UPDATE OF version_key ON albums
WHEN old.version_key IS NOT new.version_key
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
"#;
