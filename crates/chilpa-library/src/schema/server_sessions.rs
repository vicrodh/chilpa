//! Additive session identities leave existing server mirrors intact.

pub(super) const MIGRATION_V51: &str = r#"
ALTER TABLE servers ADD COLUMN session_revision INTEGER NOT NULL DEFAULT 0;
"#;
