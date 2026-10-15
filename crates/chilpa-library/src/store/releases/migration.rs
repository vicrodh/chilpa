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
    let lib = Library { db, versions: Default::default(), related: Default::default(), extra_candidates: Default::default(), candidate_changes: Default::default() };
    lib.assemble_local_releases().unwrap();
    assert_eq!(lib.album_ids(super::super::AlbumSort::Title).unwrap(), [1]);
    assert_eq!(lib.album_tracks(1).unwrap().len(), 31);
    assert_eq!(lib.version_catalog().unwrap().choices(1)[0].tracks, 31);
    assert!(lib.check().unwrap().is_empty());
    let changes = lib.db.total_changes();
    lib.assemble_local_releases().unwrap();
    assert_eq!(changes, lib.db.total_changes());
}

#[test]
fn scene_folders_with_different_album_tags_form_one_box() {
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    for (disc, title, artist) in [(1, "Complete Box", "Various Artists"),
        (2, "Character Image Song I", "Singer A"), (3, "Character Image Song II", "Singer B")] {
        lib.db.execute("INSERT INTO albums(id, title, album_artist) VALUES (?1, ?2, ?3)",
            params![disc, title, artist]).unwrap();
        lib.db.execute("INSERT INTO tracks(source_id, external_id, title, artist, album_credit, album_id,
            disc_number, track_number, added_at) VALUES (?1, ?2, 'Song', ?3, ?4, ?5, ?6, 1, 1)",
            params![source, format!("/music/Complete CD-BOX/Scene {disc:02} {title}/01.flac"), artist,
                artist, disc, if disc == 1 { Some(1) } else { None }]).unwrap();
    }
    lib.assemble_local_releases().unwrap();
    assert_eq!(lib.album_ids(super::super::AlbumSort::Title).unwrap(), [1]);
    assert_eq!(lib.album_tracks(1).unwrap().len(), 3);
    let (title, artist): (String, String) = lib.db.query_row("SELECT title, album_artist FROM albums WHERE id = 1", [],
        |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((title.as_str(), artist.as_str()), ("Complete CD-BOX", "Various Artists"));
    let discs: Vec<Option<u16>> = lib.edit_tracks(1).unwrap().into_iter().map(|track| track.disc_number).collect();
    assert_eq!(discs, [Some(1), Some(2), Some(3)]);
    let changes = lib.db.total_changes();
    lib.assemble_local_releases().unwrap();
    assert_eq!(changes, lib.db.total_changes());
}
