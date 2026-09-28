use super::*;

fn fixture() -> Library {
    let lib = Library::in_memory().unwrap();
    lib.db
        .execute_batch(
            "INSERT INTO sources(id, provider, instance) VALUES
        (1, 'local', 'default'), (2, 'server', 'home'), (3, 'server', 'away');
        INSERT INTO albums(id, title, album_artist, version_key) VALUES
        (1, 'Set', 'Band', '1:first'), (2, 'Set', 'Band', '1:second');
        INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at,
            album_title, album_credit, disc_number, disc_total) VALUES
        (1, 1, '/set/CD1/a.flac', 'One', 1, 0, 'Set', 'Band', 1, 2),
        (2, 1, '/set/CD2/b.flac', 'Two', 2, 0, 'Set', 'Band', 2, 2),
        (3, 2, 'same-id', 'Remote', NULL, 0, '', '', NULL, NULL),
        (4, 3, 'same-id', 'Remote', NULL, 0, '', '', NULL, NULL);",
        )
        .unwrap();
    lib
}

#[test]
fn ratings_are_personal_source_scoped_and_not_favorites() {
    let lib = fixture();
    for (target, stars) in [
        (RatingTarget::Track(1), 5),
        (RatingTarget::Track(3), 2),
        (RatingTarget::Track(4), 4),
        (RatingTarget::Album(1), 3),
        (RatingTarget::Album(2), 1),
    ] {
        lib.rate(target, stars).unwrap();
        assert_eq!(lib.ratings().unwrap().get(target), stars);
    }
    assert!(lib.favorite_albums().unwrap().is_empty());
    assert!(lib.rate(RatingTarget::Track(99), 1).is_err());
    assert!(lib.rate(RatingTarget::Track(1), 6).is_err());
    lib.rate(RatingTarget::Track(1), 0).unwrap();
    assert_eq!(lib.ratings().unwrap().get(RatingTarget::Track(1)), 0);
    lib.db
        .execute(
            "UPDATE tracks SET title = 'Retagged', present = 0 WHERE id = 3",
            [],
        )
        .unwrap();
    assert_eq!(lib.ratings().unwrap().get(RatingTarget::Track(3)), 2);
}

#[test]
fn assembly_keeps_track_ratings_and_the_last_album_choice_including_clear() {
    let lib = fixture();
    lib.rate(RatingTarget::Track(2), 4).unwrap();
    lib.rate(RatingTarget::Album(1), 5).unwrap();
    lib.rate(RatingTarget::Album(2), 0).unwrap();
    lib.assemble_local_releases().unwrap();
    let before = lib.ratings().unwrap();
    assert_eq!(before.get(RatingTarget::Track(2)), 4);
    assert_eq!(before.get(RatingTarget::Album(1)), 0);
    assert_eq!(lib.album_tracks(1).unwrap().len(), 2);
    lib.assemble_local_releases().unwrap();
    assert_eq!(before, lib.ratings().unwrap());
    assert!(lib.check().unwrap().is_empty());
}

#[test]
fn migration_from_v24_preserves_history_and_starts_unrated() {
    let db = Connection::open_in_memory().unwrap();
    crate::schema::apply_pragmas(&db).unwrap();
    crate::schema::stop_at(&db, 24);
    db.execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
        INSERT INTO tracks(id, source_id, external_id, title, added_at) VALUES (1, 1, 'one', 'One', 0);
        INSERT INTO play_events(track_id, source_id, started_at, played_ms) VALUES (1, 1, 1, 100);").unwrap();
    crate::schema::migrate(&db).unwrap();
    crate::schema::migrate(&db).unwrap();
    let lib = Library {
        db,
        versions: Default::default(),
    };
    assert_eq!(lib.stats().unwrap().listened_ms, 100);
    assert_eq!(lib.ratings().unwrap(), Ratings::default());
    lib.rate(RatingTarget::Track(1), 5).unwrap();
    assert_eq!(lib.ratings().unwrap().get(RatingTarget::Track(1)), 5);
}
