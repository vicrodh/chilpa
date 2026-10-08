//! Native search retains full artist names and filters advertised audio quality.
#![cfg(target_os = "linux")]
use chilpa_api::{Chilpa, Found};
use serde_json::Value;
use std::{os::unix::fs::PermissionsExt, time::{Duration, Instant}};

#[test]
fn native_artist_navigation_and_quality_filters() {
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let folder = Chilpa::plugins_dir().join("com.test.search");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("manifest.json"), r#"{"id":"com.test.search","name":"Search fixture","version":"0.1.0","api_version":1,"kind":"source","runtime":"process","capabilities":[{"type":"register_source"},{"type":"register_panel"}]}"#).unwrap();
    std::fs::write(folder.join("plugin"), r#"#!/usr/bin/bash
while IFS= read -r line; do
 id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
 case "$line" in
 *'"method":"initialize"'*) result='{"protocol":1,"capabilities":{"search":true,"pages":true},"name":"Search fixture"}' ;;
 *'"method":"search"'*) result='{"artists":[{"kind":"artist","ref":"artist:wrong","title":"Full"},{"kind":"artist","ref":"artist:right","title":"Full Artist Name"}],"tracks":[{"kind":"track","ref":"track:hi","title":"High","quality":{"codec":"flac","bits":24,"rate":96000}},{"kind":"track","ref":"track:cd","title":"CD","quality":{"codec":"flac","bits":16,"rate":44100}},{"kind":"track","ref":"track:unknown","title":"Unknown"}]}' ;;
 *'"method":"page.get"'*) result='{"header":{"kind":"artist","ref":"artist:right","title":"Full Artist Name"},"shelves":[]}' ;;
 *) result='{}' ;;
 esac
 printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$id" "$result"
done
"#).unwrap();
    std::fs::set_permissions(folder.join("plugin"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut api = Chilpa::new();
    api.load_plugins();
    assert_eq!(api.approve_plugin("com.test.search"), 1);
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        api.poll();
        let page: Value = serde_json::from_str(&api.source_page("com.test.search matching-artist-v3:Full Artist Name")).unwrap();
        if page["header"]["ref"] == "artist:right" {
            assert_eq!(page["header"]["title"], "Full Artist Name");
            assert_eq!(page["artist"], true);
            break;
        }
        assert!(Instant::now() < end, "artist page was not resolved: {page}");
        std::thread::sleep(Duration::from_millis(20));
    }
    for (filter, expected) in [("hi_res", vec!["High"]), ("lossless", vec!["High", "CD"])] {
        api.search_source_catalogs_filtered("music", "com.test.search", filter);
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            api.poll();
            let rows = api.unified_found_rows(&Found::default(), "songs");
            let titles: Vec<_> = rows.iter().filter(|r| r.kind() == 4).map(|r| r.title()).collect();
            if titles == expected { break; }
            assert!(Instant::now() < end, "filter {filter} returned {titles:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
