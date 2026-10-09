//! I exercise stars, output handoffs and shutdown against an isolated library.
use chilpa_api::listening::{ListenDimension, ListenFilter, ListenOrder};
use chilpa_api::{Chilpa, RatingTarget};
use chilpa_library::Library;
use std::io::Write;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn wave(path: &std::path::Path) {
    let data: u32 = 44100 * 4 * 60;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    for bytes in [
        16u32.to_le_bytes().as_slice(),
        1u16.to_le_bytes().as_slice(),
        2u16.to_le_bytes().as_slice(),
        44100u32.to_le_bytes().as_slice(),
        176400u32.to_le_bytes().as_slice(),
        4u16.to_le_bytes().as_slice(),
        16u16.to_le_bytes().as_slice(),
    ] {
        f.write_all(bytes).unwrap();
    }
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    f.write_all(&vec![0; data as usize]).unwrap();
}

fn wait_until(api: &mut Chilpa, predicate: impl Fn(&chilpa_api::NowPlaying) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let now = api.poll();
        assert!(now.error.is_empty(), "{}", now.error);
        if predicate(now) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("playback did not reach the expected state");
}

#[test]
fn ratings_survive_reopen_and_handoff_and_shutdown_keep_delivered_time() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("chilpa-listening-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
    let path = dir.join("one.wav");
    wave(&path);
    let lib = Library::open(&Library::default_path()).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.connection()
        .execute_batch(
            "INSERT INTO albums(id, title, album_artist) VALUES (1, 'Album', 'Band');
        INSERT INTO genres VALUES (1, 'Rock'), (2, 'Blues');",
        )
        .unwrap();
    lib.connection()
        .execute(
            "INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, added_at)
        VALUES (1, ?1, ?2, 'One', 'Band', 1, 0)",
            rusqlite::params![source, path.to_string_lossy()],
        )
        .unwrap();
    lib.connection()
        .execute_batch("INSERT INTO track_genres VALUES (1, 1), (1, 2);")
        .unwrap();
    let mut api = Chilpa::new();
    assert!(api.rate(RatingTarget::Track(1), 4));
    assert!(api.rate(RatingTarget::Album(1), 5));
    assert!(!api.rate(RatingTarget::Track(1), -1));
    assert_eq!(api.ratings().unwrap().get(RatingTarget::Track(1)), 4);
    api.play_file(&path, "file:capture", true);
    wait_until(&mut api, |now| now.seconds >= 1.0);
    api.set_paused(true);
    // I capture metadata at start, not whatever a later edit leaves behind.
    lib.connection()
        .execute_batch(
            "DELETE FROM track_genres; UPDATE tracks SET artist = 'Changed' WHERE id = 1;",
        )
        .unwrap();
    api.move_output("file:capture", true);
    // The second probe/start is asynchronous; allow it to settle while paused.
    for _ in 0..100 {
        api.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    api.set_paused(false);
    wait_until(&mut api, |now| now.seconds >= 3.0);
    api.set_paused(true);
    api.poll();
    let delivered = api.now_playing().seconds * 1000.0;
    // Drop must drain the final event. I deliberately do not call stop/poll here.
    drop(api);
    let mut reopened = Chilpa::new();
    assert_eq!(reopened.ratings().unwrap().get(RatingTarget::Track(1)), 4);
    assert_eq!(reopened.ratings().unwrap().get(RatingTarget::Album(1)), 5);
    let filter = ListenFilter::default();
    let stats = reopened.listening_summary(&filter).unwrap();
    assert_eq!(stats.events, 1, "an output move is still one listen");
    assert!(
        stats.listened_ms >= delivered as u64 - 250,
        "{stats:?}, delivered={delivered}"
    );
    assert_eq!(stats.unknown_local_time, 0);
    assert_eq!(stats.backfilled, 0);
    assert_eq!(
        reopened
            .listening_buckets(&filter, ListenDimension::Genre, ListenOrder::Plays, 10)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        reopened
            .listening_buckets(&filter, ListenDimension::Artist, ListenOrder::Plays, 10)
            .unwrap()[0]
            .label,
        "Band"
    );
    // The stats screen's documents and the history's CSV, from that listen.
    let today: serde_json::Value = serde_json::from_str(&reopened.stats_report_json("day", "", 1)).unwrap();
    assert_eq!(today["kind"], "day");
    assert_eq!(today["comparison"], "Today vs Yesterday");
    assert_eq!(today["series"].as_array().unwrap().len(), 24, "a day goes by the hour");
    let found: serde_json::Value = serde_json::from_str(&reopened.listening_history_json(1, 20, "one band")).unwrap();
    assert_eq!(found["total"], 1);
    let none: serde_json::Value = serde_json::from_str(&reopened.listening_history_json(1, 20, "nothing")).unwrap();
    assert_eq!(none["total"], 0);
    let sheet = dir.join("history.csv");
    assert_eq!(reopened.export_listening_history(&sheet).as_deref(), Some("1 listen saved"));
    let text = std::fs::read_to_string(&sheet).unwrap();
    let lines: Vec<&str> = text.split("\r\n").filter(|l| !l.is_empty()).collect();
    assert_eq!(lines[0], "Date,Time,Track,Artist,Album,Heard,Length,Skipped,Quality,Bit-perfect");
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines[1].contains(",One,Band,Album,"), "{}", lines[1]);
    assert!(reopened.rate(RatingTarget::Track(1), 0));
    assert!(lib.check().unwrap().is_empty());
    drop(reopened);
    drop(lib);
    let mut again = Chilpa::new();
    assert_eq!(again.ratings().unwrap().get(RatingTarget::Track(1)), 0);
    drop(again);
    std::fs::remove_dir_all(dir).unwrap();
}
