use super::*;
use chilpa_meta::{Http, Reply};
use std::{collections::{BTreeMap, VecDeque}, sync::Mutex, time::Duration};

struct Answers { replies: Mutex<VecDeque<(u16, Value)>>, urls: Mutex<Vec<String>> }
impl Answers {
    fn new(replies: Vec<(u16, Value)>) -> Self { Self { replies: Mutex::new(replies.into()), urls: Mutex::new(Vec::new()) } }
    fn count(&self) -> usize { self.urls.lock().unwrap().len() }
}
impl Http for Answers {
    fn get(&self, url: &str, _: &str, _: Duration, _: usize) -> Result<Reply, String> {
        self.urls.lock().unwrap().push(url.into());
        let (status, value) = self.replies.lock().unwrap().pop_front().expect("no unexpected network requests");
        Ok(Reply { status, headers: BTreeMap::new(), body: serde_json::to_vec(&value).unwrap() })
    }
}

fn fixture() -> Value { serde_json::from_str(include_str!("../../../chilpa-meta/tests/fixtures/mb-credits.json")).unwrap() }
fn hint() -> Hint {
    Hint { title: "La Bamba".into(), artist: "Ritchie Valens".into(), album: "Greatest Hits".into(), album_artist: "Ritchie Valens".into(),
        duration_ms: 124_000, ..Default::default() }
}

#[test]
fn an_isrc_connection_failure_without_an_album_match_stays_retryable() {
    let http = Answers::new(vec![(503, Value::Null), (503, Value::Null), (503, Value::Null)]);
    let net = Net::free(&http);
    let library = Library::in_memory().unwrap();
    let reader = Reader { library: Some(&library), net: &net, online: true, now: 100 };
    let track = Hint { isrc: "USABC1200001".into(), ..Default::default() };
    assert!(reader.track(&track).is_err(), "a failed ISRC lookup is not confirmed absence");
}

#[test]
fn an_image_uses_its_album_overrides_instead_of_treating_the_track_uri_as_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("record.iso");
    let path = chilpa_core::sacd_tracks::path_of(&image, 1).display().to_string();
    let home = chilpa_library::sidecar::Home::of(&path, Some(image.to_str().unwrap()));
    let sidecar = json!({"format":"chilpa-sidecar", "version":1, "album":{
        "musicbrainz_release_id":"00000000-0000-4000-8000-000000000001", "album_artist":"Various Artists", "catalog_number":null}, "tracks":{}});
    std::fs::write(home.sidecar(), serde_json::to_vec(&sidecar).unwrap()).unwrap();
    let mut hint = Hint { catalog_number: "old number".into(), ..hint() };
    file_hint(&path, &mut hint);
    assert_eq!(hint.release_id, "00000000-0000-4000-8000-000000000001");
    assert_eq!(hint.album_artist, "Various Artists");
    assert_eq!(hint.catalog_number, "", "an explicit clear also overrides a previously known value");
}

#[test]
fn one_album_read_supplies_other_tracks_permanently_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let release = fixture();
    let search = json!({"releases":[{"id":release["id"],"title":release["title"],"artist-credit":release["artist-credit"],"score":100}]});
    let http = Answers::new(vec![(200, search), (200, release)]);
    let net = Net::free(&http);
    let reader = Reader { library: Some(&library), net: &net, online: true, now: 100 };
    let (credits, url) = reader.track(&hint()).unwrap();
    assert!(credits.iter().any(|c| c.name == "Bob Keane"));
    assert!(url.contains("/recording/"));
    assert!(reader.release(&hint()).unwrap().is_some());
    assert_eq!(http.count(), 2);
    drop(library);
    let library = Library::open_existing(&path).unwrap();
    let reader = Reader { library: Some(&library), net: &net, online: true, now: 100 + chilpa_library::store::metadata::FRESH_SECONDS * 100 };
    let other = Hint { title: "Dooby Dooby Wah".into(), duration_ms: 114_000, ..hint() };
    assert!(!reader.track(&other).unwrap().0.is_empty());
    assert_eq!(http.count(), 2, "a different track reuses the cached release, including its work relationships");
    let reader = Reader { library: Some(&library), net: &net, online: false, now: 100 + 2 * chilpa_library::store::metadata::FRESH_SECONDS };
    assert!(!reader.track(&other).unwrap().0.is_empty());
    assert_eq!(http.count(), 2, "offline mode reuses permanent information");
}

#[test]
fn confirmed_absence_is_cached_but_failed_refreshes_do_not_erase_or_renew_old_information() {
    let library = Library::in_memory().unwrap();
    let http = Answers::new(vec![(400, json!({"error":"failure"})), (200, json!(null)), (400, json!({"error":"failure"}))]);
    let net = Net::free(&http);
    let reader = Reader { library: Some(&library), net: &net, online: true, now: 100 };
    assert!(reader.read("key", || contributions::recording(&net, "00000000-0000-4000-8000-000000000001").map(|_| (Value::Null, Vec::new()))).is_err());
    assert!(library.metadata_cached("key").unwrap().is_none());
    assert!(reader.read("empty", || Ok((Value::Null, Vec::new()))).unwrap().is_null());
    assert!(reader.read("empty", || panic!("a successful empty answer is not requested again")).unwrap().is_null());
    library.cache_metadata("old", 1, &json!({"known":"information"}), &[]).unwrap();
    let reader = Reader { now: 1 + chilpa_library::store::metadata::FRESH_SECONDS, ..reader };
    let value = reader.read("old", || Err(Failure::Unreachable("no network".into()))).unwrap();
    assert_eq!(value["known"], "information");
    assert_eq!(library.metadata_cached("old").unwrap().unwrap().fetched_at, 1);
    let mut refreshed = false;
    reader.read("empty", || { refreshed = true; Ok((Value::Null, Vec::new())) }).unwrap();
    assert!(!refreshed, "this entry has not reached sixty days yet");
    let reader = Reader { now: 100 + chilpa_library::store::metadata::FRESH_SECONDS, ..reader };
    reader.read("empty", || { refreshed = true; Ok((Value::Null, Vec::new())) }).unwrap();
    assert!(refreshed, "the expiry boundary permits a new request");
}

#[test]
fn explicit_album_refresh_replaces_all_recordings_and_retains_previous_data_on_failure() {
    let library = Library::in_memory().unwrap();
    let original = fixture();
    let mut updated = original.clone();
    fn rename(value: &mut Value) {
        match value {
            Value::Object(fields) => { for value in fields.values_mut() { rename(value); } }
            Value::Array(rows) => { for value in rows { rename(value); } }
            Value::String(name) if name == "Bob Keane" => *name = "Updated producer".into(),
            _ => {}
        }
    }
    rename(&mut updated);
    let mut incomplete = updated.clone();
    incomplete["media"][0]["tracks"][0]["recording"].as_object_mut().unwrap().remove("relations");
    let http = Answers::new(vec![(200, original.clone()), (200, updated), (400, json!({"error":"failure"})), (200, incomplete)]);
    let net = Net::free(&http);
    let hint = Hint { release_id: original["id"].as_str().unwrap().into(), ..hint() };
    let reader = Reader { library: Some(&library), net: &net, online: true, now: 100 };
    reader.release(&hint).unwrap().unwrap();
    let reader = Reader { now: 100 + chilpa_library::store::metadata::FRESH_SECONDS * 100, ..reader };
    reader.release(&hint).unwrap().unwrap();
    assert_eq!(http.count(), 1, "positive data never triggers a timed refresh");
    let refreshed = reader.refresh_release(&hint).unwrap().unwrap();
    assert!(refreshed.tracks[0].contributions.iter().any(|credit| credit.name == "Updated producer"));
    for track in &refreshed.tracks {
        let kept = library.metadata_cached(&format!("mb.about.v1.recording:{}", track.recording_id)).unwrap().unwrap();
        assert!(kept.contributions.iter().any(|credit| credit.contributor.name == "Updated producer"));
    }
    let key = format!("mb.about.v1.release:{}", hint.release_id);
    let before = library.metadata_cached(&key).unwrap().unwrap().document;
    assert!(reader.refresh_release(&hint).is_err());
    assert_eq!(library.metadata_cached(&key).unwrap().unwrap().document, before);
    assert!(reader.refresh_release(&hint).is_err(), "missing relationships cannot erase permanent credits");
    assert_eq!(library.metadata_cached(&key).unwrap().unwrap().document, before);
    assert_eq!(http.count(), 4);
    let offline = Reader { online: false, ..reader };
    assert!(offline.refresh_release(&hint).is_err());
    assert!(offline.release(&hint).unwrap().is_some());
    assert_eq!(http.count(), 4);
}
