//! I cache successful metadata reads and retain contributions by entity.

pub(super) const MIGRATION_V44: &str = r#"
CREATE TABLE metadata_cache (
    key TEXT PRIMARY KEY,
    fetched_at INTEGER NOT NULL,
    bytes INTEGER NOT NULL,
    document TEXT NOT NULL
);
CREATE INDEX metadata_cache_age ON metadata_cache(fetched_at, key);
CREATE TABLE contributors (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL
);
CREATE TABLE metadata_contributions (
    cache_key TEXT NOT NULL REFERENCES metadata_cache(key) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    contributor_id TEXT NOT NULL REFERENCES contributors(id),
    credited_name TEXT NOT NULL,
    role TEXT NOT NULL,
    relation TEXT NOT NULL,
    attributes TEXT NOT NULL,
    scope TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    entity_title TEXT NOT NULL,
    PRIMARY KEY(cache_key, position)
);
CREATE INDEX metadata_contributor ON metadata_contributions(contributor_id);
CREATE INDEX metadata_role ON metadata_contributions(role, contributor_id);
"#;
