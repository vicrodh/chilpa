//! A whole sync against saved answers of real servers (CHD-0060 §4), into
//! a library in memory.

use super::*;
use chilpa_servers::{Reply, Request};
use std::collections::VecDeque;
use std::sync::Mutex;

/// Answers with the saved files it was loaded with, in order.
#[derive(Default)]
struct Saved {
    replies: Mutex<VecDeque<Result<Vec<u8>, String>>>,
    asked: Mutex<Vec<String>>,
}

impl Saved {
    fn file(self, name: &str) -> Self {
        let path = format!("{}/../chilpa-servers/tests/answers/{name}", env!("CARGO_MANIFEST_DIR"));
        self.replies.lock().unwrap().push_back(Ok(std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))));
        self
    }
    fn body(self, body: &str) -> Self {
        self.replies.lock().unwrap().push_back(Ok(body.as_bytes().to_vec()));
        self
    }
    fn silence(self) -> Self {
        self.replies.lock().unwrap().push_back(Err("the server could not be reached".into()));
        self
    }
    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Net for Saved {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        self.asked.lock().unwrap().push(request.url.clone());
        let body = self.replies.lock().unwrap().pop_front().unwrap_or_else(|| Err("nothing was loaded".into()))?;
        let mut headers = std::collections::BTreeMap::new();
        headers.insert("content-type".into(), "image/jpeg".into());
        Ok(Reply { status: 200, headers, body })
    }
}

struct Lab {
    lib: chilpa_library::Library,
    keys: Keys,
    source: i64,
    root: std::path::PathBuf,
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn lab(provider: &str, details: &str, key: chilpa_servers::Credential) -> Lab {
    let lib = chilpa_library::Library::in_memory().unwrap();
    let instance = super::super::keys::mint();
    let root = std::env::temp_dir().join(format!("chilpa-sync-{instance}"));
    let keys = Keys::derived(root.clone());
    assert!(keys.put(&instance, &key));
    let source = lib
        .add_server(&chilpa_library::NewServer {
            provider: provider.into(),
            instance,
            label: "Lab".into(),
            address: "http://lab.lan".into(),
            libraries: "[]".into(),
            details: details.into(),
            ..Default::default()
        })
        .unwrap();
    Lab { lib, keys, source, root }
}

fn subsonic() -> Lab {
    lab("subsonic", "{}", chilpa_servers::Credential::Password { user: "lab".into(), password: "lab-password".into() })
}

fn navidrome(net: Saved) -> Saved {
    census(
        net.file("navidrome/subsonic-scan.json")
            .file("navidrome/subsonic-albums.json")
            .file("navidrome/subsonic-album-0.json")
            .file("navidrome/subsonic-album-1.json")
            .file("navidrome/subsonic-album-2.json")
            .file("navidrome/subsonic-album-3.json"),
    )
}

/// What Subsonic answers when the sync asks, after the last page, how many
/// songs it has: its scan status and its record list again.
fn census(net: Saved) -> Saved {
    net.file("navidrome/subsonic-scan.json").file("navidrome/subsonic-albums.json")
}

fn albums_without(name: &str) -> String {
    let mut list: serde_json::Value = serde_json::from_slice(&std::fs::read(format!(
        "{}/../chilpa-servers/tests/answers/navidrome/subsonic-albums.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap();
    list["subsonic-response"]["albumList2"]["album"].as_array_mut().unwrap().retain(|a| a["name"] != name);
    list.to_string()
}

fn never() -> bool {
    false
}

#[test]
fn a_server_s_catalogue_lands_in_the_library_under_the_entries_the_queue_carries() {
    let lab = subsonic();
    let net = navidrome(Saved::default());
    let event = run(&lab.lib, &net, &lab.keys, lab.source, &|_| {}, &never);

    assert_eq!(event, Event::Finished { source: lab.source, gone: 0 });
    let server = lab.lib.server(lab.source).unwrap().unwrap();
    assert_eq!((server.tracks, server.albums), (12, 4));
    let entry: String = lab.lib.connection().query_row("SELECT external_id FROM tracks WHERE title = 'First Song 1'", [], |r| r.get(0)).unwrap();
    let parsed = chilpa_models::RemoteEntry::parse(&entry).expect("a queue entry");
    assert_eq!((parsed.source.provider.as_str(), parsed.source.instance.as_str()), ("subsonic", server.instance.as_str()));
    assert_eq!(parsed.external_id, "2FeDGrsepscsZy3Cv5BEmt", "the server's own id inside it");
    let genres: i64 = lab.lib.connection().query_row(
        "SELECT count(*) FROM track_genres tg JOIN tracks t ON t.id = tg.track_id WHERE t.title = 'First Song 1'", [], |r| r.get(0)).unwrap();
    assert_eq!(genres, 2);
    assert_eq!(lab.lib.pending_server_covers(lab.source, 10).unwrap().len(), 3, "the record without a cover asks for none");
}

#[test]
fn a_record_the_server_no_longer_has_leaves_and_a_sync_that_is_cut_takes_nothing() {
    let lab = subsonic();
    run(&lab.lib, &navidrome(Saved::default()), &lab.keys, lab.source, &|_| {}, &never);

    // Cut off at the second record.
    let net = Saved::default().file("navidrome/subsonic-scan.json").file("navidrome/subsonic-albums.json").file("navidrome/subsonic-album-0.json").silence();
    let event = run(&lab.lib, &net, &lab.keys, lab.source, &|_| {}, &never);
    assert!(matches!(event, Event::Failed { unreachable: true, .. }), "{event:?}");
    assert_eq!(lab.lib.track_count(false).unwrap(), 12, "nothing taken out");

    // A whole one without the compilation, which the server no longer has.
    let list = albums_without("Lab Compilation");
    let net = Saved::default()
        .file("navidrome/subsonic-scan.json")
        .body(&list)
        .file("navidrome/subsonic-album-0.json")
        .file("navidrome/subsonic-album-1.json")
        .file("navidrome/subsonic-album-3.json")
        .file("navidrome/subsonic-scan.json")
        .body(&list);
    assert_eq!(run(&lab.lib, &net, &lab.keys, lab.source, &|_| {}, &never), Event::Finished { source: lab.source, gone: 3 });
    assert_eq!(lab.lib.track_count(false).unwrap(), 9);
}

#[test]
fn a_record_skipped_between_pages_takes_nothing_out() {
    let lab = subsonic();
    run(&lab.lib, &navidrome(Saved::default()), &lab.keys, lab.source, &|_| {}, &never);

    // The pages missed the compilation, but the server still has it: the
    // count asked after the last page says so.
    let net = census(
        Saved::default()
            .file("navidrome/subsonic-scan.json")
            .body(&albums_without("Lab Compilation"))
            .file("navidrome/subsonic-album-0.json")
            .file("navidrome/subsonic-album-1.json")
            .file("navidrome/subsonic-album-3.json"),
    );
    let event = run(&lab.lib, &net, &lab.keys, lab.source, &|_| {}, &never);
    assert!(matches!(&event, Event::Failed { why, .. } if why.contains("9") && why.contains("12")), "{event:?}");
    assert_eq!(lab.lib.track_count(false).unwrap(), 12, "nothing taken out");
}

#[test]
fn a_record_listed_and_then_not_read_takes_nothing_out() {
    let lab = subsonic();
    run(&lab.lib, &navidrome(Saved::default()), &lab.keys, lab.source, &|_| {}, &never);
    let net = census(
        Saved::default()
            .file("navidrome/subsonic-scan.json")
            .file("navidrome/subsonic-albums.json")
            .file("navidrome/subsonic-album-0.json")
            .file("navidrome/subsonic-album-1.json")
            .body(r#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#)
            .file("navidrome/subsonic-album-3.json"),
    );
    let event = run(&lab.lib, &net, &lab.keys, lab.source, &|_| {}, &never);
    assert!(matches!(&event, Event::Failed { .. }), "{event:?}");
    assert_eq!(lab.lib.track_count(false).unwrap(), 12, "nothing taken out");
}

fn jellyfin() -> Lab {
    lab("jellyfin", r#"{"user_id":"501512e000854c8d832d2f1d2a26a0a0"}"#, chilpa_servers::Credential::Token { token: "t".into() })
}

#[test]
fn only_what_changed_is_asked_about_again() {
    let lab = jellyfin();
    let first = Saved::default().file("jellyfin/jellyfin-views.json").file("jellyfin/jellyfin-items.json").file("jellyfin/jellyfin-details.json");
    let event = run(&lab.lib, &first, &lab.keys, lab.source, &|_| {}, &never);
    // The saved details answer holds five of the twelve: the other seven
    // were not described, so they are not in the library yet, and the sync
    // says it could not read everything instead of finishing.
    assert!(matches!(&event, Event::Failed { why, unreachable: false, .. } if why.contains("describe")), "{event:?}");
    assert!(first.asked().iter().any(|u| u.contains("MediaSources")));
    let with_quality: i64 = lab.lib.connection().query_row("SELECT count(*) FROM tracks WHERE bit_depth IS NOT NULL", [], |r| r.get(0)).unwrap();
    assert_eq!(with_quality, 5);

    // The same listing again: the five known ones are kept as they are, and
    // only the seven never described are asked about.
    let second = Saved::default().file("jellyfin/jellyfin-views.json").file("jellyfin/jellyfin-items.json").file("jellyfin/jellyfin-details.json");
    run(&lab.lib, &second, &lab.keys, lab.source, &|_| {}, &never);
    let details = second.asked().into_iter().find(|u| u.contains("MediaSources")).unwrap();
    let ids = details.split("Ids=").nth(1).unwrap().split('&').next().unwrap();
    assert_eq!(ids.split(',').count(), 7, "{ids}");
}

#[test]
fn a_sync_that_saw_fewer_tracks_than_announced_takes_nothing_out() {
    let lab = jellyfin();
    let full = Saved::default().file("jellyfin/jellyfin-views.json").file("jellyfin/jellyfin-items.json").file("jellyfin/jellyfin-details.json");
    run(&lab.lib, &full, &lab.keys, lab.source, &|_| {}, &never);
    let before = lab.lib.track_count(false).unwrap();

    let mut items: serde_json::Value = serde_json::from_slice(&std::fs::read(format!(
        "{}/../chilpa-servers/tests/answers/jellyfin/jellyfin-items.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap();
    items["Items"].as_array_mut().unwrap().truncate(3);
    // Three of the twelve, then an empty page where the rest should be: what
    // a listing that skipped some looks like.
    let short = Saved::default()
        .file("jellyfin/jellyfin-views.json")
        .body(&items.to_string())
        .body(r#"{"Items":[],"TotalRecordCount":12,"StartIndex":3}"#);
    let event = run(&lab.lib, &short, &lab.keys, lab.source, &|_| {}, &never);
    assert!(matches!(&event, Event::Failed { why, .. } if why.contains("3") && why.contains("12")), "{event:?}");
    assert_eq!(lab.lib.track_count(false).unwrap(), before);
}

/// A media server as the setup adds it: no sealed key, two containers of a
/// Plex Media Server's DLNA side chosen, its records and one of its folders.
fn dlna() -> Lab {
    let lib = chilpa_library::Library::in_memory().unwrap();
    let instance = super::super::keys::mint();
    let root = std::env::temp_dir().join(format!("chilpa-sync-{instance}"));
    let keys = Keys::derived(root.clone());
    let source = lib
        .add_server(&chilpa_library::NewServer {
            provider: "dlna".into(),
            instance,
            label: "Lab".into(),
            address: "http://192.168.0.69:32469/DeviceDescription.xml".into(),
            libraries: r#"["9f4b4647f8e03ff196aa","551c51ca11833b35b735"]"#.into(),
            details: r#"{"control":"/ContentDirectory/f0e1d2c3-b4a5-4697-8879-6a5b4c3d2e1f/control.xml","search":false}"#.into(),
            ..Default::default()
        })
        .unwrap();
    Lab { lib, keys, source, root }
}

/// The records answer without its second record.
fn without_record_two() -> String {
    let albums = std::fs::read_to_string(format!(
        "{}/../chilpa-servers/tests/answers/dlna/plex-albums.xml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let start = albums.find("&lt;container id=\"6afbf8a54d0e842523ba\"").unwrap();
    let end = start + albums[start..].find("&lt;/container&gt;").unwrap() + "&lt;/container&gt;".len();
    format!("{}{}", &albums[..start], &albums[end..])
        .replace("<NumberReturned>2</NumberReturned><TotalMatches>2</TotalMatches>", "<NumberReturned>1</NumberReturned><TotalMatches>1</TotalMatches>")
}

fn walk(net: Saved) -> Saved {
    net.file("dlna/plex-albums.xml").file("dlna/plex-album-two.xml").file("dlna/plex-album-one.xml").file("dlna/plex-folder.xml")
}

#[test]
fn a_media_server_is_walked_without_a_sealed_key_and_each_track_lands_once() {
    let lab = dlna();
    let net = walk(Saved::default());
    let event = run(&lab.lib, &net, &lab.keys, lab.source, &|_| {}, &never);
    assert_eq!(event, Event::Finished { source: lab.source, gone: 0 });
    let server = lab.lib.server(lab.source).unwrap().unwrap();
    assert_eq!((server.tracks, server.albums), (4, 2), "the folder repeats three tracks the records already gave");
    assert_eq!(net.asked().len(), 4, "four containers, no census and no details");
    let entry: String = lab.lib.connection().query_row("SELECT external_id FROM tracks WHERE title = 'Lab Song One'", [], |r| r.get(0)).unwrap();
    let parsed = chilpa_models::RemoteEntry::parse(&entry).expect("a queue entry");
    assert_eq!((parsed.source.provider.as_str(), parsed.external_id.as_str()), ("dlna", "b2000000000000000001"));
    let (codec, revision) = lab.lib.server_track(lab.source, &entry).unwrap().unwrap();
    let stream = chilpa_servers::stream(&net::server(&server).unwrap(), &chilpa_servers::Credential::Anonymous, &parsed.external_id, codec.as_deref(), revision.as_deref());
    assert_eq!(stream.url, "http://192.168.0.69:32469/object/b2000000000000000001/file.flac");
    assert_eq!(stream.extension.as_deref(), Some("flac"));

    // The same answers again change nothing.
    assert_eq!(run(&lab.lib, &walk(Saved::default()), &lab.keys, lab.source, &|_| {}, &never), Event::Finished { source: lab.source, gone: 0 });
    assert_eq!(lab.lib.track_count(false).unwrap(), 4);

    // A record the server no longer lists leaves; a walk cut short takes nothing.
    let net = Saved::default().body(&without_record_two()).file("dlna/plex-album-one.xml").file("dlna/plex-folder.xml");
    assert_eq!(run(&lab.lib, &net, &lab.keys, lab.source, &|_| {}, &never), Event::Finished { source: lab.source, gone: 1 });
    assert_eq!(lab.lib.track_count(false).unwrap(), 3);
    let cut = Saved::default().file("dlna/plex-albums.xml").silence();
    let event = run(&lab.lib, &cut, &lab.keys, lab.source, &|_| {}, &never);
    assert!(matches!(event, Event::Failed { unreachable: true, .. }), "{event:?}");
    assert_eq!(lab.lib.track_count(false).unwrap(), 3, "nothing taken out");
}

#[test]
fn a_cover_is_fetched_once_into_the_server_s_own_folder() {
    let lab = subsonic();
    run(&lab.lib, &navidrome(Saved::default()), &lab.keys, lab.source, &|_| {}, &never);
    let net = Saved::default().body("jpeg-1").body("jpeg-2").body("jpeg-3");
    assert_eq!(covers(&lab.lib, &net, &lab.keys, lab.source, 0, &never), None);
    assert!(lab.lib.pending_server_covers(lab.source, 10).unwrap().is_empty());
    let art: String = lab.lib.connection().query_row(
        "SELECT artwork_path FROM albums WHERE title = 'Lab Album One'", [], |r| r.get(0)).unwrap();
    let instance = lab.lib.server(lab.source).unwrap().unwrap().instance;
    assert!(art.starts_with(&covers_dir(&instance).display().to_string()), "{art}");
    assert!(std::fs::read(&art).is_ok());
    let _ = std::fs::remove_dir_all(covers_dir(&instance));
}

/// What 10.10.7 answers the playlists' owner asking whether it may change one.
const EDITS: &str = r#"{"UserId":"501512e000854c8d832d2f1d2a26a0a0","CanEdit":true}"#;

/// A saved answer of the lab's, read as JSON to be changed.
fn answer(name: &str) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(format!("{}/../chilpa-servers/tests/answers/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
}

/// The lab's catalogue cut to the five tracks its saved details describe,
/// so a sync of it finishes and the playlists find what they name.
fn five_described() -> Saved {
    let described: Vec<String> = answer("jellyfin/jellyfin-details.json")["Items"].as_array().unwrap().iter().map(|t| t["Id"].as_str().unwrap().to_string()).collect();
    let mut items = answer("jellyfin/jellyfin-items.json");
    items["Items"].as_array_mut().unwrap().retain(|t| described.iter().any(|id| t["Id"] == id.as_str()));
    items["TotalRecordCount"] = serde_json::json!(5);
    Saved::default().file("jellyfin/jellyfin-views.json").body(&items.to_string()).file("jellyfin/jellyfin-details.json")
}

/// The lab's two music playlists, each asked whether the account may change it.
fn road_and_empty() -> Saved {
    Saved::default().file("jellyfin/jellyfin-playlists.json").file("jellyfin/jellyfin-playlist-empty.json").body(EDITS).file("jellyfin/jellyfin-playlist-items.json").body(EDITS)
}

fn titles(lab: &Lab, playlist: i64) -> Vec<String> {
    lab.lib.playlist_entries(playlist).unwrap().iter().map(|e| lab.lib.connection().query_row("SELECT title FROM tracks WHERE id = ?1", [e.track_id.unwrap()], |r| r.get(0)).unwrap()).collect()
}

#[test]
fn a_server_s_playlists_come_in_after_its_catalogue_in_the_server_s_order() {
    let lab = jellyfin();
    assert_eq!(run(&lab.lib, &five_described(), &lab.keys, lab.source, &|_| {}, &never), Event::Finished { source: lab.source, gone: 0 });
    let lists = road_and_empty();
    let event = reading::read(&lab.lib, &lists, &lab.keys, lab.source, &Mutex::new(()), &never);
    // Hi-Res D2 T1 is in "Road Trip" and not among the five: it is left out.
    assert_eq!(event, Event::Playlists { source: lab.source, kept: 2, left_out: 1 });
    let asked = lists.asked();
    assert_eq!(asked.len(), 5, "the list, then each music playlist and whether it may be changed: {asked:?}");
    assert!(asked.iter().all(|u| !u.contains("7c4e1a9b2d5f4083b6e9a1c7d3f5e28b")), "a playlist of videos is not read");

    let stored = lab.lib.playlists().unwrap();
    assert_eq!(stored.len(), 2);
    let road = stored.iter().find(|p| p.name == "Road Trip").unwrap();
    assert!(road.server && road.writable, "the server's, and the account may change it there");
    assert_eq!((road.description.as_str(), road.left_out), ("Songs for the drive", 1));
    assert_eq!(titles(&lab, road.id), ["First Song 1", "Comp Track 2", "First Song 1", "Comp Track 1"], "its order and its repeat; the video and the missing track left out");
    let entries = lab.lib.playlist_entries(road.id).unwrap();
    assert!(chilpa_models::RemoteEntry::parse(&entries[0].external_id).is_some(), "an entry is the track's queue entry: {}", entries[0].external_id);
    assert_eq!(stored.iter().find(|p| p.name == "Empty for now").unwrap().track_count, 0);
    assert_eq!(lab.lib.server_playlists_left_out(lab.source).unwrap(), 1);
}

#[test]
fn a_partial_sync_still_brings_the_playlists_in_and_a_missing_track_asks_for_the_catalogue() {
    let lab = jellyfin();
    let partial = Saved::default().file("jellyfin/jellyfin-views.json").file("jellyfin/jellyfin-items.json").file("jellyfin/jellyfin-details.json");
    let event = run(&lab.lib, &partial, &lab.keys, lab.source, &|_| {}, &never);
    assert!(matches!(&event, Event::Failed { unreachable: false, .. }), "{event:?}");
    assert!(reads_playlists_after(&event), "partial: its playlists are read anyway");
    assert!(!reads_playlists_after(&Event::Failed { source: lab.source, why: String::new(), unreachable: true }), "not from a server that does not answer");
    let event = reading::read(&lab.lib, &road_and_empty(), &lab.keys, lab.source, &Mutex::new(()), &never);
    assert_eq!(event, Event::Playlists { source: lab.source, kept: 2, left_out: 1 });
    assert!(asks_for_catalogue(&event), "read alone, a track the library lacks asks for a sync first");
    let again = reading::read(&lab.lib, &road_and_empty(), &lab.keys, lab.source, &Mutex::new(()), &never);
    assert_eq!(again, Event::Playlists { source: lab.source, kept: 2, left_out: 1 });
    assert!(asks_for_catalogue(&again), "still missing at the next reading alone, it asks again");
    assert!(!asks_for_catalogue(&Event::Playlists { source: lab.source, kept: 2, left_out: 0 }));
    assert!(!asks_for_catalogue(&Event::PlaylistsFailed { source: lab.source, why: String::new() }));
}

#[test]
fn a_playlist_deleted_on_the_server_leaves_and_one_that_cannot_be_read_stays() {
    let lab = jellyfin();
    run(&lab.lib, &five_described(), &lab.keys, lab.source, &|_| {}, &never);
    reading::read(&lab.lib, &road_and_empty(), &lab.keys, lab.source, &Mutex::new(()), &never);

    // "Empty for now" was deleted on the server; "Road Trip" answers with
    // something that is not a playlist's items.
    let mut listing = answer("jellyfin/jellyfin-playlists.json");
    listing["Items"].as_array_mut().unwrap().retain(|p| p["Name"] != "Empty for now");
    listing["TotalRecordCount"] = serde_json::json!(2);
    let again = Saved::default().body(&listing.to_string()).body("<html>Bad gateway</html>");
    assert_eq!(reading::read(&lab.lib, &again, &lab.keys, lab.source, &Mutex::new(()), &never), Event::Playlists { source: lab.source, kept: 0, left_out: 0 });
    let stored = lab.lib.playlists().unwrap();
    assert_eq!(stored.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Road Trip"], "gone there, gone here");
    assert_eq!(stored[0].track_count, 4, "one that could not be read keeps what it had");
}

#[test]
fn a_server_that_stops_answering_leaves_its_playlists_as_they_were() {
    let lab = jellyfin();
    run(&lab.lib, &five_described(), &lab.keys, lab.source, &|_| {}, &never);
    reading::read(&lab.lib, &road_and_empty(), &lab.keys, lab.source, &Mutex::new(()), &never);

    let event = reading::read(&lab.lib, &Saved::default().silence(), &lab.keys, lab.source, &Mutex::new(()), &never);
    assert!(matches!(&event, Event::PlaylistsFailed { why, .. } if why.contains("could not be reached")), "{event:?}");
    let cut = Saved::default().file("jellyfin/jellyfin-playlists.json").file("jellyfin/jellyfin-playlist-empty.json").body(EDITS).silence();
    assert!(matches!(reading::read(&lab.lib, &cut, &lab.keys, lab.source, &Mutex::new(()), &never), Event::PlaylistsFailed { .. }));
    let stored = lab.lib.playlists().unwrap();
    assert_eq!(stored.len(), 2, "nothing taken out");
    assert_eq!(stored.iter().find(|p| p.name == "Road Trip").unwrap().track_count, 4);
}

#[test]
fn a_subsonic_playlist_of_another_user_is_read_and_not_changed_here() {
    let lab = subsonic();
    assert_eq!(run(&lab.lib, &navidrome(Saved::default()), &lab.keys, lab.source, &|_| {}, &never), Event::Finished { source: lab.source, gone: 0 });
    lab.lib.connection().execute("UPDATE servers SET account = 'someone' WHERE source_id = ?1", [lab.source]).unwrap();
    let empty = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","playlist":{"id":"0QwErTyUiOpAsDfGhJkLzX","name":"Empty for now","entry":[]}}}"#;
    let lists = Saved::default().file("navidrome/subsonic-playlists.json").file("navidrome/subsonic-playlist.json").body(empty);
    assert_eq!(reading::read(&lab.lib, &lists, &lab.keys, lab.source, &Mutex::new(()), &never), Event::Playlists { source: lab.source, kept: 2, left_out: 0 });
    let stored = lab.lib.playlists().unwrap();
    assert!(stored.iter().all(|p| p.server && !p.writable), "both are the lab user's, not this account's");
    assert_eq!(stored.iter().find(|p| p.name == "Road Trip").unwrap().track_count, 5, "the video is not a track");
}

/// A media server added with these containers chosen.
fn media_server(lib: chilpa_library::Library, libraries: &str) -> Lab {
    let instance = super::super::keys::mint();
    let root = std::env::temp_dir().join(format!("chilpa-sync-{instance}"));
    let keys = Keys::derived(root.clone());
    let source = lib
        .add_server(&chilpa_library::NewServer {
            provider: "dlna".into(),
            instance,
            label: "Lab".into(),
            address: "http://192.168.0.69:32469/DeviceDescription.xml".into(),
            libraries: libraries.into(),
            details: r#"{"control":"/ContentDirectory/f0e1d2c3-b4a5-4697-8879-6a5b4c3d2e1f/control.xml","search":false}"#.into(),
            ..Default::default()
        })
        .unwrap();
    Lab { lib, keys, source, root }
}

/// What lies between `open` and `close` in `text`.
fn between<'a>(text: &'a str, open: &str, close: &str) -> &'a str {
    let start = text.find(open).map(|at| at + open.len()).unwrap_or(text.len());
    let end = text[start..].find(close).map_or(text.len(), |at| start + at);
    &text[start..end]
}

/// A ContentDirectory made up as it is asked: a container `top` with
/// `records` records of `songs` songs each. It gives at most `most` entries
/// at a time whatever is asked for, does not count what `top` holds, and,
/// asked past its end, gives its last page again.
struct Shelves {
    records: usize,
    songs: usize,
    most: usize,
    asked: std::sync::atomic::AtomicUsize,
}

impl Shelves {
    fn entries(&self, id: &str) -> Vec<String> {
        if id == "top" {
            return (0..self.records)
                .map(|r| format!("<container id=\"r{r}\" parentID=\"top\" restricted=\"1\"><dc:title>Record {r}</dc:title><upnp:class>object.container.album.musicAlbum</upnp:class></container>"))
                .collect();
        }
        (0..self.songs)
            .map(|s| {
                format!(
                    "<item id=\"{id}-{s}\" parentID=\"{id}\" restricted=\"1\"><dc:title>Song {s}</dc:title><upnp:artist>Artist {id}</upnp:artist>\
                     <upnp:album>Record {id}</upnp:album><upnp:albumArtURI>/art/{id}.jpg</upnp:albumArtURI>\
                     <res protocolInfo=\"http-get:*:audio/flac:*\">http://192.168.0.69:32469/object/{id}-{s}.flac</res>\
                     <upnp:class>object.item.audioItem.musicTrack</upnp:class></item>"
                )
            })
            .collect()
    }
}

impl Net for Shelves {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        self.asked.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let body = String::from_utf8_lossy(&request.body.as_ref().expect("a SOAP request").1).into_owned();
        let id = between(&body, "<ObjectID>", "</ObjectID>").to_string();
        let start: usize = between(&body, "<StartingIndex>", "</StartingIndex>").parse().unwrap();
        let entries = self.entries(&id);
        let start = if start >= entries.len() { entries.len().saturating_sub(1) / self.most * self.most } else { start };
        let given = &entries[start..(start + self.most).min(entries.len())];
        let total = if id == "top" { 0 } else { entries.len() };
        let didl = format!(
            "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
             xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\">{}</DIDL-Lite>",
            given.concat()
        );
        let escaped = didl.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let answer = format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
             <u:BrowseResponse xmlns:u=\"urn:schemas-upnp-org:service:ContentDirectory:1\"><Result>{escaped}</Result>\
             <NumberReturned>{}</NumberReturned><TotalMatches>{total}</TotalMatches><UpdateID>1</UpdateID>\
             </u:BrowseResponse></s:Body></s:Envelope>",
            given.len()
        );
        Ok(Reply { status: 200, headers: Default::default(), body: answer.into_bytes() })
    }
}

#[test]
fn a_media_server_larger_than_its_pages_comes_in_whole_and_is_written_in_batches() {
    let lab = media_server(chilpa_library::Library::in_memory().unwrap(), r#"["top"]"#);
    let shelves = Shelves { records: 875, songs: 12, most: 50, asked: Default::default() };
    let readings = std::cell::RefCell::new(Vec::new());
    let began = Instant::now();
    let event = run(&lab.lib, &shelves, &lab.keys, lab.source, &|reading| readings.borrow_mut().push(reading), &never);

    // `top` does not count: read whole, it still cannot be shown whole.
    let partial = chilpa_i18n::t("the server did not describe every track it listed; nothing was taken out, sync again");
    assert_eq!(event, Event::Failed { source: lab.source, why: partial, unreachable: false });
    let server = lab.lib.server(lab.source).unwrap().unwrap();
    assert_eq!((server.tracks, server.albums), (10_500, 875), "every track past the first short page and the ones after it");
    // 18 pages of `top`, one more that came again, and one per record.
    assert_eq!(shelves.asked.load(std::sync::atomic::Ordering::Relaxed), 18 + 1 + 875);
    let written = readings.borrow().iter().filter(|r| r.wrote).count();
    let allowed = 10_500 / WRITE_TRACKS + 1 + (began.elapsed().as_secs() / WRITE_EVERY.as_secs()) as usize;
    assert!(written <= allowed, "{written} writes for 875 pages: written in batches, not page by page");
    assert_eq!(readings.borrow().last().map(|r| (r.done, r.wrote)), Some((10_500, true)));
    assert_eq!(lab.lib.pending_server_covers(lab.source, 1_000).unwrap().len(), 875, "each record's art is named for its covers");

    // Its covers come in runs of a bounded size; the rest wait for another.
    let images = Statuses(Mutex::new(std::iter::repeat(200).take(COVERS_PER_RUN).collect()));
    let next = covers(&lab.lib, &images, &lab.keys, lab.source, 0, &never);
    assert!(next.is_some(), "the rest wait for another run");
    assert_eq!(lab.lib.pending_server_covers(lab.source, 1_000).unwrap().len(), 875 - COVERS_PER_RUN);
    let _ = std::fs::remove_dir_all(covers_dir(&server.instance));
}

/// `Shelves` behind a connection that drops. The `fall_at`th request finds
/// the server gone for `gone` requests, and a Browse of `poison` takes it
/// down for `gone` requests each time it is asked. Up, it answers a ping.
struct Falls {
    shelves: Shelves,
    fall_at: usize,
    poison: Option<&'static str>,
    gone: usize,
    down: std::sync::atomic::AtomicUsize,
    sent: std::sync::atomic::AtomicUsize,
    poisoned: std::sync::atomic::AtomicUsize,
}

impl Falls {
    fn new(records: usize, fall_at: usize, poison: Option<&'static str>, gone: usize) -> Self {
        let shelves = Shelves { records, songs: 2, most: 50, asked: Default::default() };
        Self { shelves, fall_at, poison, gone, down: Default::default(), sent: Default::default(), poisoned: Default::default() }
    }
}

impl Net for Falls {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        use std::sync::atomic::Ordering::SeqCst;
        let body = String::from_utf8_lossy(&request.body.as_ref().expect("a SOAP request").1).into_owned();
        if self.sent.fetch_add(1, SeqCst) + 1 == self.fall_at {
            self.down.store(self.gone, SeqCst);
        }
        let browsed = body.contains("<ObjectID>").then(|| between(&body, "<ObjectID>", "</ObjectID>").to_string());
        if self.down.load(SeqCst) == 0 && browsed.is_some() && browsed.as_deref() == self.poison {
            self.poisoned.fetch_add(1, SeqCst);
            self.down.store(self.gone, SeqCst);
        }
        if self.down.load(SeqCst) > 0 {
            self.down.fetch_sub(1, SeqCst);
            return Err("connection reset by peer".into());
        }
        if browsed.is_none() {
            return Ok(Reply { status: 200, headers: Default::default(), body: b"<Id>1</Id>".to_vec() });
        }
        self.shelves.send(request)
    }
}

#[test]
fn a_media_server_that_goes_away_is_waited_for_and_read_on_from_the_same_page() {
    let lab = media_server(chilpa_library::Library::in_memory().unwrap(), r#"["top"]"#);
    // The fourth request finds it gone: three tries and a ping on the
    // walk's side, then two of the sync's pings, and it is back.
    let net = Falls::new(6, 4, None, 6);
    let readings = std::cell::RefCell::new(Vec::new());
    let event = run(&lab.lib, &net, &lab.keys, lab.source, &|reading| readings.borrow_mut().push(reading), &never);

    // `top` does not count, so the sync is partial for that alone.
    let partial = chilpa_i18n::t("the server did not describe every track it listed; nothing was taken out, sync again");
    assert_eq!(event, Event::Failed { source: lab.source, why: partial, unreachable: false });
    assert_eq!(lab.lib.server(lab.source).unwrap().unwrap().tracks, 12, "every record, the one it went away on too");
    let waited: Vec<_> = readings.borrow().iter().map(|r| r.waiting).collect();
    assert!(waited.contains(&true), "it said it was waiting");
    assert_eq!(readings.borrow().last().map(|r| r.waiting), Some(false), "and that it went on");
}

#[test]
fn a_container_that_takes_the_server_down_twice_is_left_out_and_the_rest_comes_in() {
    let lab = media_server(chilpa_library::Library::in_memory().unwrap(), r#"["top"]"#);
    let net = Falls::new(6, 0, Some("r2"), 5);
    let event = run(&lab.lib, &net, &lab.keys, lab.source, &|_| {}, &never);

    assert_eq!(net.poisoned.load(std::sync::atomic::Ordering::SeqCst), 2, "asked again once, then left out");
    let said = chilpa_i18n::tf(
        "{} tracks came in; {} folder of the server could not be read and was left out. Nothing was taken out; sync again.",
        "{} tracks came in; {} folders of the server could not be read and were left out. Nothing was taken out; sync again.",
        1,
        &["10", "1"],
    );
    assert_eq!(event, Event::Failed { source: lab.source, why: said, unreachable: false });
    assert_eq!(lab.lib.server(lab.source).unwrap().unwrap().tracks, 10);
}

#[test]
fn a_media_server_that_does_not_come_back_keeps_what_was_read_and_says_so() {
    let lab = media_server(chilpa_library::Library::in_memory().unwrap(), r#"["top"]"#);
    // Gone from the fourth request on: two pages of `top`, one record read.
    let net = Falls::new(6, 4, None, usize::MAX / 2);
    let event = run(&lab.lib, &net, &lab.keys, lab.source, &|_| {}, &never);

    let said = chilpa_i18n::tf(
        "{} did not come back within 30 minutes. The {} track read is kept and nothing was taken out; sync again when it is back.",
        "{} did not come back within 30 minutes. The {} tracks read are kept and nothing was taken out; sync again when it is back.",
        2,
        &["Lab", "2"],
    );
    assert_eq!(event, Event::Failed { source: lab.source, why: said, unreachable: true });
    assert_eq!(lab.lib.server(lab.source).unwrap().unwrap().tracks, 2, "what was read is kept");
}

#[test]
fn a_sync_waiting_for_its_server_says_whom_and_how_much_it_read() {
    let mut running = Running::asked();
    running.started = Some(Instant::now());
    running.done = 1234;
    running.waiting = true;
    let said = super::super::sync_state(&running, "Lab");
    assert_eq!(said, chilpa_i18n::tf("Waiting for {} to come back… ({} track read)", "Waiting for {} to come back… ({} tracks read)", 1234, &["Lab", "1234"]));
}

/// Answers each request with the next status it was loaded with, and the
/// first bytes of a JPEG under a generic type.
struct Statuses(Mutex<VecDeque<u16>>);

impl Net for Statuses {
    fn send(&self, _: &Request) -> Result<Reply, String> {
        let status = self.0.lock().unwrap().pop_front().ok_or("nothing was loaded")?;
        let mut headers = std::collections::BTreeMap::new();
        headers.insert("content-type".into(), "application/octet-stream".into());
        Ok(Reply { status, headers, body: vec![0xFF, 0xD8, 0xFF, 0xE0] })
    }
}

#[test]
fn a_cover_the_server_could_not_give_is_asked_again_a_few_times() {
    let lab = subsonic();
    run(&lab.lib, &navidrome(Saved::default()), &lab.keys, lab.source, &|_| {}, &never);
    assert_eq!(lab.lib.pending_server_covers(lab.source, 10).unwrap().len(), 3);
    let net = Statuses(Mutex::new([503, 404, 200].into()));
    assert_eq!(covers(&lab.lib, &net, &lab.keys, lab.source, 0, &never), None, "three is one run");
    assert_eq!(lab.lib.pending_server_covers(lab.source, 10).unwrap().len(), 1, "only the one it could not give now");
    let saved: i64 = lab.lib.connection().query_row("SELECT count(*) FROM albums WHERE artwork_path LIKE '%.jpg'", [], |r| r.get(0)).unwrap();
    assert_eq!(saved, 1, "a JPEG under a generic type is kept as one");
    for _ in 1..chilpa_library::store::servers::COVER_TRIES {
        covers(&lab.lib, &Statuses(Mutex::new([500].into())), &lab.keys, lab.source, 0, &never);
    }
    assert!(lab.lib.pending_server_covers(lab.source, 10).unwrap().is_empty(), "missed every time, it is left");
    let _ = std::fs::remove_dir_all(covers_dir(&lab.lib.server(lab.source).unwrap().unwrap().instance));
}

/// A media server that breaks at its first request, as a bug of the
/// protocol would: it panics. Then it answers as the lab's walk did, and a
/// cover is never there.
struct Breaks {
    broke: std::sync::atomic::AtomicBool,
}

impl Net for Breaks {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        if !self.broke.swap(true, std::sync::atomic::Ordering::SeqCst) {
            panic!("a protocol that panics");
        }
        let Some((_, body)) = request.body.as_ref() else {
            return Ok(Reply { status: 404, headers: Default::default(), body: Vec::new() });
        };
        let body = String::from_utf8_lossy(body);
        let file = match between(&body, "<ObjectID>", "</ObjectID>") {
            "9f4b4647f8e03ff196aa" => "plex-albums.xml",
            "6afbf8a54d0e842523ba" => "plex-album-two.xml",
            "f3e534d429a852d0bd40" => "plex-album-one.xml",
            "551c51ca11833b35b735" => "plex-folder.xml",
            other => panic!("not one of the lab's containers: {other}"),
        };
        let path = format!("{}/../chilpa-servers/tests/answers/dlna/{file}", env!("CARGO_MANIFEST_DIR"));
        Ok(Reply { status: 200, headers: Default::default(), body: std::fs::read(&path).unwrap() })
    }
}

/// Polls until the sync of `source` ends.
fn ended(syncer: &mut Syncer, source: i64) -> Event {
    let end = Instant::now() + Duration::from_secs(60);
    while Instant::now() < end {
        for event in syncer.poll() {
            if matches!(event, Event::Finished { source: s, .. } | Event::Failed { source: s, .. } if s == source) {
                return event;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the sync of {source} did not end");
}

#[test]
fn a_sync_that_panics_lets_its_server_go_and_the_next_sync_runs_whole() {
    let instance = super::super::keys::mint();
    let path = std::env::temp_dir().join(format!("chilpa-sync-thread-{instance}.db"));
    let lab = media_server(chilpa_library::Library::open(&path).unwrap(), r#"["9f4b4647f8e03ff196aa","551c51ca11833b35b735"]"#);
    let at = path.clone();
    let open: Open = Arc::new(move || chilpa_library::Library::open(&at).ok());
    let net: Arc<dyn Net> = Arc::new(Breaks { broke: std::sync::atomic::AtomicBool::new(false) });
    let keys = Arc::new(Keys::derived(lab.root.clone()));
    let mut syncer = Syncer::with(Tools { net, open, keys, gate: Arc::default() });

    syncer.ask(lab.source);
    let event = ended(&mut syncer, lab.source);
    let error = chilpa_i18n::t("the sync stopped on an error; nothing was taken out, sync again");
    assert_eq!(event, Event::Failed { source: lab.source, why: error, unreachable: false });
    assert!(!syncer.running.contains_key(&lab.source), "no longer shown as syncing");

    // The same thread takes the next one.
    syncer.ask(lab.source);
    assert_eq!(ended(&mut syncer, lab.source), Event::Finished { source: lab.source, gone: 0 });
    assert_eq!(lab.lib.server(lab.source).unwrap().unwrap().tracks, 4);

    // A thread that is gone is noticed when a sync is asked of it, and one
    // in its place reads the server.
    let (orders, _) = channel::<Order>();
    let (_, events) = channel::<Event>();
    syncer.orders = orders;
    syncer.events = events;
    syncer.ask(lab.source);
    assert_eq!(ended(&mut syncer, lab.source), Event::Finished { source: lab.source, gone: 0 });

    let stored = lab.lib.server(lab.source).unwrap().unwrap();
    let _ = std::fs::remove_dir_all(covers_dir(&stored.instance));
    drop(syncer);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_sync_stopped_to_start_over_is_asked_again_when_it_ends() {
    let (orders, inbox) = channel::<Order>();
    let (outbox, events) = channel::<Event>();
    let tools = Tools {
        net: Arc::new(Saved::default()),
        open: Arc::new(|| -> Option<chilpa_library::Library> { None }),
        keys: Arc::new(Keys::derived(std::env::temp_dir().join("chilpa-sync-restart-keys"))),
        gate: Arc::default(),
    };
    let mut syncer = Syncer {
        tools,
        orders,
        events,
        cancel: Arc::default(),
        running: HashMap::new(),
        queued: HashSet::new(),
        reading: HashSet::new(),
        again: HashSet::new(),
        covering: HashMap::new(),
        replaced_at: None,
    };
    let sent = |inbox: &Receiver<Order>| inbox.try_iter().map(|o| match o { Order::Catalogue(s) => s, Order::Playlists(s) | Order::Covers(s, _) => -s }).collect::<Vec<i64>>();
    syncer.ask(7);
    syncer.ask(7);
    assert_eq!(sent(&inbox), [7], "asked while it runs, it goes on");
    syncer.restart(7);
    assert!(syncer.cancel.lock().unwrap().contains(&7), "the running one stops at its next page");
    outbox.send(Event::Failed { source: 7, why: String::new(), unreachable: false }).unwrap();
    syncer.poll();
    assert!(syncer.running.contains_key(&7));
    assert_eq!(sent(&inbox), [7], "and the new one is asked when it ends");

    // Removed while it waits to start over, nothing more is asked.
    syncer.restart(7);
    syncer.cancel(7);
    outbox.send(Event::Failed { source: 7, why: String::new(), unreachable: false }).unwrap();
    syncer.poll();
    assert!(!syncer.running.contains_key(&7));
    assert!(sent(&inbox).is_empty());
}
