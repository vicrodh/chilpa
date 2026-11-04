//! A server of mine, added, synced and played through the facade, against a
//! real server (ADR-017). Ignored: each needs a server, named by a variable.
//!
//! ```text
//! CHILPA_LAB_SUBSONIC="http://127.0.0.1:4533 lab lab-password"
//! CHILPA_LAB_JELLYFIN="http://127.0.0.1:8096 lab lab-password"
//! CHILPA_LAB_JELLYFIN_PLAYLIST="<playlist name>"  # one its playlists test waits for
//! CHILPA_LAB_JELLYFIN_WRITE=1               # its playlists test also changes the server
//! CHILPA_LAB_PLEX="<account token>"        # a Plex account, instead of the PIN
//! CHILPA_LAB_PLEX_SERVER="<server name>"    # which of the account's servers, when it sees several
//! CHILPA_LAB_PLEX_SECTION="<section name>"  # which music section, instead of all of them
//! cargo test -p chilpa-api --test servers_lab -- --ignored --test-threads=1
//! ```
//!
//! Nothing plays on a device: the output is `file:capture`.

use std::time::{Duration, Instant};

const OUTPUT: &str = "file:capture";

fn isolated(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("chilpa-servers-lab-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The library, the credentials and the covers go here, never into the
    // library of whoever runs this.
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    dir
}

fn until(chilpa: &mut chilpa_api::Chilpa, what: &str, limit: Duration, done: impl Fn(&mut chilpa_api::Chilpa) -> bool) {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        chilpa.poll();
        if done(chilpa) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("{what} did not happen: {:?}", chilpa.server_setup());
}

fn step(chilpa: &mut chilpa_api::Chilpa, name: &str) {
    let name = name.to_string();
    until(chilpa, &format!("step {name}"), Duration::from_secs(30), move |c| c.server_setup().step == name && !c.server_setup().busy);
}

/// Waits for the first sync to finish, and returns the server's row.
fn synced(chilpa: &mut chilpa_api::Chilpa) -> chilpa_api::ServerRow {
    until(chilpa, "the first sync", Duration::from_secs(300), |c| c.servers().first().is_some_and(|s| !s.syncing && s.tracks > 0));
    chilpa.servers().remove(0)
}

/// Plays the server's first track to the capture output and checks what the
/// window would show.
fn plays_a_track(chilpa: &mut chilpa_api::Chilpa, dir: &std::path::Path) {
    let lib = chilpa_library::Library::open(&dir.join("data/chilpa/library.db")).unwrap();
    let entry: String = lib
        .connection()
        .query_row("SELECT external_id FROM tracks WHERE external_id LIKE 'remote:%' AND bit_depth IS NOT NULL ORDER BY title LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert!(chilpa_api::on_a_server(&entry), "{entry}");
    chilpa.play_tracks(&[entry], OUTPUT, false);
    // Launching marks it playing; the track's name comes with its start.
    until(chilpa, "the track starting", Duration::from_secs(20), |c| c.now_playing().total_seconds > 0.0 || !c.now_playing().error.is_empty());
    let now = chilpa.now_playing().clone();
    assert!(now.error.is_empty(), "{}", now.error);
    assert!(!now.title.is_empty() && !now.title.starts_with("remote:"), "named by the library: {}", now.title);
    assert!(!now.quality.format.is_empty(), "labelled by its codec: {:?}", now.quality);
    assert!(now.total_seconds > 0.0, "its length is known");
    until(chilpa, "the end of the track", Duration::from_secs(120), |c| !c.now_playing().playing);
    assert!(chilpa.now_playing().error.is_empty(), "{}", chilpa.now_playing().error);
}

fn lab(var: &str) -> Option<(String, String, String)> {
    let value = std::env::var(var).ok()?;
    let mut parts = value.split_whitespace().map(str::to_string);
    Some((parts.next()?, parts.next().unwrap_or_default(), parts.next().unwrap_or_default()))
}

#[test]
#[ignore]
fn an_opensubsonic_server_is_added_synced_played_and_removed() {
    let Some((address, user, password)) = lab("CHILPA_LAB_SUBSONIC") else { return eprintln!("CHILPA_LAB_SUBSONIC is not set") };
    let dir = isolated("subsonic");
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.start_servers();

    chilpa.setup_server("subsonic");
    chilpa.setup_address(&address);
    step(&mut chilpa, "signin");
    chilpa.setup_sign_in(&user, &password, false);
    step(&mut chilpa, "libraries");
    let all: Vec<String> = chilpa.server_setup().choices.iter().map(|(id, _, _)| id.clone()).collect();
    assert!(chilpa.setup_finish(&all));
    let row = synced(&mut chilpa);
    assert_eq!(row.provider, "subsonic");
    eprintln!("synced: {} tracks, {} records, {}", row.tracks, row.albums, row.state);

    plays_a_track(&mut chilpa, &dir);

    let said = chilpa.server_removal(row.id);
    let facts: serde_json::Value = serde_json::from_str(&said).unwrap_or_else(|e| panic!("{said}: {e}"));
    assert_eq!(facts["tracks"].as_u64(), Some(row.tracks as u64), "{said}");
    assert!(chilpa.remove_server(row.id));
    // Taken out away from the window: its sync stops, then its rows go.
    until(&mut chilpa, "the removal", Duration::from_secs(120), |c| c.servers().is_empty());
    let lib = chilpa_library::Library::open(&dir.join("data/chilpa/library.db")).unwrap();
    assert_eq!(lib.track_count(true).unwrap(), 0, "no trace left");
}

#[test]
#[ignore]
fn a_jellyfin_server_is_added_synced_and_played() {
    let Some((address, user, password)) = lab("CHILPA_LAB_JELLYFIN") else { return eprintln!("CHILPA_LAB_JELLYFIN is not set") };
    let dir = isolated("jellyfin");
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.start_servers();

    chilpa.setup_server("jellyfin");
    chilpa.setup_address(&address);
    step(&mut chilpa, "signin");
    assert!(chilpa.server_setup().product.starts_with("Jellyfin"));
    chilpa.setup_sign_in(&user, &password, false);
    step(&mut chilpa, "libraries");
    let all: Vec<String> = chilpa.server_setup().choices.iter().map(|(id, _, _)| id.clone()).collect();
    assert!(chilpa.setup_finish(&all));
    let row = synced(&mut chilpa);
    eprintln!("synced: {} tracks, {} records, {}", row.tracks, row.albums, row.state);
    plays_a_track(&mut chilpa, &dir);
}

/// The server's own playlists, after its sync, as the library keeps them;
/// then, only with `CHILPA_LAB_JELLYFIN_WRITE=1`, a playlist made for the
/// test is created, added to, reordered, taken from, renamed and deleted
/// through the server, each change read back from it. Nothing else of the
/// server is changed.
#[test]
#[ignore]
fn a_jellyfin_server_s_playlists_come_in_and_take_changes() {
    let Some((address, user, password)) = lab("CHILPA_LAB_JELLYFIN") else { return eprintln!("CHILPA_LAB_JELLYFIN is not set") };
    let dir = isolated("jellyfin-playlists");
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.start_servers();
    chilpa.setup_server("jellyfin");
    chilpa.setup_address(&address);
    step(&mut chilpa, "signin");
    chilpa.setup_sign_in(&user, &password, false);
    step(&mut chilpa, "libraries");
    let all: Vec<String> = chilpa.server_setup().choices.iter().map(|(id, _, _)| id.clone()).collect();
    assert!(chilpa.setup_finish(&all));
    let row = synced(&mut chilpa);
    // They come after the catalogue: one named, or any.
    let wanted = std::env::var("CHILPA_LAB_JELLYFIN_PLAYLIST").ok();
    until(&mut chilpa, "the server's playlists", Duration::from_secs(180), |c| c.cached_playlists().iter().any(|p| p.source_id == row.id && wanted.as_ref().is_none_or(|w| &p.name == w)));
    for playlist in chilpa.cached_playlists().into_iter().filter(|p| p.source_id == row.id) {
        eprintln!("{} ({}): {} tracks, changeable: {}", playlist.name, playlist.external_id, playlist.track_count, playlist.writable);
        assert!(!playlist.mixed && !playlist.source.is_empty(), "{} is the server's", playlist.name);
        let rows = chilpa.playlist_rows(playlist.id);
        assert_eq!(rows.len(), playlist.track_count);
        assert!(rows.iter().all(|r| chilpa_api::on_a_server(&r.path)), "every entry of {} is one of its tracks", playlist.name);
    }
    eprintln!("{}", chilpa.servers().remove(0).state);
    if std::env::var("CHILPA_LAB_JELLYFIN_WRITE").as_deref() != Ok("1") {
        return eprintln!("CHILPA_LAB_JELLYFIN_WRITE is not 1: nothing was written to the server");
    }

    let lib = chilpa_library::Library::open(&dir.join("data/chilpa/library.db")).unwrap();
    let mut find = lib.connection().prepare("SELECT id, external_id FROM tracks WHERE source_id = ?1 AND present = 1 ORDER BY title LIMIT 3").unwrap();
    let tracks: Vec<(i64, String)> = find.query_map([row.id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect();
    assert_eq!(tracks.len(), 3, "three of its tracks to work with");
    let ids: Vec<i64> = tracks.iter().map(|(id, _)| *id).collect();
    let sources: Vec<serde_json::Value> = serde_json::from_str(&chilpa.playlist_sources_json(&ids[..2])).unwrap();
    let key = sources.iter().find(|s| s["name"] == row.name.as_str()).and_then(|s| s["id"].as_str()).expect("the server among the sources").to_string();
    let name = format!("Chilpa lab {}", std::process::id());
    assert!(chilpa.write_source_playlist(&key, 0, &name, "", &ids[..2], "lab-create"), "{}", chilpa.now_playing().error);
    until(&mut chilpa, "the playlist made", Duration::from_secs(60), |c| c.playlist_write_state(&key).contains(r#""pending":false"#));
    assert!(!chilpa.playlist_write_state(&key).contains("error"), "{}", chilpa.playlist_write_state(&key));
    let made = chilpa.cached_playlists().into_iter().find(|p| p.name == name).expect("it came in");
    assert_eq!(made.track_count, 2);
    let id = made.id;
    let landed = |what: &str, chilpa: &mut chilpa_api::Chilpa| {
        until(chilpa, what, Duration::from_secs(60), |c| !c.server_playlist_json(id).contains(r#""pending":true"#));
        assert!(chilpa.now_playing().error.is_empty(), "{what}: {}", chilpa.now_playing().error);
    };
    assert_eq!(chilpa.add_track_ids_to_playlist(id, &ids[2..]).added, 1);
    landed("a track added", &mut chilpa);
    assert_eq!(chilpa.playlist(id).unwrap().track_count, 3);
    assert!(chilpa.move_in_playlist(id, 2, 0));
    landed("a new order", &mut chilpa);
    assert_eq!(chilpa.playlist_rows(id)[0].path, tracks[2].1, "the last one is first, as the server reads it back");
    assert!(chilpa.remove_from_playlist(id, 0));
    landed("a track taken out", &mut chilpa);
    assert_eq!(chilpa.playlist(id).unwrap().track_count, 2);
    chilpa.rename_playlist(id, &format!("{name} again"));
    landed("a new name", &mut chilpa);
    assert_eq!(chilpa.playlist(id).unwrap().name, format!("{name} again"));
    chilpa.delete_playlist(id);
    until(&mut chilpa, "the playlist deleted", Duration::from_secs(60), |c| c.playlist(id).is_none());
    assert!(chilpa.now_playing().error.is_empty(), "{}", chilpa.now_playing().error);
}

#[test]
#[ignore]
fn a_plex_server_is_added_synced_and_played() {
    let Ok(token) = std::env::var("CHILPA_LAB_PLEX") else { return eprintln!("CHILPA_LAB_PLEX is not set") };
    let dir = isolated("plex");
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.start_servers();

    chilpa.setup_server("plex");
    chilpa.setup_plex_account(&token);
    until(&mut chilpa, "a server or its libraries", Duration::from_secs(60), |c| {
        matches!(c.server_setup().step.as_str(), "servers" | "libraries") && !c.server_setup().busy
    });
    if chilpa.server_setup().step == "servers" {
        // An account can see servers shared by someone else; the lab names its own.
        let wanted = std::env::var("CHILPA_LAB_PLEX_SERVER").ok();
        let choices = &chilpa.server_setup().choices;
        let server = choices
            .iter()
            .find(|(_, name, _)| wanted.as_ref().is_none_or(|w| w == name))
            .unwrap_or_else(|| panic!("no server named {wanted:?} in {choices:?}"))
            .0
            .clone();
        chilpa.setup_pick_server(&server);
        step(&mut chilpa, "libraries");
    }
    // Only a music section is ever offered.
    let chosen = std::env::var("CHILPA_LAB_PLEX_SECTION").ok();
    let all: Vec<String> = chilpa
        .server_setup()
        .choices
        .iter()
        .filter(|(_, name, _)| chosen.as_ref().is_none_or(|c| c == name))
        .map(|(id, _, _)| id.clone())
        .collect();
    assert!(!all.is_empty());
    assert!(chilpa.setup_finish(&all));
    let row = synced(&mut chilpa);
    eprintln!("synced: {} tracks, {} records, {}", row.tracks, row.albums, row.state);
    plays_a_track(&mut chilpa, &dir);
}
