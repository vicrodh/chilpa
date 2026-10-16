//! Searching with a real plugin dropped on disk.
//!
//! The `merge` tests live in the module and touch nothing; this one checks
//! the other half: that the query goes out on its thread, that what the
//! plugin answers arrives through `poll`, and that a song already in the
//! library is not shown twice but with its "also in".
//!
//! It is a single test because it points `XDG_DATA_HOME` at a temporary
//! directory, and that variable belongs to the whole process.

use std::path::{Path, PathBuf};

/// The ISRC shared by the local track and the one the plugin answers with.
const ISRC: &str = "USRC17607839";

/// A catalogue plugin that always answers the same thing.
///
/// `reply` comes with its real quotes: here it is escaped to go into the
/// wasm text and the length is taken from the original, not from the escaped
/// text. Counting by hand is how a test passes for the wrong reason.
fn source_plugin(dir: &Path, id: &str, reply: &str) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(
        dir.join(id).join("manifest.json"),
        format!(
            r#"{{
                "id": "{id}",
                "name": "Source {id}",
                "version": "0.1.0",
                "api_version": 1,
                "kind": "source",
                "capabilities": []
            }}"#
        ),
    )
    .unwrap();
    let len = reply.len();
    // A wasm string carries no raw newlines; the escape is still one byte,
    // so the length does not change.
    let escaped = reply.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
    let wat = format!(
        r#"(module
            (memory (export "memory") 1)
            (global $next (mut i32) (i32.const 4096))
            (func (export "chilpa_alloc") (param i32) (result i32)
              (local i32)
              (local.set 1 (global.get $next))
              (global.set $next (i32.add (global.get $next) (local.get 0)))
              (local.get 1))
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (data (i32.const 8) "{escaped}")
            ;; It answers the same thing without looking at the query: what is
            ;; tested here is the path, not the search itself.
            (func (export "chilpa_search") (param i32 i32) (result i64)
              (i64.or
                (i64.shl (i64.const 8) (i64.const 32))
                (i64.const {len}))))"#
    );
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(&wat).expect("it compiles"))
        .unwrap();
}

/// A tiny WAV with its ISRC put in afterwards, straight into the database.
fn wave(dir: &Path, name: &str) -> PathBuf {
    use std::io::Write;
    let p = dir.join(name);
    let frames = 4_410u32;
    let data = frames * 4;
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&44_100u32.to_le_bytes()).unwrap();
    f.write_all(&176_400u32.to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    for i in 0..frames {
        let v = (i % 3_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    p
}

#[test]
fn a_source_plugin_answers_the_search_and_does_not_duplicate_what_is_local() {
    let dir = std::env::temp_dir().join("chilpa-api-search");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));

    // A library with one track, and its ISRC noted the way a scan of a
    // file that does carry one would leave it.
    let music = dir.join("music");
    std::fs::create_dir_all(&music).unwrap();
    wave(&music, "Suzie Q.wav");
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.scan(&music);
    let local = chilpa.search("Suzie", 20);
    assert_eq!(local.len(), 1, "the track was scanned");

    // The scan leaves the ISRC when the file carries one; this WAV carries
    // no tags, so it is put in by hand in the same database.
    rusqlite::Connection::open(chilpa_library::Library::default_path())
        .unwrap()
        .execute("UPDATE tracks SET isrc = ?1", rusqlite::params![ISRC])
        .unwrap();

    // Two sources: one brings the same song (same ISRC) and another that
    // is not on disk; the other falls over.
    let plugins = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&plugins).unwrap();
    source_plugin(
        &plugins,
        "com.test.good",
        &format!(
            r#"{{"tracks":[
                 {{"external_id":"a1","title":"Suzie Q","artist":"CCR","isrc":"{ISRC}",
                   "quality":{{"codec":"flac","bit_depth":24,"sample_rate_hz":96000,
                               "channels":2}}}},
                 {{"external_id":"a2","title":"Another I do not have","artist":"CCR"}}]}}"#
        ),
    );
    source_plugin(&plugins, "com.test.broken", "this is not json");

    assert_eq!(chilpa.load_plugins(), 0, "nobody searches without approval (ADR-008)");
    chilpa.approve_plugin("com.test.good");
    assert_eq!(chilpa.approve_plugin("com.test.broken"), 2, "both loaded");

    // What is local comes out at once, waiting for nobody.
    let immediate = chilpa.search_all("Suzie", 20);
    assert_eq!(immediate.len(), 1, "what is local does not wait for the plugins");
    assert_eq!(immediate[0].source, "local");
    assert!(immediate[0].playable, "what is on disk can be played");

    // And what the sources give arrives through poll, once the typing rest
    // is over. It is watched the way the window watches it: only when a
    // change is announced. A query can start and finish between two ticks,
    // and even so the last source to answer has to end up drawn.
    let mut hits = Vec::new();
    let arrived = wait_for(&mut chilpa, |c| {
        if c.search_changed() {
            hits = c.found().to_vec();
        }
        hits.len() >= 2
    });
    assert!(arrived, "the sources answered: {hits:?}");
    assert!(!chilpa.search_running(), "and nobody is missing any more");
    assert_eq!(hits.len(), 2, "the same song is not shown twice: {hits:?}");

    let suzie = &hits[0];
    assert_eq!(suzie.source, "local", "what is local keeps its place");
    assert_eq!(
        suzie.also_in,
        vec!["Source com.test.good"],
        "and it says which other source holds it"
    );

    let other = &hits[1];
    assert_eq!(other.source, "com.test.good");
    assert_eq!(other.title, "Another I do not have");
    assert!(other.playable, "the source can be asked to resolve it");
    assert_eq!(other.quality, "", "that one declared no quality and none is invented for it");

    // The one that answered badly is named, and it took nobody with it.
    let problems = chilpa.search_problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("Source com.test.broken"), "{problems:?}");

    // A dedicated source search must retain its own recording even when it
    // has the same fingerprint as a local one, and never query its neighbor.
    assert!(chilpa.search_in("Suzie", 20, "com.test.good").is_empty());
    assert!(wait_for(&mut chilpa, |c| c.found().len() == 2));
    assert!(chilpa.found().iter().all(|hit| hit.source == "com.test.good"));
    assert!(chilpa.search_problems().is_empty());
    assert_eq!(chilpa.search_in("Suzie", 20, "local").len(), 1);
    let until = std::time::Instant::now() + chilpa_api::SOURCE_COOLDOWN + chilpa_api::KEYSTROKE_DEBOUNCE;
    while std::time::Instant::now() < until {
        chilpa.poll();
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert_eq!(chilpa.found().len(), 1);
    assert_eq!(chilpa.found()[0].source, "local");
    assert!(chilpa.search_problems().is_empty());
    assert!(!chilpa.search_running());

    // Searching for something else throws away the previous query's results at once.
    let other_search = chilpa.search_all("nothing like that exists", 20);
    assert!(other_search.is_empty());
    assert!(chilpa.found().is_empty(), "no results from the old search stay behind");
    assert!(chilpa.search_problems().is_empty(), "nor its problems");

    let _ = std::fs::remove_dir_all(dir);
}

/// Calls `poll` until the condition holds, or patience runs out.
fn wait_for(
    chilpa: &mut chilpa_api::Chilpa,
    mut cond: impl FnMut(&mut chilpa_api::Chilpa) -> bool,
) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        chilpa.poll();
        if cond(chilpa) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}
