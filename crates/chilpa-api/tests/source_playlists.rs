//! A source's lists, shown next to ours.
//!
//! A list that lives on a source is asked for and not kept: a copy in the
//! library would be a second truth that goes stale without anyone noticing.
//! Opening one puts its tracks in the results, which is where playing one is
//! already a single click down a path that works.
//!
//! It is a single test because it points `XDG_DATA_HOME` at a temporary
//! directory, and that variable belongs to the whole process.

use std::path::Path;

/// A source with two lists, one of which has two tracks.
fn install(dir: &Path, id: &str) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(
        dir.join(id).join("manifest.json"),
        format!(
            r#"{{"id": "{id}", "name": "Test source", "version": "0.1.0",
                 "api_version": 1, "kind": "source", "capabilities": []}}"#
        ),
    )
    .unwrap();

    // A wasm string carries no raw newlines, so the JSON goes on one line.
    let lists = concat!(
        r#"{"playlists":[{"external_id":"p1","name":"Their mix","track_count":2},"#,
        r#"{"external_id":"p2","name":"Another of theirs","track_count":0}]}"#
    );
    let tracks = concat!(
        r#"{"tracks":[{"external_id":"t1","title":"First of the list","artist":"Someone"},"#,
        r#"{"external_id":"t2","title":"Second of the list","artist":"Someone"}]}"#
    );

    // The addresses and the lengths come from here, not from counting by
    // hand: a wrong length truncates the JSON and the test would pass
    // because the reply is unintelligible.
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let wat = format!(
        r#"(module
            (memory (export "memory") 1)
            (global $next (mut i32) (i32.const 16384))
            (func (export "chilpa_alloc") (param i32) (result i32)
              (local i32)
              (local.set 1 (global.get $next))
              (global.set $next (i32.add (global.get $next) (local.get 0)))
              (local.get 1))
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (data (i32.const 16) "{lists_esc}")
            (data (i32.const 4096) "{tracks_esc}")
            (func (export "chilpa_playlists") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 16) (i64.const 32)) (i64.const {lists_len})))
            ;; It does not look at which list it is asked for: it has one.
            (func (export "chilpa_playlist_tracks") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 4096) (i64.const 32)) (i64.const {tracks_len}))))"#,
        lists_esc = esc(lists),
        lists_len = lists.len(),
        tracks_esc = esc(tracks),
        tracks_len = tracks.len(),
    );
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(&wat).expect("it compiles"))
        .unwrap();
}

#[test]
fn a_sources_lists_show_up_and_open_into_the_results() {
    let dir = std::env::temp_dir().join("chilpa-api-source-playlists");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));

    let root = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&root).unwrap();
    install(&root, "com.test.lists");

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.load_plugins();
    assert_eq!(chilpa.approve_plugin("com.test.lists"), 1, "it runs");

    // A fresh library has no lists of its own, so what shows up is the
    // source's, with the source's name in the column the panel filters on.
    let lists = chilpa.playlists();
    assert_eq!(lists.len(), 2, "{lists:?}");
    let mix = lists.iter().find(|p| p.name == "Their mix").expect("it is listed");
    assert_eq!(mix.source, "com.test.lists", "it says which source it came from");
    assert_eq!(mix.provider, "Test source", "and its name, for the panel");
    assert_eq!(mix.external_id, "p1", "with the id to ask for it again");
    assert_eq!(mix.id, 0, "it has no row in the library, because it is not kept");
    assert_eq!(mix.track_count, 2);

    // Opening it puts its tracks in the results. It does not play: a
    // source's track is resolved one at a time, and the queue holds
    // resolved addresses.
    assert_eq!(chilpa.open_source_playlist("com.test.lists", "p1"), 2);
    let hits = chilpa.found().to_vec();
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert_eq!(hits[0].title, "First of the list");
    assert_eq!(hits[0].source, "com.test.lists");
    assert_eq!(hits[0].source_name, "Test source");
    assert!(hits[0].playable, "playing one is the same click as a search result");
    assert!(chilpa.search_changed(), "the view is told to redraw");

    // A source that cannot answer is said with its name and takes nobody
    // with it: the panel keeps working.
    assert_eq!(chilpa.open_source_playlist("com.does.not.exist", "p1"), 0);
    assert!(!chilpa.now_playing().error.is_empty(), "and why is said");

    let _ = std::fs::remove_dir_all(dir);
}
