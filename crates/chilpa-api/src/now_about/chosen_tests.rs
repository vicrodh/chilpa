//! The information of a track chosen from a list while another plays: it
//! speaks of the chosen one, and asking for it plays nothing.

use super::*;
use std::time::{Duration, Instant};

/// MusicBrainz is not asked while a test runs; the policy comes back after.
/// `Chilpa::new` applies the saved policy, so this goes after it.
struct NoMusicBrainz(chilpa_core::network::Policy);

impl NoMusicBrainz {
    fn new() -> Self {
        let before = chilpa_core::network::current();
        chilpa_core::network::configure(chilpa_core::network::Policy { musicbrainz: false, offline: true, ..before });
        Self(before)
    }
}

impl Drop for NoMusicBrainz {
    fn drop(&mut self) { chilpa_core::network::configure(self.0); }
}

/// The answer once the background has read it.
fn settled(api: &mut Chilpa, ask: impl Fn(&mut Chilpa) -> String) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found: Value = serde_json::from_str(&ask(api)).unwrap();
        if found["loading"] != true { return found; }
        assert!(Instant::now() < deadline, "the answer never came: {found}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_library_track_that_does_not_play_is_described_by_its_own_file_and_record() {
    let music = tempfile::tempdir().unwrap();
    let file = music.path().join("chosen.flac");
    std::fs::write(&file, b"not audio").unwrap();
    let path = file.to_string_lossy().into_owned();
    let library = chilpa_library::Library::in_memory().unwrap();
    let source = library.source_id("local", "default").unwrap();
    let album = library.source_edition(source, "record", "Chosen Record", "Band", Some(2001)).unwrap();
    library.connection().execute("INSERT INTO tracks(source_id,external_id,title,artist,album_id,added_at) VALUES (?1,?2,'Chosen','Band',?3,0)",
        rusqlite::params![source, path, album]).unwrap();
    let mut api = Chilpa::new();
    api.library = Some(library);
    let _policy = NoMusicBrainz::new();
    api.now.path = "/music/Elsewhere/playing.flac".into();
    api.now.title = "Playing".into();

    let subject = api.subject_of(&path);
    assert_eq!((subject.title.as_str(), subject.album_id), ("Chosen", album), "the chosen row, not what plays");
    let found = settled(&mut api, |api| api.track_about(&path));
    assert_eq!(found["file"]["value"], path.as_str());
    assert_eq!(found["file"]["note"], "", "its file is there");
    let record = settled(&mut api, |api| api.album_about(&path));
    let facts: Vec<(String, String)> = record["facts"].as_array().unwrap().iter()
        .map(|fact| (fact["label"].as_str().unwrap().to_string(), fact["value"].as_str().unwrap().to_string())).collect();
    assert!(facts.contains(&(i18n::t("Release date"), "2001".into())), "the chosen track's record: {facts:?}");

    assert_eq!(api.now_playing().path, "/music/Elsewhere/playing.flac", "what plays stays");
    assert!(api.queue_paths().is_empty(), "nothing was queued");
}

#[test]
fn a_track_whose_file_is_gone_says_it_is_not_on_disk() {
    let mut api = Chilpa::new();
    let _policy = NoMusicBrainz::new();
    let path = "/nonexistent/chilpa-chosen/gone.flac";
    let found = settled(&mut api, |api| api.track_about(path));
    assert_eq!(found["file"]["value"], path);
    assert_eq!(found["file"]["note"], i18n::t("Not found on disk"));
}

#[test]
fn a_server_entry_names_its_provider_instance_and_id() {
    let mut api = Chilpa::new();
    let _policy = NoMusicBrainz::new();
    let entry = chilpa_models::RemoteEntry::new(chilpa_models::source::SourceId::new("provider", "home"), "chosen 7").to_entry();
    let found = settled(&mut api, |api| api.track_about(&entry));
    assert_eq!(found["file"]["value"], "provider/home/chosen 7");
    assert_eq!(found["file"]["title"], i18n::t("Server item"));
}

#[test]
fn the_track_that_plays_gets_the_answer_it_always_had() {
    let music = tempfile::tempdir().unwrap();
    let file = music.path().join("playing.flac");
    std::fs::write(&file, b"not audio").unwrap();
    let path = file.to_string_lossy().into_owned();
    let mut api = Chilpa::new();
    let _policy = NoMusicBrainz::new();
    api.now.path = path.clone();
    api.now.title = "Playing".into();

    let now = settled(&mut api, |api| api.now_track_about());
    assert_eq!(now["file"]["value"], path.as_str());
    assert_eq!(api.track_about(&path), api.now_track_about(), "chosen while it plays, it is the same answer");
    assert_eq!(api.album_about(&path), api.now_album_about());
    assert_eq!(api.retry_about("track", &path), api.retry_now_about("track", &path));
    assert_eq!((api.track_about(""), api.album_about("")), ("{}".to_string(), "{}".to_string()));
    api.now.path.clear();
    assert_eq!(api.now_track_about(), "{}", "nothing plays, nothing is said");
}
