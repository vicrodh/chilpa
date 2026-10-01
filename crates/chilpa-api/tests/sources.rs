//! A source that is browsed, from the facade: the tree is walked, a station
//! sounds, and what the listener saves is still there after a restart.
//!
//! The plugin is the compiled fixture of `chilpa-plugin`, built from Rust
//! with the SDK. Its pretend service is a toy server on this machine, named
//! to it through its store. Nothing goes out a real device: the output is
//! `file:capture`, chosen before anything plays.
//!
//! It is a single test because it points the `XDG_*` variables at a
//! temporary directory, and those belong to the whole process.

use std::io::{Read, Write};
use std::net::TcpListener;

const ID: &str = "app.chilpa.fixture";
const WASM: &[u8] = include_bytes!("../../chilpa-plugin/tests/fixtures/fixture.wasm");
const MANIFEST: &str = include_str!("../../chilpa-plugin/tests/fixtures/manifest.json");

/// One second of a tone, as a WAV.
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

fn view(chilpa: &mut chilpa_api::Chilpa) -> serde_json::Value {
    serde_json::from_str(&chilpa.sources_json()).expect("the snapshot is JSON")
}

fn ids(view: &serde_json::Value) -> Vec<String> {
    view["entries"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap().to_string()).collect()
}

fn wait_for(
    chilpa: &mut chilpa_api::Chilpa,
    mut cond: impl FnMut(&mut chilpa_api::Chilpa) -> bool,
) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        chilpa.poll();
        if cond(chilpa) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

/// Sends a command and waits for the listing it asks for to settle.
fn go(chilpa: &mut chilpa_api::Chilpa, action: &str, id: &str, text: &str) -> serde_json::Value {
    chilpa.sources_command(action, id, text);
    assert!(wait_for(chilpa, |c| !view(c)["loading"].as_bool().unwrap()), "{action} {id} never settled");
    view(chilpa)
}

#[test]
fn a_source_is_browsed_a_station_sounds_and_what_is_saved_stays() {
    let dir = std::env::temp_dir().join("chilpa-api-sources");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));

    let port = serve(wave(44_100));
    let root = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(root.join(ID).join("store")).unwrap();
    std::fs::write(root.join(ID).join("manifest.json"), MANIFEST).unwrap();
    std::fs::write(root.join(ID).join("plugin.wasm"), WASM).unwrap();
    std::fs::write(root.join(ID).join("store").join("base"), format!("http://127.0.0.1:{port}")).unwrap();

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.use_output("file:capture", false);
    chilpa.load_plugins();
    assert!(chilpa.source_panels().is_empty(), "a plugin nobody approved has no place in the window");
    assert_eq!(chilpa.approve_plugin(ID), 1);
    let panels = chilpa.source_panels();
    assert_eq!(panels.len(), 1);
    assert_eq!((panels[0].id.as_str(), panels[0].name.as_str()), (ID, "Fixture"));

    // The root: Chilpa's own folder first, the plugin's place to type drawn
    // as a field and not as a row.
    let top = go(&mut chilpa, "open", ID, "");
    assert_eq!(top["error"], "", "{top}");
    assert_eq!(top["name"], "Fixture");
    assert_eq!(top["searchable"], true);
    let listed = ids(&top);
    assert_eq!(listed[0], "chilpa:saved");
    assert!(!listed.contains(&"search".to_string()));
    assert!(listed.contains(&"genres".to_string()));
    assert_eq!(top["crumbs"].as_array().unwrap().len(), 0);

    // Two levels down, and back up by the crumbs.
    let genres = go(&mut chilpa, "enter", "genres", "");
    assert_eq!(genres["title"], "Genres");
    let jazz = go(&mut chilpa, "enter", "genre:jazz", "");
    assert_eq!(jazz["crumbs"], serde_json::json!(["Genres", "Jazz"]));
    assert_eq!(ids(&jazz), ["live:one", "live:two"]);
    let back = go(&mut chilpa, "crumb", "1", "");
    assert_eq!(ids(&back), ["genre:jazz"]);

    // A long folder comes by pages, and the pages add up.
    let _ = go(&mut chilpa, "crumb", "0", "");
    let long = go(&mut chilpa, "enter", "long", "");
    assert_eq!(long["more"], true);
    let _ = go(&mut chilpa, "more", "", "");
    let all = go(&mut chilpa, "more", "", "");
    assert_eq!(ids(&all).len(), 6);
    assert_eq!(all["more"], false);

    // Typing searches; clearing the text is the root again.
    let found = go(&mut chilpa, "search", "", "two");
    assert_eq!(ids(&found), ["live:two"]);
    assert_eq!(found["text"], "two");
    let cleared = go(&mut chilpa, "search", "", "");
    assert_eq!(ids(&cleared)[0], "chilpa:saved");

    // What cannot be played says why and starts nothing.
    chilpa.sources_command("play", "hls:one", "");
    assert!(chilpa.now_playing().error.contains("HLS"), "{}", chilpa.now_playing().error);
    assert!(!chilpa.now_playing().playing);

    // A station: it is asked for off this thread, then it sounds, live,
    // under the name it was listed with.
    chilpa.sources_command("enter", "live:one", "");
    assert_eq!(view(&mut chilpa)["entries"].as_array().unwrap().iter().filter(|e| e["resolving"] == true).count(), 1);
    assert!(
        wait_for(&mut chilpa, |c| c.now_playing().playing && c.now_playing().seconds > 0.0),
        "the station started: {:?}",
        chilpa.now_playing()
    );
    let now = chilpa.now_playing().clone();
    assert!(now.path.ends_with("/live/one.wav"), "{}", now.path);
    assert_eq!(now.title, "Live One", "the card is the entry's, not the address");
    assert!(now.live, "a station has no end to seek to");
    let playing = view(&mut chilpa);
    let row = playing["entries"].as_array().unwrap().iter().find(|e| e["id"] == "live:one").unwrap();
    assert_eq!(row["sounding"], true);
    assert_eq!(row["resolving"], false);

    // Saving, and the saved folder listing it without asking the plugin.
    chilpa.sources_command("save", "live:one", "");
    assert_eq!(view(&mut chilpa)["entries"].as_array().unwrap().iter().filter(|e| e["saved"] == true).count(), 1);
    let saved = go(&mut chilpa, "enter", "chilpa:saved", "");
    assert_eq!(ids(&saved), ["live:one"]);
    assert_eq!(saved["inSaved"], true);
    chilpa.stop();
    drop(chilpa);

    // Another run: the saved station is still there, and it still plays.
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.use_output("file:capture", false);
    chilpa.load_plugins();
    let _ = go(&mut chilpa, "open", ID, "");
    let saved = go(&mut chilpa, "enter", "chilpa:saved", "");
    assert_eq!(ids(&saved), ["live:one"]);
    assert_eq!(saved["entries"][0]["title"], "Live One");
    chilpa.sources_command("play", "live:one", "");
    assert!(
        wait_for(&mut chilpa, |c| c.now_playing().playing && c.now_playing().live),
        "the saved station started: {:?}",
        chilpa.now_playing()
    );
    chilpa.sources_command("save", "live:one", "");
    let emptied = go(&mut chilpa, "retry", "", "");
    assert!(ids(&emptied).is_empty(), "taken out, it is gone from the saved ones");
    chilpa.stop();

    let _ = std::fs::remove_dir_all(dir);
}
