use super::*;

#[test]
fn a_split_three_disc_release_migrates_without_a_rescan() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    crate::schema::apply_pragmas(&db).unwrap();
    crate::schema::stop_at(&db, 22);
    db.execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default')").unwrap();
    for (disc, name, count) in [(1, "CD 1 Mono", 11), (2, "CD 2 Stereo", 11), (3, "CD 3 Bonus Disc", 9)] {
        db.execute("INSERT INTO albums(id, title, album_artist, folder, version_key)
            VALUES (?1, 'The Piper At The Gates Of Dawn (40th Anniversary Edition)', 'Pink Floyd', ?2, ?2)", params![disc, name]).unwrap();
        for track in 1..=count {
            db.execute("INSERT INTO tracks(source_id, external_id, title, artist, album_credit, album_id, disc_number, track_number, added_at)
                VALUES (1, ?1, 'Track', 'Pink Floyd', 'Pink Floyd', ?2, ?2, ?3, 1)", params![format!("/Piper/{name}/{track}.flac"), disc, track]).unwrap();
        }
    }
    crate::schema::migrate(&db).unwrap();
    let lib = Library { db, versions: Default::default() };
    lib.assemble_local_releases().unwrap();
    assert_eq!(lib.album_ids(super::super::AlbumSort::Title).unwrap(), [1]);
    assert_eq!(lib.album_tracks(1).unwrap().len(), 31);
    assert_eq!(lib.version_catalog().unwrap().choices(1)[0].tracks, 31);
    assert!(lib.check().unwrap().is_empty());
    let changes = lib.db.total_changes();
    lib.assemble_local_releases().unwrap();
    assert_eq!(changes, lib.db.total_changes());
}
