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
