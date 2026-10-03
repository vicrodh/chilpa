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
      *'"method":"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocol":1,"capabilities":{"auth":"browser","home":true,"library":["albums","playlists"]}}}\n' "$id" ;;
      *'"method":"home"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"shelves":[{"id":"n","title":"New","kind":"album","items":[{"kind":"album","ref":"album:1","title":"Record","artist":"Band"}]}]}}\n' "$id" ;;
      *'"method":"library.list"'*)
        if [[ "$line" == *'"kind":"playlists"'* ]]; then
          printf '{"jsonrpc":"2.0","id":%s,"result":{"items":[{"kind":"playlist","ref":"playlist:1","title":"Cloud list","tracks":8}],"total":1}}\n' "$id"
        else printf '{"jsonrpc":"2.0","id":%s,"result":{"items":[{"kind":"album","ref":"album:1","title":"Record","artist":"Band"}],"total":1}}\n' "$id"; fi ;;
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
    assert_eq!(lists["playlists"][0]["name"], "Cloud list");
    assert_eq!(lists["playlists"][0]["track_count"], 8);
    assert_eq!(lists["playlists"][0]["separate"], false);
    chilpa.plugin_center_command("integration", r#"{"id":"com.test.native","field":"separate_playlists","enabled":true}"#);
    assert_eq!(wait_center(&mut chilpa)["error"], "");
    let lists: serde_json::Value = serde_json::from_str(&chilpa.sidebar_playlists("")).unwrap();
    assert_eq!(lists["playlists"][0]["separate"], true);
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
