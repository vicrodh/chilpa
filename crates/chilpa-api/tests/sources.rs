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

/// Serves the same WAV to whoever asks, whole or by the range asked for,
/// which is how a file with an end is read. An address under `/episodes/`
/// is not the file: it redirects to it, the way a published episode does.
fn serve(audio: Vec<u8>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let asked = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = asked.split_whitespace().nth(1).unwrap_or("/").to_string();
            if let Some(name) = path.strip_prefix("/episodes/") {
                let _ = stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: /files/{name}\r\nContent-Length: 0\r\n\r\n").as_bytes());
                continue;
            }
            let range = asked.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(str::to_string));
            let head = match range.as_deref().and_then(|r| r.trim().split_once('-')) {
                Some((from, to)) => {
                    let from: usize = from.parse().unwrap_or(0);
                    let to = to.parse::<usize>().unwrap_or(audio.len() - 1).min(audio.len() - 1);
                    let head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Type: audio/wav\r\nContent-Range: bytes {from}-{to}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        audio.len(),
                        to + 1 - from
                    );
                    (head, from, to + 1)
                }
                None => (
                    format!("HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\n\r\n", audio.len()),
                    0,
                    audio.len(),
                ),
            };
            if stream.write_all(head.0.as_bytes()).is_ok() {
                let _ = stream.write_all(&audio[head.1..head.2]);
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

    // Eight seconds: long enough for the place in it to be written down.
    let port = serve(wave(44_100 * 8));
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

    // Typing narrows what is already on screen, without asking anybody.
    chilpa.sources_command("filter", "", "GEN");
    let narrowed = view(&mut chilpa);
    assert_eq!(ids(&narrowed), ["genres"]);
    assert_eq!(narrowed["total"], top["total"], "the listing is still whole underneath");
    chilpa.sources_command("filter", "", "");
    assert_eq!(ids(&view(&mut chilpa)).len(), listed.len());

    // Two levels down, and back up by the crumbs.
    let genres = go(&mut chilpa, "enter", "genres", "");
    assert_eq!(genres["title"], "Genres");
    let jazz = go(&mut chilpa, "enter", "genre:jazz", "");
    assert_eq!(jazz["crumbs"], serde_json::json!(["Genres", "Jazz"]));
    assert_eq!(ids(&jazz), ["live:one", "live:two"]);

    // An entry has a page of its own, with what the plugin knows of it.
    chilpa.sources_command("detail", "live:one", ID);
    let page = view(&mut chilpa)["detail"].clone();
    assert_eq!(page["title"], "Live One");
    assert_eq!(page["name"], "Fixture");
    assert_eq!(page["facts"][0]["label"], "Country");
    assert_eq!(page["sounding"], false);
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

    // A stream address typed where the search goes is played as it is, by
    // Chilpa, and listed as one entry so it can be saved like any other.
    chilpa.stop();
    let address = format!("http://127.0.0.1:{port}/typed.wav");
    chilpa.sources_command("search", "", &address);
    let typed = view(&mut chilpa);
    assert_eq!(ids(&typed), [format!("chilpa:url:{address}")]);
    assert!(
        // The card arrives when the player says it started, not when it was asked to.
        wait_for(&mut chilpa, |c| c.now_playing().live && c.now_playing().path == address),
        "the address started: {:?}",
        chilpa.now_playing()
    );
    assert_eq!(chilpa.now_playing().title, format!("127.0.0.1:{port}"));
    assert!(chilpa.now_playing().live);
    let _ = go(&mut chilpa, "search", "", "");
    chilpa.stop();
    chilpa.sources_command("enter", "live:one", "");
    assert!(wait_for(&mut chilpa, |c| c.now_playing().playing && c.now_playing().title == "Live One"));

    // Saving, and the saved folder listing it without asking the plugin.
    chilpa.sources_command("save", "live:one", "");
    assert_eq!(view(&mut chilpa)["entries"].as_array().unwrap().iter().filter(|e| e["saved"] == true).count(), 1);
    let saved = go(&mut chilpa, "enter", "chilpa:saved", "");
    assert_eq!(ids(&saved), ["live:one"]);
    assert_eq!(saved["inSaved"], true);

    // A show: a folder that is followed by saving it whole, with episodes
    // that have an end, a length and a day.
    let _ = go(&mut chilpa, "crumb", "0", "");
    let show = go(&mut chilpa, "enter", "show", "");
    assert_eq!(ids(&show), ["ep:one", "ep:two"]);
    assert_eq!((show["placeSavable"].clone(), show["placeSaved"].clone()), (true.into(), false.into()));
    assert_eq!(show["entries"][0]["finite"], true);
    assert_eq!(show["entries"][0]["meta"], "2023-11-14 · 0:02");
    chilpa.sources_command("save", "", "");
    assert_eq!(view(&mut chilpa)["placeSaved"], true);

    // An episode goes to the queue under its id, not its address, and plays
    // as a file: it has a length, it is not live, and it is named.
    chilpa.stop();
    chilpa.sources_command("play", "ep:one", "");
    assert!(
        wait_for(&mut chilpa, |c| c.now_playing().playing && c.now_playing().total_seconds > 7.0),
        "the episode started: {:?}",
        chilpa.now_playing()
    );
    let now = chilpa.now_playing().clone();
    assert!(now.path.starts_with("remote:plugin/"), "{}", now.path);
    assert!(!now.path.contains("127.0.0.1"), "the queue carries no address: {}", now.path);
    assert_eq!((now.title.as_str(), now.artist.as_str()), ("Episode one", "A Show"));
    assert!(!now.live);
    chilpa.sources_command("queue", "ep:two", "");
    assert_eq!(chilpa.now_playing().queue_len, 2, "the second waits behind the first");

    // The capture output takes the audio as fast as it is decoded, so both
    // run to their end at once. One that reached its end is marked heard,
    // even when it ended between two of the periodic notes of its place.
    assert!(
        wait_for(&mut chilpa, |c| {
            let now = view(c);
            now["entries"][0]["played"] == true && now["entries"][1]["played"] == true
        }),
        "both were heard to the end: {}",
        view(&mut chilpa)["entries"]
    );
    assert_eq!(view(&mut chilpa)["entries"][0]["heard"], "Heard");
    assert_eq!(view(&mut chilpa)["entries"][0]["progress"], 1.0);
    chilpa.stop();
    drop(chilpa);

    // Another run: the saved station is still there, and it still plays.
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.use_output("file:capture", false);
    chilpa.load_plugins();
    // What the last run left in the queue is still named, with nobody
    // asked: the queue only carried its id.
    let waiting = chilpa.now_playing().clone();
    assert!(waiting.title.starts_with("Episode "), "{waiting:?}");
    assert_eq!(waiting.artist, "A Show");

    // What was heard is still marked, and the mark can be taken away and
    // put back by hand.
    let _ = go(&mut chilpa, "open", ID, "");
    let show = go(&mut chilpa, "enter", "show", "");
    assert_eq!(show["entries"][0]["played"], true);
    chilpa.sources_command("heard", "ep:two", "");
    assert_eq!(view(&mut chilpa)["entries"][1]["played"], false);
    assert_eq!(view(&mut chilpa)["entries"][1]["heard"], "");
    chilpa.sources_command("heard", "ep:two", "");
    assert_eq!(view(&mut chilpa)["entries"][1]["played"], true);

    let _ = go(&mut chilpa, "crumb", "0", "");
    let saved = go(&mut chilpa, "enter", "chilpa:saved", "");
    assert_eq!(ids(&saved), ["show", "live:one"], "the latest first: the show, then the station");
    assert_eq!(saved["entries"][1]["title"], "Live One");
    assert_eq!(saved["entries"][0]["kind"], "folder", "a saved show still opens");
    chilpa.sources_command("play", "live:one", "");
    assert!(
        wait_for(&mut chilpa, |c| c.now_playing().playing && c.now_playing().live),
        "the saved station started: {:?}",
        chilpa.now_playing()
    );
    chilpa.sources_command("save", "live:one", "");
    let emptied = go(&mut chilpa, "retry", "", "");
    assert_eq!(ids(&emptied), ["show"], "taken out, the station is gone from the saved ones");
    chilpa.stop();

    let _ = std::fs::remove_dir_all(dir);
}
