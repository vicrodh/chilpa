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
      *'"method":"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocol":1,"capabilities":{"home":true,"library":["albums"]}}}\n' "$id" ;;
      *'"method":"home"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"shelves":[{"id":"n","title":"New","kind":"album","items":[{"kind":"album","ref":"album:1","title":"Record","artist":"Band"}]}]}}\n' "$id" ;;
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
        "api_version": 1, "kind": "source", "runtime": "process", "capabilities": [{"type":"register_source"}]}"#).unwrap();
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

    let _ = std::fs::remove_dir_all(dir);
}
