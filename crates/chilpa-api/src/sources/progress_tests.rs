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
                Ok(Answer::Picture { play, url, song, path }) => api.take_picture(play, url, song, path),
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

/// A facade where `play` was launched for a station listed at `url`.
fn station(play: u64, url: &str) -> Chilpa {
    let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
    api.sources.looked_back = true;
    api.sources.sounding = Some(Sounding { url: url.into(), source: "example.radio".into(), entry: Entry { title: "The station".into(), live: true, ..Entry::default() }, picture: String::new() });
    api.launched(play, 0, crate::Relaunch::Track);
    api.now.path = url.into();
    api.now.playing = true;
    api
}

#[test]
fn a_station_picture_that_arrives_before_its_track_starts_is_on_the_face_it_starts_with() {
    let url = "https://radio.example/early";
    let mut api = station(31, url);
    api.take_picture(Some(31), url.into(), String::new(), "/covers/early.jpg".into());
    let face = api.face_of(31, std::path::Path::new(url), None, None);
    assert_eq!(face.cover, "/covers/early.jpg", "what the window is given as the track starts");
    assert_eq!(face.title, "The station");
}

#[test]
fn a_station_picture_that_arrives_after_its_track_started_is_shown_at_once() {
    let url = "https://radio.example/late";
    let mut api = station(32, url);
    api.take_picture(Some(32), url.into(), String::new(), "/covers/late.jpg".into());
    assert_eq!(api.now.artwork_path, "/covers/late.jpg");
}

#[test]
fn a_picture_for_a_replaced_play_changes_neither_the_window_nor_the_card_of_the_play_that_replaced_it() {
    let url = "https://radio.example/again";
    let mut api = station(33, url);
    // The same station is started again: another play, with its own card.
    api.launched(34, 0, crate::Relaunch::Track);
    api.take_picture(Some(33), url.into(), String::new(), "/covers/of-the-first.jpg".into());
    assert_eq!(api.now.artwork_path, "", "the window is the new play's");
    assert_eq!(api.face_of(34, std::path::Path::new(url), None, None).cover, "", "and so is its card");
    assert_eq!(api.face_of(33, std::path::Path::new(url), None, None).cover, "/covers/of-the-first.jpg", "the play it was asked for keeps it");
    // The new play's own picture is its own, and shown.
    api.take_picture(Some(34), url.into(), String::new(), "/covers/of-the-second.jpg".into());
    assert_eq!(api.now.artwork_path, "/covers/of-the-second.jpg");
}

#[test]
fn a_picture_asked_for_with_no_play_is_shown_as_before() {
    let url = "https://radio.example/cast";
    let mut api = station(35, url);
    api.take_picture(None, url.into(), String::new(), "/covers/no-play.jpg".into());
    assert_eq!(api.now.artwork_path, "/covers/no-play.jpg");
}
