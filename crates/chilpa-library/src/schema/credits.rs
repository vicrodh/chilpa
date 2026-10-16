//! V22 shipped first with a view and later with indexed membership tables.
//! Advance either layout through a new version; never rerun its column add.

use rusqlite::Connection;

pub(super) fn upgrade(db: &Connection) -> rusqlite::Result<()> {
    let kind: String = db.query_row("SELECT type FROM sqlite_master WHERE name = 'album_members'", [], |r| r.get(0))?;
    if kind == "view" {
        db.execute_batch("DROP VIEW album_members;")?;
        // Reuse the immutable table/trigger definitions, excluding the column
        // and credit backfill: the per-file credits already exist in V22.
        let (_, tables) = super::steps::MIGRATION_V22.split_once("CREATE TABLE album_members")
            .expect("V22 membership definitions");
        db.execute_batch(&format!("CREATE TABLE album_members{tables}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_shipped_v22_layouts_upgrade_without_overwriting_file_credits() {
        for early in [false, true] {
            let db = Connection::open_in_memory().unwrap();
            crate::schema::apply_pragmas(&db).unwrap();
            crate::schema::stop_at(&db, if early { 21 } else { 22 });
            if early {
                db.execute_batch("ALTER TABLE tracks ADD COLUMN album_credit TEXT NOT NULL DEFAULT '';
                    CREATE VIEW album_members AS SELECT album_id, id AS track_id, artist FROM tracks;
                    PRAGMA user_version = 22;").unwrap();
            }
            db.execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
                INSERT INTO albums(id, title, album_artist) VALUES (1, 'Compilation', 'Various Artists');
                INSERT INTO tracks(id, source_id, external_id, title, artist, album_credit, album_id, added_at)
                VALUES (1, 1, '/one.flac', 'One', 'Singer', 'Original Credit', 1, 1);").unwrap();
            crate::schema::migrate(&db).unwrap();
            crate::schema::migrate(&db).unwrap();
            assert_eq!(db.query_row("SELECT album_credit FROM tracks", [], |r| r.get::<_, String>(0)).unwrap(), "Original Credit");
            assert_eq!(db.query_row("SELECT count(*) FROM album_artist_rollup", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
            db.execute("UPDATE tracks SET present = 0", []).unwrap();
            assert_eq!(db.query_row("SELECT count(*) FROM album_artist_rollup", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
            db.execute("UPDATE tracks SET present = 1, artist = 'Another'", []).unwrap();
            assert_eq!(db.query_row("SELECT tracks FROM album_artist_rollup WHERE artist = 'Another'", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        }
    }
}
