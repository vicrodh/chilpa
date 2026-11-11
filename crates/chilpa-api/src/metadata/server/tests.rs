//! Saving a server's record to the server, against a network that answers
//! as the lab servers did (`chilpa-servers/tests/answers`): nothing here
//! leaves the machine.

use super::super::draft::{Draft, HomeState};
use super::super::save::{self as local_save};
use super::super::Mode;
use super::*;
use chilpa_library::sidecar::Home;
use chilpa_library::{Hosted, StoredAlbum};
use chilpa_servers::{Reply, Request};
use serde_json::{json, Value};
use std::sync::Mutex;

type Answer = Box<dyn Fn(&Request) -> Result<(u16, Vec<u8>), String> + Send + Sync>;

/// Answers each request with what `answer` says of it, and keeps what it
/// was asked.
struct Fake {
    answer: Answer,
    asked: Mutex<Vec<Request>>,
}

impl Fake {
    fn new(answer: impl Fn(&Request) -> Result<(u16, Vec<u8>), String> + Send + Sync + 'static) -> Arc<Fake> {
        Arc::new(Fake { answer: Box::new(answer), asked: Mutex::new(Vec::new()) })
    }
    fn asked(&self) -> Vec<Request> {
        self.asked.lock().unwrap().clone()
    }
}

impl Net for Fake {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        self.asked.lock().unwrap().push(request.clone());
        let (status, body) = (self.answer)(request)?;
        Ok(Reply { status, headers: Default::default(), body })
    }
}

fn saved(name: &str) -> Value {
    let path = format!("{}/../chilpa-servers/tests/answers/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap()
}

fn stored(provider: &str) -> chilpa_library::StoredServer {
    chilpa_library::StoredServer {
        source_id: 1,
        provider: provider.into(),
        instance: "inst".into(),
        label: "Lab".into(),
        reachable: true,
        enabled: true,
        address: "http://lab.lan".into(),
        account: String::new(),
        identity: String::new(),
        product: String::new(),
        libraries: "[]".into(),
        details: json!({ "user_id": "u1" }).to_string(),
        offline_folder: String::new(),
        added_at: 0,
        session_revision: 0,
        synced_at: None,
        tracks: 0,
        albums: 0,
    }
}

fn reach(provider: &str, net: Arc<Fake>) -> Reach {
    Reach { net, keys: None, credential: Some(Credential::Token { token: "tok".into() }), stored: stored(provider), offline: false }
}

/// A record of two tracks on server 1, as the library keeps them.
fn tracks(provider: &str) -> Vec<EditTrack> {
    (1..=2)
        .map(|n| EditTrack {
            id: n,
            external_id: format!("remote:{provider}/inst/t{n}"),
            container: None,
            hosted: Some(Hosted { source_id: 1, album_key: Some("al".into()) }),
            title: format!("Song {n}"),
            artist: "Band".into(),
            track_number: Some(n as u16),
            disc_number: Some(1),
            disc_subtitle: String::new(),
            work: String::new(),
            recording_mbid: None,
            explicit: None,
            genres: vec!["Rock".into()],
        })
        .collect()
}

fn album() -> StoredAlbum {
    StoredAlbum { id: 7, title: "One".into(), album_artist: "Band".into(), year: Some(2001), artwork_path: None, track_count: 2, explicit: None }
}

/// What a server says of the record's tracks, as `read` turns it.
fn known(n: u32) -> Read {
    let said = Snapshot {
        title: Some(format!("Song {n}")),
        artist_credit: vec!["Band".into()],
        album: Some("One".into()),
        album_artist: Some("Band".into()),
        date: Some("2001".into()),
        genres: vec!["Rock".into()],
        label: Some("Label".into()),
        track_number: Some(n),
        disc_number: Some(1),
        ..Snapshot::default()
    };
    Read { said, known: true, placed: Vec::new() }
}

fn home(sidecar: Option<Sidecar>) -> HomeState {
    HomeState { home: Home::hosted(1, Some("al"), "remote:jellyfin/inst/t1"), of: "remote:jellyfin/inst/t1".into(), sidecar, unreadable: String::new(), xml: None }
}

fn draft(provider: &str, sidecar: Option<Sidecar>) -> Draft {
    Draft::open(&album(), tracks(provider), vec![known(1), known(2)], vec![home(sidecar)])
}

fn state(kind: Kind) -> ServerState {
    ServerState { source_id: 1, name: "Lab".into(), kind: Some(kind), writable: true, why: String::new(), album_key: Some("al".into()) }
}

#[test]
fn a_save_sends_only_what_changed_to_the_items_that_hold_it() {
    let mut draft = draft("jellyfin", None);
    assert!(draft.set_album("title", "One, corrected"));
    assert!(draft.set_album("label", "Other label"));
    assert!(draft.set_album("date", "2001"), "typed back as it was: not a change");
    assert!(draft.set_track(1, "title", "First"));
    assert!(draft.set_track(2, "track_number", "9"));
    let made = plan(&draft, &state(Kind::Jellyfin), None, 5).unwrap();
    let (record, edit) = made.album.clone().unwrap();
    assert_eq!(record, "al");
    assert_eq!((edit.title.as_deref(), edit.label.as_deref(), edit.date.as_deref()), (Some("One, corrected"), Some("Other label"), None));
    assert_eq!(made.tracks.len(), 2);
    assert_eq!(made.tracks[0].0, "t1");
    assert_eq!(made.tracks[0].1.album.as_deref(), Some("One, corrected"), "Jellyfin's tracks carry the record's title");
    assert_eq!(made.tracks[0].1.title.as_deref(), Some("First"));
    assert_eq!(made.tracks[0].1.label, None, "a label is the record's alone");
    assert_eq!(made.tracks[1].1.track_number, Some(Some(9)));
    assert_eq!(made.every, ["t1", "t2"]);
    assert_eq!(made.asked.len(), 4);
    assert!(made.homes.is_empty(), "no sidecar was there, and none is made");
}

#[test]
fn plex_tracks_carry_only_their_record_s_genres() {
    let mut draft = draft("plex", None);
    draft.set_album("title", "Renamed");
    draft.set_album("genres", "Blues; Soul");
    let made = plan(&draft, &state(Kind::Plex), None, 5).unwrap();
    let edit = made.album.unwrap().1;
    assert_eq!((edit.title.as_deref(), edit.genres.clone()), (Some("Renamed"), Some(vec!["Blues".to_string(), "Soul".to_string()])));
    for (_, track) in &made.tracks {
        assert_eq!(track.genres, Some(vec!["Blues".to_string(), "Soul".to_string()]));
        assert_eq!(track.album, None, "a Plex track reads its record's title");
    }
}

#[test]
fn a_field_the_server_does_not_keep_refuses_the_save_and_goes_nowhere_else() {
    let mut draft = draft("plex", None);
    draft.set_track(1, "composers", "Somebody");
    assert_eq!(plan(&draft, &state(Kind::Plex), None, 5).err(), Some(not_kept()));
    let mut draft = super::super::draft::testing::plain();
    draft.rows.iter_mut().for_each(|row| row.track.hosted = Some(Hosted { source_id: 1, album_key: Some("al".into()) }));
    let mut no_cover = state(Kind::Plex);
    no_cover.album_key = None;
    let cover = local_save_waiting();
    assert!(plan(&draft, &no_cover, Some(cover), 5).is_err(), "a cover the server takes none of is refused");
}

fn local_save_waiting() -> super::super::artwork::Waiting {
    super::super::artwork::Waiting {
        bytes: super::super::artwork::testing::picture(8, image::ImageFormat::Png),
        extension: "png",
        mime: "image/png",
        width: 8,
        height: 8,
        preview: std::path::PathBuf::from("/nowhere/preview.png"),
        from: "file",
    }
}

#[test]
fn a_field_saved_to_the_server_leaves_the_sidecar_and_the_others_stay() {
    let mut sidecar = Sidecar::new(0);
    sidecar.album.title = Override::Value("Laid over".into());
    sidecar.album.catalog_number = Override::Value("CAT-1".into());
    let mut draft = draft("jellyfin", Some(sidecar));
    assert_eq!(draft.album_cell("title").now, "One", "the record's title is what the library shows");
    draft.set_album("title", "Typed now");
    let made = plan(&draft, &state(Kind::Jellyfin), None, 5).unwrap();
    assert_eq!(made.album.unwrap().1.title.as_deref(), Some("Typed now"));
    assert_eq!(made.homes.len(), 1);
    assert!(made.homes[0].sidecar.album.title.is_absent(), "the latest choice was the server");
    assert_eq!(made.homes[0].sidecar.album.catalog_number, Override::Value("CAT-1".into()), "what nobody touched stays laid over");
}

#[test]
fn typing_back_the_server_s_value_takes_the_sidecar_off_and_sends_nothing() {
    let mut sidecar = Sidecar::new(0);
    sidecar.track_mut("t1").title = Override::Value("Mine".into());
    let mut draft = draft("jellyfin", Some(sidecar.clone()));
    let at = super::super::fields::track("title").unwrap();
    assert_eq!(draft.rows[0].cells[at].now, "Mine", "the sidecar wins while it is there");
    assert_eq!(draft.rows[0].file[at], "Song 1", "and what the server says is known");
    draft.set_track(1, "title", "Song 1");
    let made = plan(&draft, &state(Kind::Jellyfin), None, 5).unwrap();
    assert!(made.album.is_none() && made.tracks.is_empty() && made.cover.is_none(), "nothing to send");
    assert_eq!(made.homes.len(), 1);
    assert!(made.homes[0].sidecar.is_empty(), "the overlay goes, and the server's word shows");
    // Saved to the sidecar instead, the same typing takes it off too.
    let mut draft = self::draft("jellyfin", Some(sidecar));
    draft.set_track(1, "title", "Song 1");
    let local = local_save::plan(&draft, Mode::Sidecar, chilpa_tags::edit::Options::default(), None, 5);
    assert!(local.hosted[0].sidecar.is_empty(), "typed back to what the server says, nothing is laid over it");
}

#[test]
fn a_change_saved_to_the_sidecar_stays_on_this_device_and_is_not_sent() {
    let mut draft = draft("jellyfin", None);
    draft.set_track(2, "composers", "Somebody");
    draft.set_album("title", "Mine only");
    let local = local_save::plan(&draft, Mode::Sidecar, chilpa_tags::edit::Options::default(), None, 5);
    assert_eq!(local.hosted.len(), 1);
    assert_eq!(local.hosted[0].sidecar.album.title, Override::Value("Mine only".into()));
    assert_eq!(local.hosted[0].sidecar.tracks["t2"].composers, Override::Value(vec!["Somebody".into()]));
    assert!(local.files.iter().all(|f| f.is_empty()), "no file is written");
}

/// A Jellyfin lab server that answers what it was saved saying.
fn jellyfin_lab(admin: bool) -> Arc<Fake> {
    let album = saved("jellyfin/edit-album.json");
    let track = saved("jellyfin/edit-track.json");
    Fake::new(move |request| {
        let path = request.url.split('?').next().unwrap_or("");
        Ok(match (request.method, path) {
            ("GET", p) if p.ends_with("/Users/Me") => (200, json!({ "Policy": { "IsAdministrator": admin } }).to_string().into_bytes()),
            ("GET", p) if p.ends_with("/Items/al") => (200, album.to_string().into_bytes()),
            ("GET", p) if p.ends_with("/Items") => {
                let ids = request.url.split("Ids=").nth(1).unwrap_or("").split('&').next().unwrap_or("");
                let items: Vec<Value> = ids
                    .split(',')
                    .map(|id| {
                        let mut t = track.clone();
                        t["Id"] = json!(id);
                        t
                    })
                    .collect();
                (200, json!({ "Items": items, "TotalRecordCount": items.len() }).to_string().into_bytes())
            }
            ("GET", p) if p.contains("/Items/t") => (200, track.to_string().into_bytes()),
            ("GET", p) if p.ends_with("/Images/Primary") => (200, super::super::artwork::testing::picture(4, image::ImageFormat::Png)),
            ("POST", _) => (204, Vec::new()),
            _ => (404, Vec::new()),
        })
    })
}

#[test]
fn opening_reads_what_the_server_says_and_whether_it_takes_changes() {
    let net = jellyfin_lab(true);
    let (reads, state) = read(Some(&reach("jellyfin", net.clone())), &tracks("jellyfin"));
    let state = state.unwrap();
    assert!(state.writable, "{}", state.why);
    assert_eq!(state.kind, Some(Kind::Jellyfin));
    let said = &reads[0].as_ref().unwrap().said;
    assert_eq!(said.title.as_deref(), Some("Blackened"));
    assert_eq!(said.album.as_deref(), Some("...And Justice For All"), "the record's own title");
    assert_eq!(said.label.as_deref(), Some("Elektra"));
    assert_eq!(said.date.as_deref(), Some("1988-01-01"));
    assert!(net.asked().iter().all(|r| r.method == "GET"), "opening only reads");

    let (reads, state) = read(Some(&reach("jellyfin", jellyfin_lab(false))), &tracks("jellyfin"));
    let state = state.unwrap();
    assert!(!state.writable && state.why.contains("administrator"), "{}", state.why);
    assert!(reads[0].is_some(), "what it says is still shown");
}

#[test]
fn a_server_that_cannot_take_changes_is_said_and_not_asked() {
    let net = Fake::new(|_| panic!("nothing is asked of it"));
    let (reads, state) = read(Some(&reach("subsonic", net.clone())), &tracks("subsonic"));
    let state = state.unwrap();
    assert!(!state.writable && !state.why.is_empty());
    assert!(reads.iter().all(Option::is_none), "the library's mirror stands for it");
    let mut offline = reach("jellyfin", net.clone());
    offline.offline = true;
    let (_, state) = read(Some(&offline), &tracks("jellyfin"));
    assert!(!state.unwrap().writable);
    let mut off = reach("jellyfin", net.clone());
    off.stored.enabled = false;
    assert!(!read(Some(&off), &tracks("jellyfin")).1.unwrap().writable);
    assert!(net.asked().is_empty());
    let (_, state) = read(None, &tracks("jellyfin"));
    assert!(!state.unwrap().why.is_empty(), "a server the library no longer has says so");
    let gone = Fake::new(|_| Err("no route to host".into()));
    let (_, state) = read(Some(&reach("jellyfin", gone)), &tracks("jellyfin"));
    let state = state.unwrap();
    assert!(!state.writable && state.why.contains("no route to host"), "{}", state.why);
    let local: Vec<EditTrack> = tracks("jellyfin").into_iter().map(|mut t| { t.hosted = None; t }).collect();
    assert_eq!(read(None, &local).1, None, "a record on no server has no server state");
}

#[test]
fn a_save_writes_each_item_then_reads_the_record_back() {
    let net = jellyfin_lab(true);
    let mut draft = draft("jellyfin", None);
    draft.set_album("title", "One, corrected");
    draft.set_track(1, "title", "First");
    let made = plan(&draft, &state(Kind::Jellyfin), None, 5).unwrap();
    let outcome = match run(3, made, reach("jellyfin", net.clone())) {
        Done::ServerSaved { session, outcome } => {
            assert_eq!(session, 3);
            outcome
        }
        _ => panic!("a save to the server ends as one"),
    };
    assert_eq!((outcome.sent, outcome.total, outcome.failed.as_deref()), (3, 3, None));
    assert_eq!(outcome.tracks.len(), 2, "every track of the record is read back");
    assert!(outcome.unread.is_none());
    assert_eq!(outcome.unlike, 2, "the fake server keeps what it had: both fields say otherwise");
    let posts: Vec<String> = net.asked().iter().filter(|r| r.method == "POST").map(|r| r.url.clone()).collect();
    assert_eq!(posts.len(), 3);
    assert!(posts[0].ends_with("/Items/t1") && posts[1].ends_with("/Items/t2") && posts[2].ends_with("/Items/al"), "the tracks before their record: {posts:?}");
    assert!(net.asked().iter().all(|r| r.method != "DELETE"));
}

#[test]
fn a_save_that_fails_says_how_far_it_went() {
    let refused = Fake::new(|request| Ok(if request.method == "POST" { (403, Vec::new()) } else { (200, saved("jellyfin/edit-track.json").to_string().into_bytes()) }));
    let mut draft = draft("jellyfin", None);
    draft.set_track(1, "title", "First");
    let made = plan(&draft, &state(Kind::Jellyfin), None, 5).unwrap();
    let Done::ServerSaved { outcome, .. } = run(1, made, reach("jellyfin", refused.clone())) else { panic!() };
    assert_eq!(outcome.sent, 0);
    assert!(outcome.failed.as_deref().is_some_and(|why| !why.is_empty()));
    assert!(outcome.tracks.is_empty(), "nothing reached it: nothing is read back");

    let posts = std::sync::atomic::AtomicUsize::new(0);
    let second = Fake::new(move |request| {
        let path = request.url.split('?').next().unwrap_or("").to_string();
        if request.method == "POST" {
            let n = posts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Ok(if n == 0 { (204, Vec::new()) } else { (500, Vec::new()) });
        }
        if path.ends_with("/Items") {
            let t = saved("jellyfin/edit-track.json");
            return Ok((200, json!({ "Items": [t] }).to_string().into_bytes()));
        }
        Ok((200, saved("jellyfin/edit-track.json").to_string().into_bytes()))
    });
    let mut draft = self::draft("jellyfin", None);
    draft.set_track(1, "title", "First");
    draft.set_track(2, "title", "Second");
    let made = plan(&draft, &state(Kind::Jellyfin), None, 5).unwrap();
    let Done::ServerSaved { outcome, .. } = run(1, made, reach("jellyfin", second)) else { panic!() };
    assert_eq!((outcome.sent, outcome.total), (1, 2));
    assert!(outcome.failed.is_some());
    assert_eq!(outcome.tracks.len(), 1, "what reached it is read back");
}

#[test]
fn a_cover_goes_to_the_server_and_comes_back_from_it() {
    let net = jellyfin_lab(true);
    let draft = draft("jellyfin", None);
    let made = plan(&draft, &state(Kind::Jellyfin), Some(local_save_waiting()), 5).unwrap();
    assert!(made.album.is_none() && made.tracks.is_empty());
    let Done::ServerSaved { outcome, .. } = run(1, made, reach("jellyfin", net.clone())) else { panic!() };
    assert!(outcome.cover_sent && outcome.failed.is_none(), "{:?}", outcome.failed);
    let upload = net.asked().into_iter().find(|r| r.method == "POST").unwrap();
    assert!(upload.url.ends_with("/Items/al/Images/Primary"));
    let (mime, body) = upload.body.unwrap();
    assert_eq!(mime, "image/png");
    assert!(body.starts_with(b"iVBORw0KGgo"), "base64 of a PNG");
    let (bytes, ext) = outcome.cover.expect("the cover the server gives now");
    assert_eq!(ext, "png");
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
}
