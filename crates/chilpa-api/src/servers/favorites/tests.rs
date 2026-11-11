//! Hearts between the library and a server, against the shapes of the lab
//! servers' answers and a network that answers what it was loaded with.

use super::super::ratings::tests::{lab_library, navidrome, ok, plex, plex_catalogue, subsonic, Answers, Lab};
use super::*;
use crate::ratings::writer;
use std::sync::Arc;

/// Settings › Library's switch, put back on when the test ends.
struct AllOn<'a>(&'a mut crate::Chilpa);
impl Drop for AllOn<'_> {
    fn drop(&mut self) {
        self.0.adjust(SYNC, "true");
    }
}

fn starred() -> Answers {
    navidrome("navidrome/subsonic-album-0.json").file("navidrome/subsonic-starred2.json")
}

fn due(lib: &chilpa_library::Library) -> Vec<(String, String, bool)> {
    lib.due_favorites(i64::MAX, 10).unwrap().into_iter().map(|c| (c.kind, c.id, c.on)).collect()
}

fn hearted(lab: &Lab, lib: &chilpa_library::Library, kind: &str, id: &str) -> bool {
    use chilpa_library::store::favorite_sync::Hearted;
    lib.is_favorite(if kind == "track" { Hearted::Track(lab.track(id)) } else { Hearted::Album(lab.album(id)) }).unwrap()
}

#[test]
fn a_sync_takes_in_a_subsonic_server_s_starred_songs_and_records_and_writes_nothing() {
    let lab = subsonic();
    let net = starred();
    assert_eq!(lab.sync(&net), Event::Finished { source: lab.source, gone: 0 });
    assert!(hearted(&lab, &lab.lib, "track", "47gLTJvR49ADB1Pa0vPhB5"));
    assert!(hearted(&lab, &lab.lib, "album", "2NGalRARWUlolzE5BntvyN"));
    assert!(!hearted(&lab, &lab.lib, "track", "4AbMVAGmME5H9RzBSSy1Rr"));
    assert!(net.asked().last().unwrap().1.contains("/rest/getStarred2.view?"), "favorites are listed after the catalogue");
    assert!(net.asked().iter().all(|(_, url)| !url.contains("/rest/star.view") && !url.contains("/rest/unstar.view")));
    assert!(due(&lab.lib).is_empty());
}

#[test]
fn a_heart_changed_here_goes_to_the_server_and_waits_while_it_cannot_take_it() {
    let mut api = crate::Chilpa::new();
    let lab = subsonic();
    lab.sync(&starred());
    api.library = Some(lab_library(&lab));
    let (song, record) = (lab.track("4AbMVAGmME5H9RzBSSy1Rr"), lab.album("2NGalRARWUlolzE5BntvyN"));
    assert!(api.set_favorite("track", &lab.track("47gLTJvR49ADB1Pa0vPhB5").to_string(), true), "already a heart: no change");
    assert!(due(api.library().unwrap()).is_empty());
    assert!(api.set_tracks_favorite(&[song], true));
    assert!(api.set_favorite("album", &record.to_string(), false));
    assert_eq!(due(api.library().unwrap()), [("track".to_string(), "4AbMVAGmME5H9RzBSSy1Rr".to_string(), true), ("album".to_string(), "2NGalRARWUlolzE5BntvyN".to_string(), false)]);
    assert!(api.library().unwrap().due_ratings(i64::MAX, 10).unwrap().is_empty(), "a heart is no rating");
    let net = Arc::new(Answers::default().silence().status(200, ok()).status(200, ok()));
    let now = 1_000_000;
    let failed = writer::run(api.library().unwrap(), &lab.tools(net.clone()), now);
    assert_eq!((failed.sent, failed.failing), (1, 1));
    let sent = writer::run(api.library().unwrap(), &lab.tools(net.clone()), now + 30);
    assert_eq!((sent.sent, sent.failing, sent.next_due), (1, 0, None));
    let urls: Vec<String> = net.asked().into_iter().map(|(_, url)| url).collect();
    assert!(urls.iter().any(|u| u.contains("/rest/star.view?") && u.contains("id=4AbMVAGmME5H9RzBSSy1Rr")), "{urls:?}");
    assert!(urls.iter().any(|u| u.contains("/rest/unstar.view?") && u.contains("albumId=2NGalRARWUlolzE5BntvyN")), "{urls:?}");
}

#[test]
fn turned_off_for_every_server_or_for_one_nothing_comes_in_and_nothing_goes() {
    let mut api = crate::Chilpa::new();
    let lab = subsonic();
    lab.sync(&navidrome("navidrome/subsonic-album-0.json"));
    api.library = Some(lab_library(&lab));
    let all = AllOn(&mut api);
    let key = format!("sources.server/{}", lab.source);
    let row = |api: &mut crate::Chilpa| api.server_rows(&Default::default()).into_iter().find(|row| row.key == key).unwrap();
    assert!(row(all.0).options.contains(&"Stop syncing favorites".to_string()));
    let song = lab.track("4AbMVAGmME5H9RzBSSy1Rr");
    assert!(all.0.adjust(SYNC, "false"));
    assert!(all.0.set_tracks_favorite(&[song], true));
    assert!(due(all.0.library().unwrap()).is_empty(), "off for every server");
    assert!(!on_for_all());
    assert!(all.0.adjust(SYNC, "true"));
    assert!(all.0.adjust(&key, "favorites-off"));
    let off = row(all.0);
    assert!(off.values.contains(&"favorites-on".to_string()) && off.description.contains("Favorites are not synced with it."), "{}", off.description);
    assert!(off.values.contains(&"ratings-off".to_string()), "ratings keep their own switch");
    assert!(all.0.set_tracks_favorite(&[song], false));
    assert!(due(all.0.library().unwrap()).is_empty(), "off for this server");
    assert!(all.0.rate_as("track", song, 3));
    assert_eq!(all.0.library().unwrap().due_ratings(i64::MAX, 10).unwrap().len(), 1, "its ratings still go");
    let off_lab = subsonic();
    off_lab.lib.set_favorite_sync(&off_lab.origin(), false).unwrap();
    off_lab.sync(&navidrome("navidrome/subsonic-album-0.json"));
    assert!(!hearted(&off_lab, &off_lab.lib, "track", "47gLTJvR49ADB1Pa0vPhB5"), "no listing is even asked for");
}

#[test]
fn plex_keeps_no_heart_and_five_stars_are_not_one() {
    let mut api = crate::Chilpa::new();
    let lab = plex();
    lab.sync(&plex_catalogue());
    api.library = Some(lab_library(&lab));
    let track = lab.track("18");
    assert!(!hearted(&lab, api.library().unwrap(), "track", "18"), "rated 10 on the server, and still no heart");
    assert!(api.set_tracks_favorite(&[track], true));
    assert!(due(api.library().unwrap()).is_empty(), "a heart on a Plex track stays in Chilpa");
    assert!(api.library().unwrap().due_ratings(i64::MAX, 10).unwrap().is_empty(), "and does not move its stars");
    let key = format!("sources.server/{}", lab.source);
    let row = api.server_rows(&Default::default()).into_iter().find(|row| row.key == key).unwrap();
    assert!(!row.values.iter().any(|v| v.starts_with("favorites-")), "no favorites entry for a server that keeps none");
}

#[test]
fn jellyfin_takes_a_heart_with_post_and_drops_one_for_a_gone_item() {
    let lab = Lab::new("jellyfin", "[]", chilpa_servers::Credential::Token { token: "tok".into() });
    lab.lib.update_server(lab.source, "Lab", "http://lab.lan", "Jellyfin", "[]", r#"{"user_id":"u1"}"#).unwrap();
    lab.lib.queue_favorite(&lab.origin(), "track", "t1", true, None).unwrap();
    lab.lib.queue_favorite(&lab.origin(), "album", "gone", false, None).unwrap();
    let net = Arc::new(Answers::default().status(200, r#"{"IsFavorite":true}"#).status(404, ""));
    let outcome = writer::run(&lab.lib, &lab.tools(net.clone()), crate::now_unix());
    assert_eq!((outcome.sent, outcome.dropped, outcome.failing), (1, 1, 0));
    assert_eq!(net.asked()[0], ("POST".to_string(), "http://lab.lan/UserFavoriteItems/t1?userId=u1".to_string()));
    assert_eq!(net.asked()[1].0, "DELETE");
    assert_eq!(lab.lib.remote_favorite(&lab.origin(), "track", "t1").unwrap(), Some(true));
}
