//! Buffers preserve the live queue while another list is prepared.
use chilpa_api::{Chilpa, TrackEntry};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};

/// Both tests point the XDG directories somewhere of their own; the
/// environment is one per process, so they take turns.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn state(api: &Chilpa) -> Value { serde_json::from_str(&api.listen_json()).unwrap() }
fn keys(api: &mut Chilpa, id: u64) -> Vec<String> {
    api.listen_rows(id, "", true).1.into_iter().filter(|k| !k.is_empty()).collect()
}
fn wave(path: &Path) {
    let data = 44_100u32 * 4 * 30;
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF"); bytes.extend((36 + data).to_le_bytes()); bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes()); bytes.extend(1u16.to_le_bytes()); bytes.extend(2u16.to_le_bytes());
    bytes.extend(44_100u32.to_le_bytes()); bytes.extend(176_400u32.to_le_bytes());
    bytes.extend(4u16.to_le_bytes()); bytes.extend(16u16.to_le_bytes()); bytes.extend(b"data");
    bytes.extend(data.to_le_bytes()); bytes.resize(bytes.len() + data as usize, 0);
    std::fs::write(path, bytes).unwrap();
}
fn wait(api: &mut Chilpa, predicate: impl Fn(&Chilpa) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        api.poll();
        if predicate(api) { return; }
        assert!(Instant::now() < end, "timed out: {}", api.listen_json());
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn buffers_keep_occurrences_and_only_the_activated_one_reaches_the_player() {
    let _env = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = std::env::temp_dir().join(format!("chilpa-listen-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
    let paths: Vec<_> = ["a.wav", "b.wav", "c.wav"].into_iter().map(|name| {
        let path = dir.join(name); wave(&path); path.to_string_lossy().into_owned()
    }).collect();
    let mut api = Chilpa::new();
    // A single queue still offers creating another; unavailable selections do not.
    assert_eq!(state(&api)["tabs"].as_array().unwrap().len(), 1);
    let available = api.track_queue_menu(chilpa_api::TrackMenuContext { count: 1, ..Default::default() });
    let extra = available.iter().find(|entry| entry["submenu"] == true).unwrap();
    assert_ne!(extra["enabled"], false);
    assert_eq!(extra["children"][0]["act"], "buffer:new");
    assert!(extra["children"].as_array().unwrap().iter().all(|entry| entry["enabled"] != false));
    for count in [1, 3] {
        let unavailable = api.track_queue_menu(chilpa_api::TrackMenuContext { count, unavailable: true, editable: true, ..Default::default() });
        assert!(!unavailable.iter().any(|entry| entry["submenu"] == true));
        assert_eq!(unavailable.len(), 1);
        assert_eq!(unavailable[0]["act"], "remove");
    }
    api.use_output("file:capture", false);
    let a = api.listen_create("Jazz", &[paths[0].clone(), paths[1].clone(), paths[0].clone()]);
    let b = api.listen_create("Tests", &[paths[2].clone()]);
    assert!(api.queue_paths().is_empty(), "preparing tabs does not load the panel");
    let targets = api.listen_targets();
    assert_eq!(targets.len(), 2, "extra queues occupy only one menu entry and a separator");
    assert_eq!(targets[1]["submenu"], true);
    assert_eq!(targets[1]["children"][0]["act"], "buffer:new");
    let original = keys(&mut api, a);
    assert_ne!(original[0], original[2], "duplicate tracks have distinct identities");
    assert!(api.listen_play(a, &original[1]));
    api.set_paused(true);
    wait(&mut api, |api| api.now_playing().path == paths[1] && api.now_playing().total_seconds > 0.0);
    assert_eq!(api.now_playing().index, 1);
    api.listen_select(b);
    api.listen_rename(b, "Other");
    assert_eq!(api.queue_paths().len(), 3);
    assert_eq!(api.now_playing().path, paths[1]);
    assert_eq!(state(&api)["active"], a.to_string());
    assert!(api.listen_row_action(&original[2], "remove", ""));
    assert_eq!(keys(&mut api, a), original[..2]);
    assert!(api.listen_play(b, ""));
    wait(&mut api, |api| state(api)["active"] == b.to_string());
    api.set_paused(true);
    assert_eq!(api.queue_paths(), &[paths[2].clone()]);
    assert!(api.listen_play(a, ""));
    wait(&mut api, |api| state(api)["active"] == a.to_string());
    api.set_paused(true);
    assert_eq!(api.now_playing().index, 1, "the buffer resumes its cursor");
    assert_eq!(keys(&mut api, a), original[..2], "removed rows stay removed on reactivation");
    assert!(api.listen_play(a, &original[0]));
    wait(&mut api, |api| api.now_playing().index == 0);
    api.set_paused(true);
    assert_eq!(keys(&mut api, a), original[..2], "rewinding preserves the sequence");
    let saved = api.listen_save(a).unwrap();
    assert_eq!(api.playlist_rows(saved).len(), 2);
    api.listen_replace(b, &[paths[0].clone(), paths[0].clone()]);
    assert_eq!(api.now_playing().path, paths[0]);
    api.listen_undo();
    assert_eq!(keys(&mut api, b).len(), 1);
    let stale = keys(&mut api, b)[0].clone();
    api.listen_replace(b, &[paths[1].clone()]);
    assert!(!api.listen_row_action(&stale, "play", ""));
    let replacement = keys(&mut api, b)[0].clone();
    api.listen_add(b, &[paths[2].clone()]);
    assert!(api.listen_row_action(&replacement, "remove", ""));
    assert!(api.listen_play(b, ""), "removing a saved cursor leaves the remaining buffer playable");
    wait(&mut api, |api| state(api)["active"] == b.to_string());
    api.set_paused(true);
    assert_eq!(api.now_playing().path, paths[2]);
    assert!(api.listen_play(a, ""));
    wait(&mut api, |api| state(api)["active"] == a.to_string());
    api.set_paused(true);
    api.listen_select(b);
    drop(api);
    let mut restored = Chilpa::new();
    assert_eq!(state(&restored)["selected"], b.to_string());
    assert_eq!(keys(&mut restored, a), original[..2]);
    assert!(restored.now_playing().paused);
    let (rows, _) = restored.listen_rows(b, "c", true);
    assert!(rows.iter().any(|row| matches!(row, TrackEntry::Track(_))));
    restored.listen_close(a);
    assert!(restored.queue_paths().is_empty());
    assert!(!restored.now_playing().playing);
    drop(restored);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_session_comes_back_on_the_track_it_was_closed_on() {
    let _env = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = std::env::temp_dir().join(format!("chilpa-listen-last-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
    let paths: Vec<_> = ["a.wav", "b.wav", "c.wav"].into_iter().map(|name| {
        let path = dir.join(name); wave(&path); path.to_string_lossy().into_owned()
    }).collect();
    let cover = dir.join("cover.jpg");
    std::fs::write(&cover, b"not decoded, only found").unwrap();
    let mut api = Chilpa::new();
    api.use_output("file:capture", false);
    api.play_tracks(&paths, "file:capture", false);
    wait(&mut api, |api| api.now_playing().path == paths[0] && api.now_playing().total_seconds > 0.0);
    api.next_track();
    wait(&mut api, |api| api.now_playing().index == 1);
    api.next_track();
    wait(&mut api, |api| api.now_playing().index == 2);
    api.seek(4.0);
    wait(&mut api, |api| api.now_playing().seconds >= 4.0);
    api.set_paused(true);
    let closed_at = api.now_playing().seconds;
    // Killed, not closed: a process that dies never reaches `Drop`.
    std::mem::forget(api);
    let restored = Chilpa::new();
    let now = restored.now_playing();
    assert_eq!(now.path, paths[2], "the player bar shows the track the session closed on");
    assert_eq!(now.index, 2);
    assert!((now.seconds - closed_at).abs() < 1.0, "the position comes back: {} vs {closed_at}", now.seconds);
    assert_eq!(now.artwork_path, cover.display().to_string(), "the restored track shows its cover before it plays");
    assert_eq!(now.title, "c.wav");
    assert!(now.total_seconds > 29.0, "the bar knows the length before it plays");
    drop(restored);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn closing_every_buffer_starts_a_fresh_queue_name_without_reusing_identity() {
    let _env = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let mut api = Chilpa::new();
    let first = api.listen_selected();
    let second = api.listen_create("", &[]);
    api.listen_close(first);
    api.listen_close(second);
    let fresh = api.listen_selected();
    assert!(fresh > second, "stale track keys must never target the new queue");
    assert_eq!(state(&api)["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(state(&api)["name"], chilpa_i18n::t("Now playing"));
    api.listen_rename(fresh, "My queue");
    assert_eq!(state(&api)["name"], "My queue");
    drop(api);
    let mut api = Chilpa::new();
    assert_eq!(state(&api)["name"], "My queue");
    api.listen_create("", &[]);
    assert_eq!(state(&api)["name"], chilpa_i18n::t_args("Queue {}", &["2"]));
}

#[test]
fn shuffle_starts_at_the_front_and_retains_every_occurrence() {
    let _env = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = std::env::temp_dir().join(format!("chilpa-shuffle-start-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
    let file = dir.join("track.wav"); wave(&file);
    let mut api = Chilpa::new();
    api.use_output("file:capture", false);
    for count in [22, 40] {
        let paths = vec![file.to_string_lossy().into_owned(); count];
        api.play_list(&paths, true);
        api.set_paused(true);
        assert_eq!(api.queue_paths().len(), count);
        let order = api.queue_order();
        assert_eq!(order.len(), count, "a fresh shuffle has no played prefix");
        assert_eq!(order[0], api.now_playing().index);
        let mut sorted = order; sorted.sort_unstable();
        assert_eq!(sorted, (0..count).collect::<Vec<_>>());
    }
    api.stop();
    drop(api);
    let _ = std::fs::remove_dir_all(dir);
}
