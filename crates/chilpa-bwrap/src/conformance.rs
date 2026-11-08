//! What every Linux process backend owes a plugin, checked the same way
//! against each: it is told its folders before and in `initialize` and can
//! write there, its package is read-only, a file beside its folders that it
//! was not handed is out of reach, an export folder is there only once
//! approved, it has a network only when given one, and stopping it leaves no
//! process behind.

use crate::{NativeLauncher, NativePaths, Notice};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MEMORY: usize = 512 * 1024 * 1024;

/// Probes, from inside the plugin's sandbox, what each call names.
const PROBE: &str = r#"#!/usr/bin/bash
field() { printf '%s' "$line" | sed -E "s/.*\"$1\":\"([^\"]*)\".*/\1/"; }
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
  data=$(field data_dir); cache=$(field cache_dir); outside=$(field outside); export=$(field export); withheld=$(field withheld)
  told=false
  [ "$CHILPA_PLUGIN_DATA_DIR" = "$data" ] && [ "$CHILPA_PLUGIN_CACHE_DIR" = "$cache" ] && [ "$HOME" = "$data" ] \
    && [ "$XDG_CACHE_HOME" = "$cache" ] && told=true
  wrote=false; touch "$data/written" "$cache/written" 2>/dev/null && wrote=true
  sealed=true; touch "$(dirname "$0")/written" 2>/dev/null && sealed=false
  hidden=true; cat "$outside" >/dev/null 2>&1 && hidden=false
  closed=true; [ -n "$withheld" ] && touch "$withheld/written" 2>/dev/null && closed=false
  exported=false; [ -n "$export" ] && touch "$export/written" 2>/dev/null && exported=true
  network=$(readlink /proc/self/ns/net)
  printf '{"jsonrpc":"2.0","id":%s,"result":{"told":%s,"wrote":%s,"sealed":%s,"hidden":%s,"closed":%s,"exported":%s,"network":"%s"}}\n' \
    "$id" "$told" "$wrote" "$sealed" "$hidden" "$closed" "$exported" "$network"
done
"#;

fn quiet() -> Notice {
    Box::new(|_: &str, _: &Value| {})
}

/// Runs the probe under `launcher` in folders made under `root`, which must
/// be a place the backend can hand over. `export_guest` is where the backend
/// shows an approved export folder.
pub(crate) fn a_native_plugin_sees_only_its_folders(launcher: &dyn NativeLauncher, root: &Path, export_guest: &dyn Fn(&Path) -> PathBuf) {
    let folder = root.join("plugin");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("plugin"), PROBE).unwrap();
    std::fs::set_permissions(folder.join("plugin"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let outside = root.join("outside");
    std::fs::write(&outside, "not handed over").unwrap();
    let export = root.join("export");
    std::fs::create_dir(&export).unwrap();
    let mut paths = NativePaths { folder, data: root.join("data"), cache: root.join("cache"), export: None, private: Vec::new() };
    let own_network = json!(std::fs::read_link("/proc/self/ns/net").unwrap().display().to_string());

    let offline = launcher.launch(&paths, "test", MEMORY, false, quiet()).unwrap();
    let (data, cache) = offline.guest_directories.clone();
    let inspected = offline.client.call("inspect", json!({"data_dir": data, "cache_dir": cache, "outside": outside,
        "export": "", "withheld": export}), Duration::from_secs(5)).unwrap();
    assert_eq!(inspected["told"], true, "its environment names the folders initialize names");
    assert_eq!(inspected["wrote"], true, "its data and cache folders are writable");
    assert_eq!(inspected["sealed"], true, "its package is read-only");
    assert_eq!(inspected["hidden"], true, "a file beside its folders, not handed over, is out of reach");
    assert_eq!(inspected["closed"], true, "an export folder not approved is out of reach");
    assert_ne!(inspected["network"], own_network, "offline, it has a network of its own");
    assert!(paths.data.join("written").is_file() && paths.cache.join("written").is_file(), "it wrote in the folders Chilpa gave it");
    assert!(!paths.folder.join("written").exists() && !export.join("written").exists());
    offline.client.stop();
    assert!(!offline.process.alive(), "stopping it left no process behind");
    assert!(!offline.client.alive());

    paths.export = Some(export.clone());
    let online = launcher.launch(&paths, "test", MEMORY, true, quiet()).unwrap();
    let (data, cache) = online.guest_directories.clone();
    let inspected = online.client.call("inspect", json!({"data_dir": data, "cache_dir": cache, "outside": outside,
        "export": export_guest(&export), "withheld": ""}), Duration::from_secs(5)).unwrap();
    assert_eq!(inspected["exported"], true, "an approved export folder is writable");
    assert!(export.join("written").is_file());
    assert_eq!(inspected["network"], own_network, "online, it shares Chilpa's network");
    online.client.stop();
    assert!(!online.process.alive());
}
