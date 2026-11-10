//! Audio copies have their own registry, independent of artwork caches.

pub(super) const MIGRATION_V53: &str = r#"
CREATE TABLE offline_audio (
    key TEXT PRIMARY KEY,
    source_id INTEGER NOT NULL,
    account TEXT NOT NULL,
    identity TEXT NOT NULL,
    entry TEXT NOT NULL,
    revision TEXT NOT NULL,
    profile TEXT NOT NULL DEFAULT 'original',
    path TEXT NOT NULL,
    bytes INTEGER NOT NULL DEFAULT 0 CHECK(bytes >= 0),
    checksum TEXT NOT NULL DEFAULT '',
    content_type TEXT NOT NULL DEFAULT '',
    codec TEXT NOT NULL DEFAULT '',
    extension TEXT NOT NULL DEFAULT '',
    validator TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL DEFAULT 'queued',
    error TEXT NOT NULL DEFAULT '',
    changed_at INTEGER NOT NULL DEFAULT 0,
    UNIQUE(source_id, account, identity, entry, revision, profile)
);
CREATE INDEX offline_audio_entry ON offline_audio(source_id, account, identity, entry, state);
CREATE TABLE offline_collections (
    collection TEXT NOT NULL,
    key TEXT NOT NULL REFERENCES offline_audio(key),
    PRIMARY KEY(collection, key)
);
"#;
