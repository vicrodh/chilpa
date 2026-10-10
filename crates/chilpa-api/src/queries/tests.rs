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
