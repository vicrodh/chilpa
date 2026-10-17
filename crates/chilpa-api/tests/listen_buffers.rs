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
fn paths(api: &mut Chilpa, id: u64) -> Vec<String> {
    api.listen_rows(id, "", true).0.into_iter().filter_map(|row| match row { TrackEntry::Track(t) => Some(t.path), _ => None }).collect()
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
    const PHASE: &str = "CHILPA_TEST_LISTEN_SESSION_PHASE";
    const ROOT: &str = "CHILPA_TEST_LISTEN_SESSION_ROOT";
    if let Ok(phase) = std::env::var(PHASE) {
        let dir = std::path::PathBuf::from(std::env::var_os(ROOT).unwrap());
        let paths: Vec<_> = ["a.wav", "b.wav", "c.wav"].into_iter()
            .map(|name| dir.join(name).to_string_lossy().into_owned()).collect();
        if phase == "record" {
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
            std::fs::write(dir.join("closed-at"), api.now_playing().seconds.to_string()).unwrap();
            // I end the process without Drop, including its background writers.
            std::process::exit(0);
        }
        assert_eq!(phase, "restore");
        let closed_at: f64 = std::fs::read_to_string(dir.join("closed-at")).unwrap().parse().unwrap();
        let restored = Chilpa::new();
        let now = restored.now_playing();
        assert_eq!(now.path, paths[2], "the player bar shows the track the session closed on");
        assert_eq!(now.index, 2);
        assert!((now.seconds - closed_at).abs() < 1.0, "the position comes back: {} vs {closed_at}", now.seconds);
        assert_eq!(now.artwork_path, dir.join("cover.jpg").display().to_string(), "the restored track shows its cover before it plays");
        assert_eq!(now.title, "c.wav");
        assert!(now.total_seconds > 29.0, "the bar knows the length before it plays");
        drop(restored);
        // I wait for process exit before the parent removes this profile.
        std::process::exit(0);
    }
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.wav", "b.wav", "c.wav"] { wave(&dir.path().join(name)); }
    std::fs::write(dir.path().join("cover.jpg"), b"not decoded, only found").unwrap();
    for phase in ["record", "restore"] {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "a_session_comes_back_on_the_track_it_was_closed_on", "--nocapture"])
            .envs(chilpa_dirs::profile_environment(dir.path()))
            .env(PHASE, phase).env(ROOT, dir.path()).status().unwrap();
        assert!(status.success(), "session {phase} child failed: {status}");
    }
    std::fs::remove_dir_all(dir.path()).unwrap();
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
        for _ in 0..8 {
            let current = api.now_playing().index;
            api.set_shuffle(true);
            let order = api.queue_order();
            assert_eq!(order.len(), count, "toggling shuffle must not create a played prefix");
            assert_eq!(order[0], current);
            let id = state(&api)["active"].as_str().unwrap().parse().unwrap();
            let sidebar = api.listen_rows(id, "", false).1;
            let full = keys(&mut api, id);
            assert_eq!(sidebar, full, "both queue views keep the current occurrence first");
            assert!(api.listen_is_playing(&full[0]));
        }
    }
    api.stop();
    drop(api);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn playing_with_no_queue_playing_uses_the_queue_in_view_instead_of_a_new_one() {
    let _env = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let paths: Vec<_> = ["a.wav", "b.wav"].into_iter().map(|name| {
        let path = dir.path().join(name); wave(&path); path.to_string_lossy().into_owned()
    }).collect();
    let mut api = Chilpa::new();
    api.use_output("file:capture", false);
    api.play_tracks(&paths[..1], "file:capture", false);
    api.set_paused(true);
    let only = api.listen_selected();
    // Clearing the playing queue leaves nothing playing, and a session can
    // close that way.
    api.listen_command("clear", &only.to_string(), "");
    assert_eq!(state(&api)["active"], "0");
    drop(api);
    let mut api = Chilpa::new();
    api.use_output("file:capture", false);
    api.play_tracks(&paths[1..], "file:capture", false);
    api.set_paused(true);
    assert_eq!(state(&api)["tabs"].as_array().unwrap().len(), 1, "a queue is made only when asked for: {}", api.listen_json());
    assert_eq!(state(&api)["active"], only.to_string());
    assert_eq!(api.queue_paths(), &paths[1..]);
    api.stop();
}

#[test]
fn split_and_merge_keep_playback_order_and_survive_a_new_session() {
    let _env = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let tracks: Vec<_> = ["a.wav", "b.wav", "c.wav"].into_iter().map(|name| {
        let file = dir.path().join(name); wave(&file); file.to_string_lossy().into_owned()
    }).collect();
    let mut api = Chilpa::new(); api.use_output("file:capture", false);
    let a = api.listen_create("First", &[tracks[0].clone(), tracks[1].clone(), tracks[0].clone()]);
    let b = api.listen_create("Second", &[tracks[2].clone()]);
    let original = keys(&mut api, a);
    assert!(api.listen_split(&original[0]).is_none(), "both halves must contain tracks");
    assert!(api.listen_play(a, &original[1]));
    wait(&mut api, |api| api.now_playing().path == tracks[1] && api.now_playing().total_seconds > 0.0);
    api.seek(3.0); wait(&mut api, |api| api.now_playing().seconds >= 3.0); api.set_paused(true);
    let before = api.now_playing().seconds;
    let tail = api.listen_split(&original[1]).unwrap();
    assert_eq!(paths(&mut api, a), tracks[..1]);
    assert_eq!(paths(&mut api, tail), [tracks[1].clone(), tracks[0].clone()]);
    assert_eq!(state(&api)["active"], tail.to_string());
    assert_eq!(api.now_playing().path, tracks[1]); assert!(api.now_playing().paused);
    assert_eq!(api.now_playing().seconds, before);
    assert!(!api.listen_row_action(&original[1], "play", ""), "the old occurrence cannot address the other half");
    let combined = api.listen_merge(&[b, tail, a], "Together", true).unwrap();
    assert_eq!(paths(&mut api, combined), [tracks[2].clone(), tracks[1].clone(), tracks[0].clone(), tracks[0].clone()]);
    assert_eq!(state(&api)["active"], tail.to_string());
    api.listen_undo(); assert!(!state(&api)["tabs"].as_array().unwrap().iter().any(|row| row["id"] == combined.to_string()));
    assert_eq!(api.now_playing().path, tracks[1]);
    let merged = api.listen_merge(&[b, tail], "Merged", false).unwrap();
    assert_eq!(merged, tail, "the decoder retains its queue identity when originals are retired");
    assert_eq!(paths(&mut api, merged), [tracks[2].clone(), tracks[1].clone(), tracks[0].clone()]);
    assert_eq!(api.now_playing().path, tracks[1]); assert_eq!(api.now_playing().seconds, before);
    assert_eq!(state(&api)["tabs"].as_array().unwrap().len(), 3);
    api.listen_step(1); assert_ne!(api.listen_selected(), merged);
    api.listen_step(-1); assert_eq!(api.listen_selected(), merged);
    assert!(api.listen_merge(&[merged, merged], "", false).is_err());
    assert!(api.listen_merge(&[merged, 9999], "", false).is_err());
    drop(api);
    let mut restored = Chilpa::new();
    assert_eq!(paths(&mut restored, merged), [tracks[2].clone(), tracks[1].clone(), tracks[0].clone()]);
    assert_eq!(restored.now_playing().path, tracks[1]); assert!(restored.now_playing().paused);
    assert_eq!(restored.listen_selected(), merged);
    let remaining = keys(&mut restored, merged);
    let tail = restored.listen_split(&remaining[2]).unwrap();
    assert_eq!(state(&restored)["active"], merged.to_string());
    assert_eq!(restored.now_playing().path, tracks[1]);
    assert_eq!(paths(&mut restored, merged), [tracks[2].clone(), tracks[1].clone()]);
    assert_eq!(paths(&mut restored, tail), tracks[..1]);
}

#[test]
fn undo_restores_both_halves_and_retired_queues_in_their_tab_order() {
    let _env = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] { std::env::set_var(key, dir.path().join(folder)); }
    let mut api = Chilpa::new();
    let a = api.listen_create("First", &["a".into(), "b".into(), "a".into()]);
    let b = api.listen_create("Second", &["c".into()]);
    let order = state(&api)["tabs"].clone();
    let old = keys(&mut api, a);
    api.listen_split(&old[2]).unwrap(); api.listen_undo();
    assert_eq!(state(&api)["tabs"], order); assert_eq!(keys(&mut api, a), old);
    let merged = api.listen_merge(&[b, a], "", false).unwrap();
    assert_eq!(paths(&mut api, merged), ["c", "a", "b", "a"]);
    api.listen_undo(); assert_eq!(state(&api)["tabs"], order);
    assert_eq!(keys(&mut api, a), old); assert_eq!(paths(&mut api, b), ["c"]);
}

#[test]
fn language_selection_is_saved_and_available_in_appearance() {
    let _env = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] { std::env::set_var(key, dir.path().join(folder)); }
    let mut api = Chilpa::new();
    api.choose_language("es"); assert_eq!(api.language(), "es"); drop(api);
    let mut restored = Chilpa::new(); assert_eq!(restored.language(), "es");
    let row = restored.settings_rows("appearance", &Default::default()).0.into_iter().find(|row| row.key == "appearance.language").unwrap();
    assert_eq!(row.value, "es"); assert_eq!(row.values, ["", "en", "es"]);
    restored.choose_language("unknown"); assert_eq!(restored.language(), "es");
    restored.choose_language("");
    assert_eq!(restored.settings_rows("appearance", &Default::default()).0.into_iter().find(|row| row.key == "appearance.language").unwrap().value, "");
    restored.choose_language("en");
}

#[test]
fn the_decoder_skips_the_split_tail_and_walks_the_combined_order() {
    let _env = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] { std::env::set_var(key, dir.path().join(folder)); }
    let tracks: Vec<_> = ["a.wav", "b.wav", "c.wav"].into_iter().map(|name| {
        let file = dir.path().join(name); wave(&file); file.to_string_lossy().into_owned()
    }).collect();
    let mut api = Chilpa::new(); api.use_output("file:capture", false);
    api.play_tracks(&tracks, "file:capture", false); api.set_paused(true);
    wait(&mut api, |api| api.now_playing().path == tracks[0] && api.now_playing().total_seconds > 0.0);
    let original = api.listen_selected();
    let keys = keys(&mut api, original);
    let tail = api.listen_split(&keys[1]).unwrap();
    assert_eq!(api.now_playing().path, tracks[0]);
    api.next_track(); wait(&mut api, |api| !api.now_playing().playing);
    assert!(api.listen_play(tail, ""));
    wait(&mut api, |api| api.now_playing().path == tracks[1] && api.now_playing().total_seconds > 0.0); api.set_paused(true);
    assert_eq!(api.listen_merge(&[tail, original], "Joined", false).unwrap(), tail);
    api.next_track(); wait(&mut api, |api| api.now_playing().path == tracks[2] && api.now_playing().total_seconds > 0.0); api.set_paused(true);
    api.next_track(); wait(&mut api, |api| api.now_playing().path == tracks[0] && api.now_playing().total_seconds > 0.0); api.set_paused(true);
    assert_eq!(api.now_playing().index, 2, "the added occurrence reaches the existing decoder queue");
    api.stop();
}
