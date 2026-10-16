//! I isolate queue preparation from tests that switch a live player's profile.
use chilpa_api::{Chilpa, TrackEntry};

fn keys(api: &mut Chilpa, id: u64) -> Vec<String> {
    api.listen_rows(id, "", true).0.into_iter().filter_map(|row| match row {
        TrackEntry::Track(track) => Some(track.path), _ => None,
    }).collect()
}

#[test]
fn a_long_album_reports_work_until_it_reaches_the_requested_queue() {
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let lib = chilpa_library::Library::open(&chilpa_library::Library::default_path()).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    let album = lib.source_edition(source, "box", "Box", "Band", None).unwrap();
    lib.connection().execute_batch("BEGIN IMMEDIATE").unwrap();
    for number in 1..=1205 {
        lib.connection().execute("INSERT INTO tracks(source_id,external_id,title,album_id,disc_number,track_number,added_at)
            VALUES (?1,?2,'Song',?3,1,?4,1)", rusqlite::params![source,format!("/{number}.flac"),album,number]).unwrap();
    }
    lib.connection().execute_batch("COMMIT").unwrap();
    let mut api = Chilpa::new();
    let request = api.prepare_album_queue(vec![album], "buffer:new".into(), false, String::new()).unwrap();
    assert!(api.queue_busy());
    assert!(api.queue_paths().is_empty(), "path resolution does not start playback or replace a queue");
    let paths = api.reader().unwrap().album_paths(&request.ids);
    api.finish_album_queue(request, &paths);
    assert!(!api.queue_busy());
    let selected = api.listen_selected();
    assert_eq!(keys(&mut api, selected).len(), 1205);
    assert!(api.queue_paths().is_empty(), "a prepared extra queue does not open an output");
    let cancelled = api.prepare_album_queue(vec![album], "queue".into(), false, String::new()).unwrap();
    assert!(api.queue_busy());
    drop(cancelled);
    assert!(!api.queue_busy(), "cancellation releases the indicator");
}
