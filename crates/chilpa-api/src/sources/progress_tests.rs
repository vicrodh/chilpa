use super::*;

#[test]
fn playback_poll_does_not_wait_for_a_database_writer_without_plugins() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let library = chilpa_library::Library::open(&path).unwrap();
    library.remember_heard("example.source", &Heard { external_id: "episode:1".into(), ..Default::default() }).unwrap();
    let blocker = chilpa_library::Library::open(&path).unwrap();
    blocker.connection().execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut api = Chilpa::new();
    api.library = Some(library);
    api.listen.active = 0;
    api.now.path = entry_of("example.source", "episode:1");
    api.now.title = "Episode".into();
    api.now.artwork_path = "known-cover".into();
    api.now.playing = true;
    api.now.seconds = 42.0;
    api.now.total_seconds = 600.0;
    assert!(api.plugins.is_none());
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(900));
        blocker.connection().execute_batch("ROLLBACK").unwrap();
    });
    let started = Instant::now();
    api.poll();
    let elapsed = started.elapsed();
    release.join().unwrap();
    assert!(elapsed < Duration::from_millis(250), "poll waited for a database writer: {elapsed:?}");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if api.library.as_ref().unwrap().heard("example.source", "episode:1").unwrap().unwrap().position_ms == 42_000 { break; }
        assert!(Instant::now() < deadline, "queued progress was not saved after the writer released the database");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn spoken_media_metadata_keeps_the_medium_without_becoming_a_music_catalog() {
    let mut api = Chilpa::new();
    let document: SourceDocument = serde_json::from_value(serde_json::json!({
        "medium":"podcasts", "attribution":"An open directory", "entries":[
            {"id":"episode:1", "title":"Episode", "kind":"item"}
        ], "presentation":{"episode:1":{"metadata":{"season":"2"}}},
        "facets":[{"id":"language", "title":"Language", "entries":[{"id":"en", "title":"English"}]}]
    })).unwrap();
    api.take_listing(None, Ok(document));
    assert_eq!(api.sources.medium, "podcasts");
    assert!(!api.sources.music);
    assert_eq!(api.sources.presentation["episode:1"].metadata["season"], "2");
    assert_eq!(api.sources.columns[0].id, "language");
    assert_eq!(api.sources.sections_pending, 0);
}

#[test]
fn a_late_chapter_reply_does_not_replace_the_selected_transcript() {
    let mut api = Chilpa::new();
    api.sources.extras = serde_json::json!({"id":"episode:1","kind":"transcript","loading":true});
    api.sources.tx.send(Answer::Extra { generation: api.sources.generation, id: "episode:1".into(),
        kind: "chapters".into(), result: Ok(serde_json::json!([])) }).unwrap();
    api.advance_sources();
    assert_eq!(api.sources.extras["kind"], "transcript");
    assert_eq!(api.sources.extras["loading"], true);
    api.sources.tx.send(Answer::Extra { generation: api.sources.generation, id: "episode:1".into(),
        kind: "transcript".into(), result: Ok(serde_json::json!([{"start":0,"text":"Hello"}])) }).unwrap();
    api.advance_sources();
    assert_eq!(api.sources.extras["rows"][0]["text"], "Hello");
}

#[test]
fn a_station_flooding_its_title_goes_to_the_network_once_and_the_cover_returns_after_a_reconnect() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    static COVER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    fn find(_: &song_cover::Song) -> song_cover::Found {
        ASKED.fetch_add(1, Ordering::SeqCst);
        song_cover::Found::Cover(COVER.get().cloned().unwrap_or_default())
    }
    let directory = tempfile::tempdir().unwrap();
    let cover = directory.path().join("one.jpg");
    std::fs::write(&cover, b"cover").unwrap();
    let cover = cover.to_string_lossy().into_owned();
    COVER.set(cover.clone()).unwrap();
    let mut api = Chilpa::new();
    let url = "https://radio.example/stream".to_string();
    api.sources.song_cover_find = find;
    api.sources.sounding = Some(Sounding { url: url.clone(), source: "example.radio".into(), entry: Entry::default(), picture: "/covers/station.jpg".into() });
    api.now.path = url;
    api.now.title = "A - One".into();
    let take_until_cover = |api: &mut Chilpa| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while api.now.artwork_path != *COVER.get().unwrap() {
            assert!(Instant::now() < deadline, "the cover did not come");
            match api.sources.rx.try_recv() {
                Ok(Answer::Picture { url, song, path }) => api.take_picture(url, song, path),
                _ => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    };
    api.song_changed("A - One");
    take_until_cover(&mut api);
    api.song_changed("A - One");
    assert_eq!(api.now.artwork_path, "/covers/station.jpg", "a reconnect shows the station until the cover is back");
    take_until_cover(&mut api);
    for _ in 0..1_000 {
        api.song_changed("A - One");
    }
    take_until_cover(&mut api);
    assert_eq!(api.now.artwork_path, cover);
    assert_eq!(ASKED.load(Ordering::SeqCst), 1);
}
