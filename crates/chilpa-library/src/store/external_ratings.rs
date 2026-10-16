//! I keep personal stars independently of remote cache ids and account availability.
use super::Library;
use rusqlite::params;
use std::collections::BTreeMap;

pub type ExternalRatings = BTreeMap<(String, String, String), u8>;

impl Library {
    pub fn external_ratings(&self) -> rusqlite::Result<ExternalRatings> {
        self.db.prepare("SELECT source, kind, external_id, stars FROM external_ratings WHERE stars > 0")?
            .query_map([], |row| Ok(((row.get(0)?, row.get(1)?, row.get(2)?), row.get(3)?)))?.collect()
    }

    pub fn rate_external(&self, source: &str, kind: &str, external_id: &str, stars: u8) -> rusqlite::Result<()> {
        if source.is_empty() || external_id.is_empty() || stars > 5 || !["track", "album", "playlist"].contains(&kind) {
            return Err(rusqlite::Error::InvalidParameterName("invalid external rating target".into()));
        }
        let tx = self.own_transaction()?;
        tx.execute("UPDATE rating_clock SET revision = revision + 1 WHERE id = 1", [])?;
        tx.execute("INSERT INTO external_ratings(source, kind, external_id, stars, revision, updated_at)
            VALUES (?1, ?2, ?3, ?4, (SELECT revision FROM rating_clock WHERE id = 1), unixepoch())
            ON CONFLICT(source, kind, external_id) DO UPDATE SET stars = excluded.stars,
                revision = excluded.revision, updated_at = excluded.updated_at",
            params![source, kind, external_id, stars])?;
        tx.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_and_kind_disambiguate_ids_and_clear_remains_explicit() {
        let lib = Library::in_memory().unwrap();
        lib.rate_external("one", "track", "7", 5).unwrap();
        lib.rate_external("two", "track", "7", 2).unwrap();
        lib.rate_external("one", "album", "7", 3).unwrap();
        let values = lib.external_ratings().unwrap();
        assert_eq!(values.len(), 3);
        assert_eq!(values[&("one".into(), "track".into(), "7".into())], 5);
        lib.rate_external("one", "track", "7", 0).unwrap();
        assert_eq!(lib.external_ratings().unwrap().len(), 2);
        assert_eq!(lib.connection().query_row("SELECT stars FROM external_ratings WHERE source='one' AND kind='track'", [], |row| row.get::<_, u8>(0)).unwrap(), 0);
        assert!(lib.rate_external("one", "track", "7", 6).is_err());
    }
}
