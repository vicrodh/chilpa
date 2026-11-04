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
    covers(&lab.lib, &net, &lab.keys, lab.source, &never);
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
