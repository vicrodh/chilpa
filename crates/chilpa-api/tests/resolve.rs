//! From a source's result to music coming out the device.
//!
//! It is the whole path: the plugin answers the search, the user clicks,
//! Chilpa asks it for `chilpa_resolver`, the plugin gives a URL, and that
//! URL is opened and played like any other stream.
//!
//! The server is local and a toy: what is tested is the seam, not HTTP. And
//! the plugin **asks for no network permission**, on purpose: resolving does
//! not need the guest to reach the internet, because the one opening the URL
//! is Chilpa.
//!
//! It is a single test because it points `XDG_DATA_HOME` at a temporary
//! directory, and that variable belongs to the whole process.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;

const OUTPUT: &str = "capture";

/// A short WAV with a tone, in memory.
fn wave(frames: u32) -> Vec<u8> {
    let data = frames * 4;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&44_100u32.to_le_bytes());
    w.extend_from_slice(&176_400u32.to_le_bytes());
    w.extend_from_slice(&4u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        let v = (i % 3_000) as i16;
        w.extend_from_slice(&v.to_le_bytes());
        w.extend_from_slice(&v.to_le_bytes());
    }
    w
}

/// Serves the same WAV to whoever asks, once per connection.
fn serve(audio: Vec<u8>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\n\r\n",
                audio.len()
            );
            if stream.write_all(head.as_bytes()).is_ok() {
                let _ = stream.write_all(&audio);
            }
            let _ = stream.flush();
        }
    });
    port
}

/// Serves a WAV cut in pieces, one piece per connection, in order.
///
/// That is what a source that signs a URL per part looks like from here: the
/// chained reader opens the next one only when the current runs out, so the
/// order is the order of the connections.
fn serve_in_parts(audio: Vec<u8>, pieces: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let size = audio.len().div_ceil(pieces);
    let parts: Vec<Vec<u8>> = audio.chunks(size).map(<[u8]>::to_vec).collect();
    std::thread::spawn(move || {
        for (stream, part) in listener.incoming().flatten().zip(parts) {
            let mut stream = stream;
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\n\r\n",
                part.len()
            );
            if stream.write_all(head.as_bytes()).is_ok() {
                let _ = stream.write_all(&part);
            }
            let _ = stream.flush();
        }
    });
    port
}

/// A source that finds a track and can say what it is played with.
fn install(dir: &Path, id: &str, found: &str, stream: &str) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(
        dir.join(id).join("manifest.json"),
        format!(
            r#"{{"id": "{id}", "name": "Test source", "version": "0.1.0",
                 "api_version": 1, "kind": "source", "capabilities": []}}"#
        ),
    )
    .unwrap();

    // Two pieces of data in memory, each with its address and length taken
    // from here. Counting them by hand is how a test passes for the wrong
    // reason.
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let wat = format!(
        r#"(module
            (memory (export "memory") 1)
            (global $next (mut i32) (i32.const 8192))
            (func (export "chilpa_alloc") (param i32) (result i32)
              (local i32)
              (local.set 1 (global.get $next))
              (global.set $next (i32.add (global.get $next) (local.get 0)))
              (local.get 1))
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (data (i32.const 16) "{found_esc}")
            (data (i32.const 2048) "{stream_esc}")
            (func (export "chilpa_search") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 16) (i64.const 32)) (i64.const {found_len})))
            ;; It does not look at the id it is sent: it has only one track.
            (func (export "chilpa_resolver") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 2048) (i64.const 32)) (i64.const {stream_len}))))"#,
        found_esc = esc(found),
        found_len = found.len(),
        stream_esc = esc(stream),
        stream_len = stream.len(),
    );
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(&wat).expect("it compiles"))
        .unwrap();
}

#[test]
fn a_track_from_a_source_resolves_and_plays() {
    let dir = std::env::temp_dir().join("chilpa-api-resolve");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));

    let port = serve(wave(44_100));
    let root = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&root).unwrap();
    install(
        &root,
        "com.test.source",
        r#"{"tracks":[{"external_id":"a1","title":"One from a source","artist":"Someone"}]}"#,
        &format!(r#"{{"url":"http://127.0.0.1:{port}/track.wav","codec":"wav"}}"#),
    );

    // A source that hands the track over in two parts, which is the normal
    // case in hi-res. The two URLs are the same address on purpose: the
    // server answers a different piece per connection, so what is being
    // checked is that both are read and in order.
    let split = serve_in_parts(wave(44_100), 2);
    install(
        &root,
        "com.test.parts",
        r#"{"tracks":[{"external_id":"c1","title":"One in two parts","artist":"Someone"}]}"#,
        &format!(
            r#"{{"urls":["http://127.0.0.1:{split}/a.wav","http://127.0.0.1:{split}/b.wav"],"codec":"wav"}}"#
        ),
    );

    // And one that answers with an address that is not http, to check it
    // is not opened.
    install(
        &root,
        "com.test.liar",
        r#"{"tracks":[{"external_id":"b1","title":"One that lies","artist":"Someone"}]}"#,
        r#"{"url":"file:///etc/passwd"}"#,
    );

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.load_plugins();
    chilpa.approve_plugin("com.test.source");
    chilpa.approve_plugin("com.test.parts");
    assert_eq!(chilpa.approve_plugin("com.test.liar"), 3, "all three running");

    // Search: the local side is empty, so the only result is the source's.
    chilpa.search_all("one", 20);
    assert!(wait_for(&mut chilpa, |c| c.found().len() >= 3), "all three answered");
    let hits = chilpa.found().to_vec();
    assert_eq!(hits.len(), 3);
    let good = hits.iter().position(|h| h.source == "com.test.source").unwrap();
    let liar = hits.iter().position(|h| h.source == "com.test.liar").unwrap();
    assert_eq!(hits[good].title, "One from a source");
    assert!(hits[good].playable, "now it can be attempted");

    // The one that lies: it is not opened, and why is said.
    assert!(!chilpa.play_hit(liar, OUTPUT, true), "a file:// is not opened");
    let e = chilpa.now_playing().error.clone();
    assert!(e.contains("http"), "and why is said: {e}");
    assert!(e.starts_with("Test source"), "with the source's name: {e}");

    // And the good one: a click, it resolves and it plays.
    assert!(chilpa.play_hit(good, OUTPUT, true), "{}", chilpa.now_playing().error);
    assert!(
        wait_for(&mut chilpa, |c| c.now_playing().seconds > 0.0 || !c.now_playing().playing),
        "playback started"
    );
    let now = chilpa.now_playing().clone();
    assert!(now.error.is_empty(), "it played without complaining: {}", now.error);
    // The window shows the source's card, not the URL it resolved to: a
    // stream's decoder knows nothing beyond that.
    assert_eq!(now.title, "One from a source", "the title shows, not the URL");
    assert_eq!(now.artist, "Someone");
    assert!(now.total_seconds > 0.0 || now.seconds > 0.0, "something went out the device: {now:?}");

    // And the one in two parts: it used to be refused for having more than
    // one, saying how many. It plays now, and the whole thing plays: a WAV
    // cut in half only decodes if the second half arrives behind the first.
    chilpa.search_all("one", 20);
    assert!(wait_for(&mut chilpa, |c| c.found().len() >= 3), "the sources answered again");
    let hits = chilpa.found().to_vec();
    let split = hits.iter().position(|h| h.source == "com.test.parts").unwrap();
    assert!(chilpa.play_hit(split, OUTPUT, true), "{}", chilpa.now_playing().error);
    assert!(
        wait_for(&mut chilpa, |c| !c.now_playing().playing && c.now_playing().seconds > 0.0),
        "the whole thing played: {:?}",
        chilpa.now_playing()
    );
    let now = chilpa.now_playing().clone();
    assert!(now.error.is_empty(), "it played without complaining: {}", now.error);
    assert_eq!(now.title, "One in two parts", "the card is the source's, not a URL");
    assert!(now.seconds > 0.9, "both parts came out, not just the first: {now:?}");

    let _ = std::fs::remove_dir_all(dir);
}

fn wait_for(
    chilpa: &mut chilpa_api::Chilpa,
    mut cond: impl FnMut(&mut chilpa_api::Chilpa) -> bool,
) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        chilpa.poll();
        if cond(chilpa) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}
