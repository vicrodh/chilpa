use super::*;
use crate::{Filter, TrackOrder};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

#[test]
fn a_failed_reader_open_is_retried_on_the_next_job() {
    let dir = std::env::temp_dir().join(format!("chilpa-reader-retry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A directory cannot be opened as a database, on any platform.
    let path = dir.join("library.db");
    std::fs::create_dir(&path).unwrap();
    let worker = QueryWorker::start(path.clone(), crate::source_catalog::Catalog::new()).unwrap();
    let (sent, received) = channel::<()>();
    assert!(worker.submit(move |_| { let _ = sent.send(()); }));
    assert_eq!(received.recv_timeout(Duration::from_secs(2)), Err(RecvTimeoutError::Disconnected),
        "the failed job is dropped after reporting the open error");

    std::fs::remove_dir(&path).unwrap();
    let lib = chilpa_library::Library::open(&path).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.connection().execute("INSERT INTO albums(id, title, album_artist) VALUES (1, 'Record', 'Artist')", []).unwrap();
    lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, added_at)
        VALUES (?1, '/music/song.flac', 'Song', 1, 1)", [source]).unwrap();
    let (sent, received) = channel();
    assert!(worker.submit(move |reader| { let _ = sent.send((reader.snapshot(&Filter::default(), TrackOrder::Album), reader.catalog.is_some())); }));
    let (snapshot, has_catalog) = received.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(has_catalog, "the query worker must read native library tracks from the shared catalog");
    assert_eq!(snapshot.totals.tracks, 1,
        "the same worker must reopen the reader and deliver the next snapshot");
}

#[test]
fn album_navigation_does_not_wait_for_an_obsolete_search() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let library = chilpa_library::Library::open(&path).unwrap();
    let source = library.source_id("local", "default").unwrap();
    library.connection().execute("INSERT INTO albums(id, title, album_artist) VALUES (1, 'Record', 'Artist')", []).unwrap();
    library.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, added_at)
        VALUES (?1, '/music/song.flac', 'Song', 1, 1)", [source]).unwrap();
    let search = QueryWorker::start(path.clone(), crate::source_catalog::Catalog::new()).unwrap();
    let album = QueryWorker::start(path, crate::source_catalog::Catalog::new()).unwrap();
    let (entered, running) = channel();
    let (release, waiting) = channel();
    assert!(search.submit(move |_| { entered.send(()).unwrap(); let _ = waiting.recv(); }));
    running.recv_timeout(Duration::from_secs(2)).unwrap();
    let (finished, page) = channel();
    assert!(album.submit(move |reader| { let _ = finished.send(reader.album_page(1, false)); }));
    let result = page.recv_timeout(Duration::from_secs(2));
    release.send(()).unwrap();
    assert_eq!(result.unwrap().unwrap().detail.album.id, 1,
        "the album reader must finish while the old search is still blocked");
}

#[test]
fn version_commands_keep_fifo_order_after_their_page_is_gone() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let library = chilpa_library::Library::open(&path).unwrap();
    let source = library.source_id("local", "default").unwrap();
    let first_edition = library.source_edition(source, "edition:first", "Record", "Artist", None).unwrap();
    let second_edition = library.source_edition(source, "edition:second", "Record", "Artist", None).unwrap();
    assert_ne!(first_edition, second_edition);
    for id in [first_edition, second_edition] {
        library.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, added_at)
            VALUES (?1, ?2, 'Song', ?3, 1)", rusqlite::params![source, format!("/music/{id}.flac"), id]).unwrap();
    }
    let worker = QueryWorker::start(path, crate::source_catalog::Catalog::new()).unwrap();
    let (entered, running) = channel();
    let (release, waiting) = channel();
    assert!(worker.submit(move |_| { entered.send(()).unwrap(); let _ = waiting.recv(); }));
    running.recv_timeout(Duration::from_secs(2)).unwrap();
    let (finished, results) = channel();
    let first = finished.clone();
    assert!(worker.submit(move |reader| { let _ = first.send(reader.select_album_version(first_edition, second_edition)); }));
    assert!(worker.submit(move |reader| { let _ = finished.send(reader.select_album_version(first_edition, first_edition)); }));
    release.send(()).unwrap();
    assert!(results.recv_timeout(Duration::from_secs(2)).unwrap());
    assert!(results.recv_timeout(Duration::from_secs(2)).unwrap());
    assert_eq!(library.selected_album(first_edition), first_edition, "the last explicit choice wins after both queued commands persist");
}
