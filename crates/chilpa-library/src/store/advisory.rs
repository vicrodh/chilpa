//! Which tracks and records their files call explicit, for a search that
//! hides them.
use super::Library;
use std::collections::HashSet;

impl Library {
    /// Of these tracks, the ones whose file says explicit.
    pub fn explicit_tracks(&self, ids: &[i64]) -> rusqlite::Result<HashSet<i64>> {
        self.ids_where(ids, "SELECT id FROM tracks WHERE explicit = 1 AND id IN")
    }

    /// Of these records, the ones with an explicit track that is there.
    pub fn explicit_albums(&self, ids: &[i64]) -> rusqlite::Result<HashSet<i64>> {
        self.ids_where(ids, "SELECT DISTINCT album_id FROM tracks WHERE explicit = 1 AND present = 1 AND album_id IN")
    }

    fn ids_where(&self, ids: &[i64], query: &str) -> rusqlite::Result<HashSet<i64>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let marks = vec!["?"; ids.len()].join(",");
        self.db.prepare(&format!("{query} ({marks})"))?.query_map(rusqlite::params_from_iter(ids.iter()), |r| r.get(0))?.collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::store::fixtures;

    #[test]
    fn explicit_tracks_and_the_records_they_make_explicit() {
        let lib = fixtures::sample_library();
        lib.db.execute("UPDATE tracks SET explicit = 1 WHERE album_id = 2", []).unwrap();
        lib.db.execute("UPDATE tracks SET explicit = 0 WHERE album_id = 3", []).unwrap();
        let ids: Vec<i64> = lib.db.prepare("SELECT id FROM tracks ORDER BY album_id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        let explicit = lib.explicit_tracks(&ids).unwrap();
        assert_eq!(explicit.len(), 1);
        assert_eq!(lib.explicit_albums(&[1, 2, 3]).unwrap(), [2].into_iter().collect());
        assert!(lib.explicit_albums(&[]).unwrap().is_empty());
    }

    #[test]
    fn advisory_survives_pages_rows_and_cached_editions_without_guessing_clean() {
        use crate::{AlbumSort, Filter, TrackOrder};
        let lib = fixtures::sample_library();
        let before = lib.version_catalog().unwrap();
        assert_eq!(before.versions[&1].explicit, None);
        lib.db.execute("UPDATE tracks SET explicit = 1 WHERE album_id = 1", []).unwrap();
        lib.db.execute("UPDATE tracks SET explicit = 0 WHERE album_id = 2", []).unwrap();
        let after = lib.version_catalog().unwrap();
        assert!(!std::sync::Arc::ptr_eq(&before, &after));
        assert_eq!((after.versions[&1].explicit, after.versions[&2].explicit, after.versions[&3].explicit), (Some(true), Some(false), None));
        let albums = lib.albums_by_ids(&[1, 2, 3]).unwrap();
        assert_eq!(albums.iter().map(|(a, _)| a.explicit).collect::<Vec<_>>(), [Some(true), Some(false), None]);
        assert_eq!(lib.album(1).unwrap().unwrap().0.explicit, Some(true));
        assert_eq!(lib.albums(10, 0).unwrap().iter().find(|a| a.id == 2).unwrap().explicit, Some(false));
        assert_eq!(lib.albums_sorted(AlbumSort::Title, 10, 0).unwrap().iter().find(|(a, _)| a.id == 3).unwrap().0.explicit, None);
        assert_eq!(lib.most_played_albums(10).unwrap().iter().find(|(a, _)| a.id == 1).unwrap().0.explicit, Some(true));
        let rows = lib.browse_tracks(&Filter::default(), TrackOrder::Title, 10).unwrap();
        assert_eq!(rows.iter().find(|t| t.album_id == Some(1)).unwrap().explicit, Some(true));
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO tracks(source_id,external_id,title,album_id,duration_ms,added_at) VALUES (?1,'/unknown.flac','Unknown',2,1000,1)", [source]).unwrap();
        assert_eq!(lib.album(2).unwrap().unwrap().0.explicit, None, "one unknown track cannot certify a clean album");
        lib.db.execute("UPDATE tracks SET explicit = 1 WHERE external_id = '/unknown.flac'", []).unwrap();
        assert_eq!(lib.album(2).unwrap().unwrap().0.explicit, Some(true));
        lib.db.execute("UPDATE tracks SET present = 0 WHERE external_id = '/unknown.flac'", []).unwrap();
        assert_eq!(lib.album(2).unwrap().unwrap().0.explicit, Some(false), "absent tracks do not set a visible advisory");
    }
}
