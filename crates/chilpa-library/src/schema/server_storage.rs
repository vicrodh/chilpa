//! Keep each server's selected offline destination across connection edits.

pub(super) const MIGRATION_V52: &str = r#"
ALTER TABLE servers ADD COLUMN offline_folder TEXT NOT NULL DEFAULT '';
"#;
