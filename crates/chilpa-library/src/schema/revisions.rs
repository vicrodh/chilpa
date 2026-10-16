//! I separate edition inputs from listening, analysis and artwork writes.

pub(super) const MIGRATION_V43: &str = r#"
CREATE TABLE version_catalog_revision (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    revision INTEGER NOT NULL
);
INSERT INTO version_catalog_revision VALUES (1, 0);
CREATE TRIGGER version_catalog_albums_insert AFTER INSERT ON albums
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_albums_delete AFTER DELETE ON albums
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_albums_update AFTER UPDATE OF id, title, album_artist, folder ON albums
WHEN old.id IS NOT new.id OR old.title IS NOT new.title OR old.album_artist IS NOT new.album_artist OR old.folder IS NOT new.folder
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_tracks_insert AFTER INSERT ON tracks
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_tracks_delete AFTER DELETE ON tracks
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_tracks_update AFTER UPDATE OF id, album_id, source_id, external_id, duration_ms, bit_depth, sample_rate_hz, present, reachable, codec ON tracks
WHEN old.id IS NOT new.id OR old.album_id IS NOT new.album_id OR old.source_id IS NOT new.source_id OR old.external_id IS NOT new.external_id OR old.duration_ms IS NOT new.duration_ms OR old.bit_depth IS NOT new.bit_depth OR old.sample_rate_hz IS NOT new.sample_rate_hz OR old.present IS NOT new.present OR old.reachable IS NOT new.reachable OR old.codec IS NOT new.codec
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_sources_insert AFTER INSERT ON sources
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_sources_delete AFTER DELETE ON sources
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_sources_update AFTER UPDATE OF id, provider, instance, enabled, label, icon, kind ON sources
WHEN old.id IS NOT new.id OR old.provider IS NOT new.provider OR old.instance IS NOT new.instance OR old.enabled IS NOT new.enabled OR old.label IS NOT new.label OR old.icon IS NOT new.icon OR old.kind IS NOT new.kind
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_album_preferences_insert AFTER INSERT ON album_preferences
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_album_preferences_delete AFTER DELETE ON album_preferences
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_album_preferences_update AFTER UPDATE OF group_key, album_id ON album_preferences
WHEN old.group_key IS NOT new.group_key OR old.album_id IS NOT new.album_id
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_album_artist_rollup_insert AFTER INSERT ON album_artist_rollup
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_album_artist_rollup_delete AFTER DELETE ON album_artist_rollup
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER version_catalog_album_artist_rollup_update AFTER UPDATE OF album_id, artist, tracks ON album_artist_rollup
WHEN old.album_id IS NOT new.album_id OR old.artist IS NOT new.artist OR old.tracks IS NOT new.tracks
BEGIN UPDATE version_catalog_revision SET revision = revision + 1 WHERE id = 1; END;
"#;
