//! The shell's query dispatch, without Qt or an audio device.
//! One test owns the process environment and the worker's singleton.

#[allow(dead_code)]
#[path = "../../chilpa-qt/src/app.rs"]
mod app;
#[path = "../../chilpa-qt/src/worker.rs"]
mod worker;

use chilpa_api::{Filter, TrackOrder};
use chilpa_library::Library;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

#[test]
fn a_brief_writer_cannot_leave_the_library_empty_for_the_session() {
    let dir = std::env::temp_dir().join(format!("chilpa-query-worker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.join(folder));
    }
    let path = Library::default_path();
    let lib = Library::open(&path).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.connection().execute_batch("INSERT INTO albums(id, title, album_artist, folder)
        VALUES (1, 'Carbon-Based Anatomy', 'Cynic', '/music/Cynic');").unwrap();
    lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, duration_ms, added_at)
        VALUES (1, ?1, '/music/Cynic/01.flac', 'Carbon-Based Anatomy', 'Cynic', 1, 1000, 1)", [source]).unwrap();
    lib.set_track_genres(1, &["Heavy Metal; Ambient; Electronic; Jazz; Rock; Folk; Prog Rock; World; & CountryStyle:Heavy Metal".into()]).unwrap();
    lib.assemble_local_releases().unwrap();
    lib.update_statistics().unwrap();
    // Home has already opened the facade's connection, as in the report.
    assert_eq!(app::with(|api| api.albums_sorted(chilpa_api::AlbumSort::Title, 10, 0)).len(), 1);

    // A deferred read-to-write upgrade fails immediately, even though this
    // writer lives for far less than the five-second busy timeout.
    let tx = lib.connection().unchecked_transaction().unwrap();
    tx.execute("UPDATE tracks SET title = 'Pending edit' WHERE id = 1", []).unwrap();
    let start = Instant::now();
    let error = Library::open(&path).err().expect("the maintenance open tries to upgrade its read transaction");
    assert_eq!(error.sqlite_error_code(), Some(rusqlite::ErrorCode::DatabaseBusy));
    eprintln!("maintenance open: {error} after {:?}", start.elapsed());
    let (sent, received) = channel();
    worker::submit(move |reader| { let _ = sent.send(reader.snapshot(&Filter::default(), TrackOrder::Album)); });
    let first = received.recv_timeout(Duration::from_millis(500)).unwrap_or_default();
    tx.rollback().unwrap();
    eprintln!("writer released after {:?}; first snapshot: {} tracks", start.elapsed(), first.totals.tracks);

    // Reopening the page must also work in this same process. On 32021db
    // both jobs are dropped and the view keeps its default empty snapshot.
    let (sent, received) = channel();
    worker::submit(move |reader| {
        let filter = Filter::default();
        let _ = sent.send((reader.snapshot(&filter, TrackOrder::Album), reader.facets(&filter)));
    });
    let (second, facets) = received.recv_timeout(Duration::from_secs(2)).unwrap_or_default();
    assert_eq!(second.totals.tracks, 1, "a later job must populate Library after the writer is gone");
    assert_eq!(first.totals.tracks, 1, "a reader must see committed tracks while a writer is active");
    assert_eq!(second.totals.albums, 1);
    assert_eq!(facets.sources.len(), 1);
    assert_eq!(facets.genres.len(), 9);
    assert_eq!(facets.albums.len(), 1);
    app::shutdown();
}
