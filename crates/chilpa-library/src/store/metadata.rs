//! I retain positive metadata permanently; only confirmed absence expires.
//! Reads do not write. Replacements clean only their own orphaned contributors.

use super::Library;
use chilpa_models::credits::Contribution;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub const FRESH_SECONDS: i64 = 60 * 24 * 60 * 60;
const BATCH_BYTES: usize = 8 * 1024 * 1024;
const BATCH_ENTRIES: usize = 2049;
const ENTRY_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug)]
pub struct CachedMetadata {
    pub fetched_at: i64,
    pub document: Value,
    pub contributions: Vec<Contribution>,
}

impl CachedMetadata {
    pub fn fresh(&self, now: i64) -> bool {
        !self.document.is_null() || (now >= self.fetched_at && now - self.fetched_at < FRESH_SECONDS)
    }
}

impl Library {
    pub fn metadata_cached(&self, key: &str) -> rusqlite::Result<Option<CachedMetadata>> {
        let row: Option<(i64, String)> = self.db.query_row(
            "SELECT fetched_at, document FROM metadata_cache WHERE key = ?1", [key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).optional()?;
        let Some((fetched_at, document)) = row else { return Ok(None); };
        let document = serde_json::from_str(&document).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let mut statement = self.db.prepare("SELECT c.contributor_id, p.name, c.credited_name, c.role, c.relation, c.attributes, c.scope, c.entity_id, c.entity_title
            FROM metadata_contributions c JOIN contributors p ON p.id = c.contributor_id WHERE c.cache_key = ?1 ORDER BY c.position")?;
        let contributions = statement.query_map([key], |r| {
            let attributes: String = r.get(5)?;
            let attributes: Value = serde_json::from_str(&attributes).map_err(|_| rusqlite::Error::InvalidQuery)?;
            serde_json::from_value(json!({"contributor":{"id":r.get::<_, String>(0)?, "name":r.get::<_, String>(1)?},
                "credited_name":r.get::<_, String>(2)?, "role":r.get::<_, String>(3)?, "relation":r.get::<_, String>(4)?, "attributes":attributes,
                "scope":r.get::<_, String>(6)?, "entity_id":r.get::<_, String>(7)?, "entity_title":r.get::<_, String>(8)?}))
                .map_err(|_| rusqlite::Error::InvalidQuery)
        })?.collect::<rusqlite::Result<Vec<Contribution>>>()?;
        Ok(Some(CachedMetadata { fetched_at, document, contributions }))
    }

    /// I replace a complete answer atomically, preserving every typed credit.
    /// An unavailable provider never calls this method with an empty answer.
    pub fn cache_metadata(&self, key: &str, fetched_at: i64, document: &Value, contributions: &[Contribution]) -> rusqlite::Result<()> {
        self.store_metadata_batch(fetched_at, &[(key.to_owned(), document.clone(), contributions.to_vec())])
    }

    /// I replace a release and its recording credits together, without evicting other albums.
    pub fn store_metadata_batch(&self, fetched_at: i64, entries: &[(String, Value, Vec<Contribution>)]) -> rusqlite::Result<()> {
        if entries.is_empty() || entries.len() > BATCH_ENTRIES { return Err(rusqlite::Error::InvalidQuery); }
        let mut prepared = Vec::with_capacity(entries.len());
        let mut total = 0usize;
        let mut keys = std::collections::HashSet::new();
        for (key, document, credits) in entries {
            let document = document.to_string();
            let records: Vec<String> = credits.iter().map(|credit| serde_json::to_string(credit).unwrap()).collect();
            let size = key.len() + document.len() + records.iter().map(|s| s.len() * 2 + 256).sum::<usize>();
            total = total.saturating_add(size);
            if key.len() > 4096 || size > ENTRY_BYTES || credits.len() > 2048 || total > BATCH_BYTES
                || !keys.insert(key) || credits.iter().any(|c| c.contributor.id.is_empty()) {
                return Err(rusqlite::Error::InvalidQuery);
            }
            prepared.push((key, document, credits, size));
        }
        let tx = self.db.unchecked_transaction()?;
        for (key, document, credits, size) in prepared {
            let mut previous = std::collections::HashSet::<String>::new();
            {
                let mut query = tx.prepare("SELECT contributor_id FROM metadata_contributions WHERE cache_key = ?1")?;
                for id in query.query_map([key], |row| row.get::<_, String>(0))? { previous.insert(id?); }
            }
            tx.execute("DELETE FROM metadata_cache WHERE key = ?1", [key])?;
            tx.execute("INSERT INTO metadata_cache(key, fetched_at, bytes, document) VALUES (?1, ?2, ?3, ?4)",
                params![key, fetched_at, size as i64, document])?;
            for (position, credit) in credits.iter().enumerate() {
                tx.execute("INSERT INTO contributors(id, name) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET name = excluded.name",
                    params![credit.contributor.id, credit.contributor.name])?;
                let role = serde_json::to_value(credit.role).unwrap();
                let scope = serde_json::to_value(credit.scope).unwrap();
                tx.execute("INSERT INTO metadata_contributions(cache_key, position, contributor_id, credited_name, role, relation, attributes, scope, entity_id, entity_title)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![key, position as i64, credit.contributor.id, credit.credited_name, role.as_str().unwrap(), credit.relation,
                        serde_json::to_string(&credit.attributes).unwrap(), scope.as_str().unwrap(), credit.entity_id, credit.entity_title])?;
            }
            // I bound cleanup memory to one replaced entry; both lookups use indexes.
            for id in previous {
                tx.execute("DELETE FROM contributors WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM metadata_contributions WHERE contributor_id = ?1)", [id])?;
            }
        }
        tx.commit()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_models::credits::{Contributor, CreditRole, CreditScope};
    use serde_json::json;

    fn producer() -> Contribution {
        Contribution { contributor: Contributor { id: "person:1".into(), name: "Producer".into() }, credited_name: "P.".into(),
            role: CreditRole::Producer, relation: "producer".into(), attributes: vec!["additional".into()], scope: CreditScope::Recording,
            entity_id: "recording:1".into(), entity_title: "Song".into() }
    }

    #[test]
    fn positive_credits_survive_reopening_and_never_expire() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let library = Library::open(&path).unwrap();
        let revision = library.db.query_row("SELECT revision FROM version_catalog_revision", [], |r| r.get::<_, i64>(0)).unwrap();
        library.cache_metadata("recording:1", 100, &json!({"title":"Song"}), &[producer()]).unwrap();
        assert_eq!(library.db.query_row("SELECT revision FROM version_catalog_revision", [], |r| r.get::<_, i64>(0)).unwrap(), revision, "enrichment does not invalidate edition candidates");
        drop(library);
        let library = Library::open_existing(&path).unwrap();
        let before = library.db.total_changes();
        let kept = library.metadata_cached("recording:1").unwrap().unwrap();
        assert_eq!(kept.contributions, [producer()]);
        assert!(kept.fresh(100 + FRESH_SECONDS - 1));
        assert!(kept.fresh(100 + FRESH_SECONDS * 100));
        assert!(kept.fresh(99), "clock changes do not invalidate positive metadata");
        assert_eq!(library.db.total_changes(), before);
        library.cache_metadata("empty", 100, &Value::Null, &[]).unwrap();
        let empty = library.metadata_cached("empty").unwrap().unwrap();
        assert!(empty.document.is_null());
        assert!(empty.fresh(100 + FRESH_SECONDS - 1));
        assert!(!empty.fresh(100 + FRESH_SECONDS));
    }

    #[test]
    fn oversized_answers_do_not_replace_old_credits_or_touch_tags() {
        let library = Library::in_memory().unwrap();
        library.cache_metadata("key", 100, &json!({}), &[producer()]).unwrap();
        assert!(library.cache_metadata("key", 200, &json!("x".repeat(ENTRY_BYTES)), &[]).is_err());
        assert_eq!(library.metadata_cached("key").unwrap().unwrap().contributions, [producer()]);
        library.cache_metadata("key", 200, &Value::Null, &[]).unwrap();
        assert_eq!(library.db.query_row("SELECT count(*) FROM contributors", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    }
    #[test]
    fn a_batch_rolls_back_together_and_cleans_only_replaced_contributors() {
        let library = Library::in_memory().unwrap();
        library.cache_metadata("release", 100, &json!({"old":true}), &[producer()]).unwrap();
        library.cache_metadata("recording", 100, &json!({"old":true}), &[producer()]).unwrap();
        library.db.execute("INSERT INTO contributors(id,name) VALUES ('unrelated-orphan','Unrelated')", []).unwrap();
        library.db.execute_batch("CREATE TEMP TRIGGER reject_recording BEFORE INSERT ON metadata_cache WHEN new.key = 'recording' BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
        let entries = vec![("release".into(), json!({"new":true}), Vec::new()), ("recording".into(), json!({"new":true}), Vec::new())];
        assert!(library.store_metadata_batch(200, &entries).is_err());
        assert_eq!(library.metadata_cached("release").unwrap().unwrap().document, json!({"old":true}));
        assert_eq!(library.metadata_cached("recording").unwrap().unwrap().contributions, [producer()]);
        library.db.execute_batch("DROP TRIGGER reject_recording;").unwrap();
        library.store_metadata_batch(200, &entries).unwrap();
        let ids: Vec<String> = library.db.prepare("SELECT id FROM contributors ORDER BY id").unwrap()
            .query_map([], |row| row.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
        assert_eq!(ids, ["unrelated-orphan"], "cleanup is scoped to contributors of replaced entries");
    }

    #[test]
    fn positive_entries_are_not_evicted_by_an_unrelated_write() {
        let library = Library::in_memory().unwrap();
        library.cache_metadata("retained", 1, &json!({"permanent":true}), &[producer()]).unwrap();
        library.db.execute("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<4100) INSERT INTO metadata_cache(key,fetched_at,bytes,document) SELECT 'entry:'||x,2,9000,'{}' FROM n", []).unwrap();
        library.cache_metadata("latest", 3, &json!({}), &[]).unwrap();
        assert!(library.metadata_cached("retained").unwrap().is_some());
        assert_eq!(library.db.query_row("SELECT count(*) FROM metadata_cache", [], |row| row.get::<_,i64>(0)).unwrap(), 4102);
    }

}
