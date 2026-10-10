//! A process plugin is reached outside the sandbox lock: a slow call to it
//! does not hold a fast one, nor a call to any other plugin.
//!
//! It is a single test because it points `XDG_DATA_HOME` and
//! `XDG_CACHE_HOME` at a temporary directory, and they belong to the whole
//! process.
#![cfg(target_os = "linux")]

use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

/// A plugin in bash: each line is answered on its own subshell, so `slow`
/// (3 s) and `echo` are in flight at the same time.
const PLUGIN: &str = r#"#!/usr/bin/bash
while IFS= read -r line; do
  (
    id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
    case "$line" in
      *'"method":"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocol":1,"capabilities":{"auth":"browser","home":true,"search":true,"track_quality":true,"playlist_edit":true,"navigation":{"discover":"Music","library":"Collection"},"library":["albums","playlists","tracks"]}}}\n' "$id" ;;
      *'"method":"search"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"artists":[{"kind":"artist","ref":"artist:9","title":"Band"}]}}\n' "$id" ;;
      *'"method":"artist.get"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"albums":[{"kind":"album","ref":"album:2","title":"Another record","artist":"Band"}]}}\n' "$id" ;;
      *'"method":"home"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"shelves":[{"id":"n","title":"New","kind":"album","items":[{"kind":"album","ref":"album:1","title":"Record","artist":"Band"}]}]}}\n' "$id" ;;
      *'"method":"library.list"'*)
        if [[ "$line" == *'"kind":"playlists"'* ]]; then
          printf '{"jsonrpc":"2.0","id":%s,"result":{"items":[{"kind":"playlist","ref":"playlist:1","title":"Cloud list","tracks":1,"editable":true,"native_folder":"folder:a"}],"folders":[{"ref":"folder:a","name":"Remote folder","parent":""}],"total":1}}\n' "$id"
        elif [[ "$line" == *'"kind":"tracks"'* ]]; then
          printf '{"jsonrpc":"2.0","id":%s,"result":{"items":[{"kind":"track","ref":"track:7","title":"Remote song","artist":"Band","duration_ms":120000,"quality":{"codec":"flac"}}],"total":1}}\n' "$id"
        else printf '{"jsonrpc":"2.0","id":%s,"result":{"items":[{"kind":"album","ref":"album:1","title":"Record","artist":"Band"}],"total":1}}\n' "$id"; fi ;;
      *'"method":"playlist.get"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"playlist":{"kind":"playlist","ref":"playlist:1","title":"Cloud list","editable":true},"tracks":[{"kind":"track","ref":"track:7","title":"Remote song","artist":"Band","duration_ms":120000}]}}\n' "$id" ;;
      *'"method":"track.quality"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"ref":"track:7","quality":{"codec":"wav","bits":24,"rate":96000}}}\n' "$id" ;;
      *'"method":"page.action"'*)
        if [[ "$line" == *'"action":"playlist.create"'* ]]; then
          printf '{"jsonrpc":"2.0","id":%s,"result":{"item":{"ref":"playlist:new"},"changed_refs":["playlist:new"]}}\n' "$id"
        elif [[ "$line" == *'"action":"playlist.add"'* && "$line" == *'"tracks":["track:7"]'* ]]; then
          printf '{"jsonrpc":"2.0","id":%s,"result":{"changed_refs":["playlist:new"]}}\n' "$id"
        else printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32602,"message":"Foreign tracks"}}\n' "$id"; fi ;;
      *'"method":"settings.open"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"revision":7,"form":{"fields":[{"key":"app_id","label":"App ID","control":{"type":"secret"}}],"actions":[{"id":"sign_in","label":"Sign in"}]},"values":{}}}\n' "$id" ;;
      *'"method":"settings.submit"'*)
        if [[ "$line" == *'"revision":7'* && "$line" == *'"action":"sign_in"'* && "$line" == *'"app_id":"fixture"'* ]]; then
          printf '{"jsonrpc":"2.0","id":%s,"result":{"revision":8,"form":{"fields":[],"actions":[]},"values":{},"links":[{"url":"https://example.org/authorize","label":"Sign in"}]}}\n' "$id"
        else printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32602,"message":"Invalid submission"}}\n' "$id"; fi ;;
      *'"method":"inspect"'*|*'"method":"tick"'*) printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"No compatibility methods"}}\n' "$id" ;;
      *'"method":"slow"'*) sleep 3; printf '{"jsonrpc":"2.0","id":%s,"result":"slow"}\n' "$id" ;;
      *) printf '{"jsonrpc":"2.0","id":%s,"result":"fast"}\n' "$id" ;;
    esac
  ) &
done
wait
"#;

#[test]
fn a_slow_call_to_a_process_plugin_holds_nothing_else() {
    let dir = std::env::temp_dir().join(format!("chilpa-api-native-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
    let library = chilpa_library::Library::open(&chilpa_library::Library::default_path()).unwrap();
    let local_source = library.source_id("local", "default").unwrap();
    library.connection().execute("INSERT INTO albums(id, title, album_artist) VALUES (1, 'Local record', 'Local artist')", []).unwrap();
    library.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, duration_ms, added_at)
        VALUES (1, ?1, '/music/local.flac', 'Local song', 'Local artist', 1, 1000, 1)", [local_source]).unwrap();
    drop(library);

    let folder = chilpa_api::Chilpa::plugins_dir().join("com.test.native");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("manifest.json"), r#"{"id": "com.test.native", "name": "Native", "version": "0.1.0",
        "api_version": 1, "kind": "source", "runtime": "process", "capabilities": [{"type":"register_source"},{"type":"register_panel"}]}"#).unwrap();
    std::fs::write(folder.join("plugin"), PLUGIN).unwrap();
    std::fs::set_permissions(folder.join("plugin"), std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut chilpa = chilpa_api::Chilpa::new();
    assert_eq!(chilpa.load_plugins(), 0, "nothing runs before it is approved");
    assert_eq!(chilpa.approve_plugin("com.test.native"), 1, "approved, it starts and greets");
    assert_eq!(chilpa.plugins()[0].state, "fine");
    let panels = chilpa.source_panels();
    let panel = panels.iter().find(|panel| panel.id == "com.test.native").unwrap();
    assert_eq!(panel.children[0].title, "Music");
    assert_eq!(panel.children[1].title, "Collection");
    assert_eq!(panel.children[0].route, "com.test.native");
    assert_eq!(panel.children[1].route, "com.test.native section:library");

    let caller = chilpa.native_caller();
    let slow = {
        let caller = caller.clone();
        std::thread::spawn(move || caller.call("com.test.native", "slow", json!({}), Duration::from_secs(10)))
    };
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    assert_eq!(chilpa.native_call("com.test.native", "echo", json!({}), Duration::from_secs(2)).unwrap(), json!("fast"));
    assert!(started.elapsed() < Duration::from_millis(500), "the fast call waited {:?}", started.elapsed());
    // While the slow call is in flight, the sandbox lock is free.
    assert_eq!(chilpa.plugins().len(), 1);
    assert!(started.elapsed() < Duration::from_millis(500), "listing waited {:?}", started.elapsed());
    assert_eq!(slow.join().unwrap().unwrap(), json!("slow"));

    // Its home is read in one call into the Discover page, as shelves.
    let deadline = Instant::now() + Duration::from_secs(10);
    let page = loop {
        let page: serde_json::Value = serde_json::from_str(&chilpa.source_page("com.test.native")).unwrap();
        if page["shelves"].as_array().is_some_and(|s| !s.is_empty()) || Instant::now() > deadline {
            break page;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(page["shelves"][0]["title"], "New", "{page}");
    assert_eq!(page["shelves"][0]["key"], "com.test.native route:discover:0");

    let deadline = Instant::now() + Duration::from_secs(10);
    let remote_id = loop {
        let sidebar: serde_json::Value = serde_json::from_str(&chilpa.sidebar_playlists("")).unwrap();
        if let Some(id) = sidebar["playlists"].as_array().unwrap().iter().find(|p| p["name"] == "Cloud list").and_then(|p| p["id"].as_i64()) { break id; }
        assert!(Instant::now() < deadline, "native playlist did not reach the sidebar");
        std::thread::sleep(Duration::from_millis(50));
    };
    let sidebar: serde_json::Value = serde_json::from_str(&chilpa.sidebar_playlists("")).unwrap();
    assert_eq!(sidebar["native_folders"][0]["ref"],"folder:a");
    assert_eq!(sidebar["folder_sources"][0]["id"],"com.test.native");
    assert_eq!(sidebar["playlists"][0]["native_folder"],"folder:a");
    let record_id = chilpa.source_shelf("com.test.native route:discover:0", "", "", "").0[0].id;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let related = chilpa.same_artist_albums(record_id, 8);
        if related.iter().any(|a| a.title == "Another record" && a.id < 0) { break; }
        assert!(Instant::now() < deadline, "same-artist releases did not include the active native source");
        std::thread::sleep(Duration::from_millis(20));
    }
    let playlist_folder = chilpa.create_playlist_folder("Every source", "accent").unwrap();
    assert!(chilpa.move_playlist_to_folder(remote_id, Some(playlist_folder)));
    assert_eq!(chilpa.playlist(remote_id).unwrap().folder_id, Some(playlist_folder));
    assert!(chilpa.managed_playlists("", "all", "name", true, Some(playlist_folder)).iter().any(|p| p.id == remote_id));
    let track = loop {
        if let Some(track) = chilpa.album_track_rows(remote_id).into_iter().next() { break track; }
        assert!(Instant::now() < deadline, "native playlist tracks did not load");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(chilpa_api::track_menu(chilpa_api::TrackMenuContext { count: 1, remote: true, ..Default::default() })
        .iter().any(|entry| entry["act"] == "pick"));
    let mixed = chilpa.create_playlist("Mixed sources").unwrap();
    assert_eq!(chilpa.add_track_ids_to_playlist(mixed, &[track.id]).added, 1);
    let saved = chilpa.playlist_rows(mixed);
    assert_eq!(saved[0].title, "Remote song");
    assert_eq!(saved[0].path, track.path);
    assert!(saved[0].available);
    chilpa.request_track_quality(saved[0].id,&saved[0].path);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let rows = chilpa.album_track_rows(remote_id);
        if rows[0].quality.short() == "WAV 24/96" { break; }
        assert!(Instant::now() < deadline,"technical quality did not arrive without playback");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(chilpa.playlist_rows(mixed)[0].quality.short(),"WAV 24/96");
    assert_eq!(chilpa.album_card_quality(remote_id).short(),"WAV 24/96");
    assert!(chilpa.playlist_targets(&[track.id], "Cloud list")[0].accepts);
    assert!(chilpa.write_source_playlist("com.test.native", 0, "New cloud list", "", &[track.id], "create-test"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state: serde_json::Value = serde_json::from_str(&chilpa.playlist_write_state("com.test.native")).unwrap();
        if state["pending"] == false {
            assert!(state["error"].is_null(), "{state}");
            assert!(state["result"]["warning"].is_null(), "{state}");
            assert_eq!(state["result"]["item"]["ref"], "playlist:new");
            break;
        }
        assert!(Instant::now() < deadline, "playlist creation did not finish");
        std::thread::sleep(Duration::from_millis(20));
    }

    // Settings use the native protocol without compatibility inspection.
    let installed = wait_center(&mut chilpa);
    let row = installed["installed"].as_array().unwrap().iter().find(|row| row["id"] == "com.test.native").unwrap();
    assert_eq!(row["settings"], true);
    assert_eq!(row["native"], true);
    assert!(row["origin"].is_null());
    chilpa.plugin_center_command("settings", r#"{"id":"com.test.native"}"#);
    let opened = wait_center(&mut chilpa);
    assert_eq!(opened["error"], "", "{opened}");
    assert_eq!(opened["form"]["revision"], 7);
    assert_eq!(opened["form"]["form"]["fields"][0]["control"]["type"], "secret");
    chilpa.plugin_center_command("submit", &json!({"id":"com.test.native","submission":{
        "revision":7,"action":"sign_in","values":{"app_id":"fixture"}}}).to_string());
    let submitted = wait_center(&mut chilpa);
    assert_eq!(submitted["error"], "", "{submitted}");
    assert_eq!(submitted["form"]["revision"], 8);
    assert_eq!(submitted["form"]["links"][0]["url"], "https://example.org/authorize");
    chilpa.plugin_center_command("tick", r#"{"id":"com.test.native"}"#);
    assert_eq!(wait_center(&mut chilpa)["error"], "");
    chilpa.plugin_center_command("disable", r#"{"id":"com.test.native"}"#);
    assert_eq!(wait_center(&mut chilpa)["installed"][0]["enabled"], false);
    assert!(!chilpa.same_artist_albums(record_id, 8).iter().any(|a| a.id < 0));
    assert!(!chilpa.playlist_rows(mixed)[0].available);
    assert_eq!(chilpa.playlist_rows(mixed)[0].title, "Remote song");
    assert_eq!(chilpa.playlist(remote_id).unwrap().folder_id, Some(playlist_folder));
    assert!(caller.call("com.test.native", "echo", json!({}), Duration::from_secs(1)).is_err());
    chilpa.plugin_center_command("enable", r#"{"id":"com.test.native"}"#);
    assert!(!wait_center(&mut chilpa)["error"].as_str().unwrap().is_empty(), "a switch alone cannot approve permissions");
    chilpa.plugin_center_command("review_permissions", r#"{"id":"com.test.native"}"#);
    let proposal = wait_center(&mut chilpa)["approval"].clone();
    assert!(!proposal["permissions"].as_array().unwrap().is_empty());
    chilpa.plugin_center_command("enable", &json!({"id":"com.test.native","permission_revision":proposal["revision"]}).to_string());
    assert_eq!(wait_center(&mut chilpa)["error"], "");
    let deadline = Instant::now() + Duration::from_secs(10);
    let lists = loop {
        let value: serde_json::Value = serde_json::from_str(&chilpa.sidebar_playlists("")).unwrap();
        if !value["playlists"].as_array().unwrap().is_empty() { break value; }
        assert!(Instant::now() < deadline, "native playlists did not arrive");
        std::thread::sleep(Duration::from_millis(30));
    };
    let cloud = lists["playlists"].as_array().unwrap().iter().find(|p| p["name"] == "Cloud list").unwrap();
    assert_eq!(cloud["track_count"], 1);
    assert_eq!(cloud["separate"], false);
    let libraries: serde_json::Value = serde_json::from_str(&chilpa.album_libraries()).unwrap();
    assert_eq!(libraries[0]["id"], "com.test.native");
    assert_eq!(libraries[0]["enabled"], false);
    chilpa.sources_command("album_library", "com.test.native", "true");
    assert_eq!(serde_json::from_str::<serde_json::Value>(&chilpa.album_libraries()).unwrap()[0]["enabled"], true);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let albums = chilpa.integrated_albums("", &Default::default());
        if !albums.is_empty() { assert_eq!(albums[0].title, "Record"); break; }
        assert!(Instant::now() < deadline, "saved releases did not arrive");
        std::thread::sleep(Duration::from_millis(30));
    }
    let reader = chilpa.reader().unwrap();
    assert!(reader.tracks("",&Default::default()).snapshot.ids.iter().all(|id| *id > 0));
    chilpa.sources_command("track_library","com.test.native","true");
    assert_eq!(reader.local_tracks("", &Default::default()).snapshot.ids, vec![1],
        "Tracks must be able to show local rows before the source merge finishes");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found = reader.tracks("",&Default::default());
        if found.snapshot.ids.contains(&track.id) {
            assert!(found.snapshot.ids.contains(&1), "source rows must not replace local tracks");
            break;
        }
        assert!(Instant::now() < deadline,"saved tracks did not reach the shared snapshot");
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut stages = Vec::new();
    reader.tracks_progressive("", &Default::default(), |found| stages.push(found.snapshot.ids));
    assert_eq!(stages.len(), 2);
    assert_eq!(stages[0], vec![1]);
    assert!(stages[1].contains(&1) && stages[1].contains(&track.id));
    assert_eq!(reader.paths(&[track.id]),vec![track.path.clone()]);
    let filter = chilpa_api::Filter { excluded_sources:vec!["plugin:com.test.native".into()], ..Default::default() };
    assert!(!reader.tracks("",&filter).snapshot.ids.contains(&track.id));
    chilpa.sources_command("track_library","com.test.native","false");
    assert!(!reader.tracks("",&Default::default()).snapshot.ids.contains(&track.id));
    chilpa.sources_command("album_library", "com.test.native", "false");
    assert!(chilpa.integrated_albums("", &Default::default()).is_empty());
    assert_eq!(serde_json::from_str::<serde_json::Value>(&chilpa.album_libraries()).unwrap()[0]["enabled"], false);
    chilpa.plugin_center_command("integration", r#"{"id":"com.test.native","field":"separate_playlists","enabled":true}"#);
    assert_eq!(wait_center(&mut chilpa)["error"], "");
    let lists: serde_json::Value = serde_json::from_str(&chilpa.sidebar_playlists("")).unwrap();
    assert_eq!(lists["playlists"].as_array().unwrap().iter().find(|p| p["name"] == "Cloud list").unwrap()["separate"], true);
    chilpa.plugin_center_command("tint", r#"{"id":"com.test.native","enabled":true}"#);
    assert_eq!(wait_center(&mut chilpa)["error"], "");
    assert!(chilpa.source_panels().iter().find(|p| p.id == "com.test.native").unwrap().icon_tint);
    let data = chilpa_dirs::data().join("plugin-data/com.test.native");
    let cache = chilpa_dirs::cache().join("plugins/com.test.native");
    std::fs::create_dir_all(&data).unwrap(); std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(data.join("account.json"), b"fixture").unwrap();
    std::fs::write(cache.join("catalog.json"), b"fixture").unwrap();
    chilpa.remember_preference("ui.source.com.test.native section:albums.order", "title");
    chilpa.plugin_center_command("uninstall", r#"{"id":"com.test.native"}"#);
    let removed = wait_center(&mut chilpa);
    assert_eq!(removed["error"], "", "{removed}");
    assert!(removed["installed"].as_array().unwrap().is_empty());
    assert_eq!(chilpa.playlist_rows(mixed)[0].title, "Remote song");
    assert!(!chilpa.same_artist_albums(record_id, 8).iter().any(|a| a.id < 0));
    assert!(!chilpa.playlist_rows(mixed)[0].available);
    assert_eq!(chilpa.playlist_folders().into_iter().find(|f| f.id == playlist_folder).unwrap().playlist_count, 0);
    assert!(!folder.exists() && !data.exists() && !cache.exists());
    assert_eq!(chilpa.preference("ui.source.com.test.native section:albums.order"), None);
    assert!(!chilpa_api::Chilpa::plugins_dir().join(".integration/com.test.native.json").exists());
    assert!(caller.call("com.test.native", "echo", json!({}), Duration::from_secs(1)).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

fn wait_center(chilpa: &mut chilpa_api::Chilpa) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let view: serde_json::Value = serde_json::from_str(&chilpa.plugin_center_json()).unwrap();
        if view["working"] == false { return view; }
        assert!(Instant::now() < deadline, "plugin center timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
