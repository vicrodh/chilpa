//! A server of mine, added, synced and played through the facade, against a
//! real server (ADR-017). Ignored: each needs a server, named by a variable.
//!
//! ```text
//! CHILPA_LAB_SUBSONIC="http://127.0.0.1:4533 lab lab-password"
//! CHILPA_LAB_JELLYFIN="http://127.0.0.1:8096 lab lab-password"
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
    assert!(said.contains(&row.tracks.to_string()), "{said}");
    assert!(chilpa.remove_server(row.id));
    assert!(chilpa.servers().is_empty());
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
