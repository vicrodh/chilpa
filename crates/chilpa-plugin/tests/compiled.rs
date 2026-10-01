//! The host against a plugin compiled from Rust.
//!
//! The hostile suite writes its plugins in wasm text, which proves the
//! sandbox holds but not that a real toolchain produces something the host
//! can load. This one loads `fixtures/fixture.wasm`, built with the SDK, and
//! talks to it the way the facade does.

use chilpa_plugin::messages::{self, op, Browse, EntryKind, Listing, Query, Resolve, Results, Stream, MAX_PER_REPLY};
use chilpa_plugin::{Host, Limits, Manifest, Plugin, PluginError, Reply, Services, TestHost};
use std::sync::{Arc, Mutex};

const WASM: &[u8] = include_bytes!("fixtures/fixture.wasm");
const MANIFEST: &str = include_str!("fixtures/manifest.json");

fn loaded() -> (Host, Plugin, Services, TestHost) {
    let host = Host::new().expect("the engine starts");
    let plugin = host.load(Manifest::parse(MANIFEST).expect("the manifest reads"), WASM).expect("it loads");
    let notes = TestHost::new();
    let services: Services = Arc::new(Mutex::new(notes.clone()));
    (host, plugin, services, notes)
}

fn ask<T: for<'de> serde::Deserialize<'de>>(
    host: &Host,
    plugin: &Plugin,
    services: &Services,
    operation: &str,
    input: &impl serde::Serialize,
) -> Result<T, String> {
    match plugin.call(host, services, operation, &messages::write(input), Limits::default()) {
        Ok(Reply::Ok(bytes)) => messages::read(&bytes),
        Ok(Reply::Error(e)) => Err(e),
        Err(e) => Err(e.to_string()),
    }
}

fn browse(host: &Host, plugin: &Plugin, services: &Services, ask_for: Browse) -> Listing {
    let cursor = ask_for.cursor.clone();
    ask::<Listing>(host, plugin, services, op::BROWSE, &ask_for)
        .expect("the listing arrives")
        .clean(cursor.as_deref())
}

#[test]
fn a_plugin_built_from_rust_loads_and_searches() {
    let started = std::time::Instant::now();
    let (host, plugin, services, _) = loaded();
    let loading = started.elapsed();

    let started = std::time::Instant::now();
    let found: Results = ask(&host, &plugin, &services, op::SEARCH, &Query::new("second", 10)).unwrap();
    let calling = started.elapsed();
    assert_eq!(found.tracks.len(), 1);
    assert_eq!(found.tracks[0].external_id, "file:two");

    // Printed for whoever is measuring; the bound only catches a collapse.
    println!("fixture: {} bytes, loaded in {loading:?}, one call in {calling:?}", WASM.len());
    assert!(calling < std::time::Duration::from_secs(2), "a call took {calling:?}");
}

#[test]
fn it_is_browsed_two_levels_down() {
    let (host, plugin, services, _) = loaded();
    let root = browse(&host, &plugin, &services, Browse::root());
    assert_eq!(root.entries[0].kind, EntryKind::Search);
    let genres = root.entries.iter().find(|e| e.id == "genres").expect("the root has the folder");
    assert_eq!(genres.kind, EntryKind::Folder);

    let genres = browse(&host, &plugin, &services, Browse::at("genres"));
    assert_eq!(genres.title, "Genres");
    let jazz = browse(&host, &plugin, &services, Browse::at(&genres.entries[0].id));
    assert_eq!(jazz.entries.len(), 2);
    assert!(jazz.entries.iter().all(|e| e.kind == EntryKind::Item && e.live && e.playable));

    let hls = root.entries.iter().find(|e| e.id == "hls:one").unwrap();
    assert!(!hls.playable && !hls.why_not.is_empty(), "what cannot play says why");
}

#[test]
fn a_long_folder_is_walked_by_cursor_and_a_typed_text_is_searched() {
    let (host, plugin, services, _) = loaded();
    let mut cursor = None;
    let mut seen = 0;
    for _ in 0..10 {
        let page = browse(&host, &plugin, &services, Browse { cursor: cursor.clone(), ..Browse::at("long") });
        seen += page.entries.len();
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen, 6);

    let typed = Browse { text: Some("two".into()), ..Browse::at("search") };
    let found = browse(&host, &plugin, &services, typed);
    assert_eq!(found.entries.len(), 1);
    assert_eq!(found.entries[0].id, "live:two");
}

#[test]
fn a_listing_that_misbehaves_is_cleaned_on_this_side() {
    let (host, plugin, services, _) = loaded();
    let huge = browse(&host, &plugin, &services, Browse::at("hostile:huge"));
    assert_eq!(huge.entries.len(), MAX_PER_REPLY, "far more came and only what fits stays");

    let blank = browse(&host, &plugin, &services, Browse::at("hostile:blank"));
    assert_eq!(blank.entries.len(), 1, "no id or no title is not an entry");

    // A `next` that never changes would send the window round the same page
    // forever. The first page gives it; asking with it gives it back, and
    // there it is dropped.
    let first = browse(&host, &plugin, &services, Browse::at("hostile:loop"));
    assert_eq!(first.next.as_deref(), Some("same"));
    let again = browse(&host, &plugin, &services, Browse { cursor: first.next, ..Browse::at("hostile:loop") });
    assert_eq!(again.next, None);
}

#[test]
fn resolving_goes_out_through_the_host_and_uses_its_clock_and_store() {
    let (host, plugin, services, notes) = loaded();
    notes.prepare(|n| {
        n.clock = 1_000;
        n.http_reply = br#"{"status":200,"headers":{},"body":"https://fixture.test/one.flac\n"}"#.to_vec();
    });
    let stream: Stream = ask(&host, &plugin, &services, op::RESOLVER, &Resolve::new("file:one")).unwrap();
    assert_eq!(stream.url, "https://fixture.test/one.flac");
    assert_eq!(stream.expires_at, Some(1_060));
    notes.examine(|n| {
        assert_eq!(n.requests.len(), 1);
        assert!(n.requests[0].1.contains("https://fixture.test/where/one"), "{}", n.requests[0].1);
        assert_eq!(n.writes.len(), 1, "it noted in its store that it resolved one");
    });

    let live: Stream = ask(&host, &plugin, &services, op::RESOLVER, &Resolve::new("live:one")).unwrap();
    assert!(live.usable() && live.expires_at.is_none());

    let refused = ask::<Stream>(&host, &plugin, &services, op::RESOLVER, &Resolve::new("nothing"));
    assert!(refused.unwrap_err().contains("nothing"), "its own error arrives as words");
}

#[test]
fn an_operation_it_does_not_export_is_said_and_is_not_a_crash() {
    let (host, plugin, services, _) = loaded();
    let answer = plugin.call(&host, &services, op::PLAYLISTS, b"{}", Limits::default());
    assert_eq!(answer, Err(PluginError::MissingExport(op::PLAYLISTS.to_string())));
}

#[test]
fn being_asked_for_what_it_does_not_do_never_sends_it_to_quarantine() {
    let host = Host::new().unwrap();
    let services: Services = Arc::new(Mutex::new(TestHost::new()));
    let mut registry = chilpa_plugin::Registry::new();
    assert!(registry.install(&host, MANIFEST, WASM, |_| true));
    let id = "app.chilpa.fixture";
    // Chilpa asks every source for its playlists, again and again.
    for _ in 0..5 {
        let answer = registry.call(&host, &services, id, op::PLAYLISTS, b"{}", Limits::default());
        assert_eq!(answer, Err(PluginError::MissingExport(op::PLAYLISTS.to_string())));
    }
    assert_eq!(registry.state(id), chilpa_plugin::State::Healthy);
    let found = registry.call(&host, &services, id, op::SEARCH, &messages::write(&Query::new("first", 5)), Limits::default());
    assert!(matches!(found, Ok(Reply::Ok(_))), "and it still answers what it does do: {found:?}");
}

#[test]
fn a_module_missing_a_required_export_does_not_load() {
    let host = Host::new().unwrap();
    let bare = wat::parse_str(r#"(module (memory (export "memory") 1))"#).unwrap();
    let refused = host.load(Manifest::parse(MANIFEST).unwrap(), &bare);
    assert!(matches!(refused, Err(PluginError::MissingExport(_))));
}
