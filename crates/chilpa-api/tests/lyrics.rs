//! A track's lyrics through the whole facade: what came with the file, what
//! its source has, and what the lyrics service finds.
//!
//! The service here is a stand-in that speaks the contract of
//! `chilpa_lyrics::service`; the source is a process plugin in bash. Nothing
//! leaves the machine.
//!
//! It is a single test because it points `XDG_DATA_HOME`, `XDG_CACHE_HOME`
//! and the service's address somewhere temporary, and those belong to the
//! whole process.
#![cfg(target_os = "linux")]

use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const OUTPUT: &str = "file:capture";
const PLUGIN_ID: &str = "com.test.lyrics";

/// A source in bash with three tracks: one it has lyrics for, one it has
/// none for, and one it fails on. `@ORIGIN@` is where their audio is.
const PLUGIN: &str = r#"#!/usr/bin/bash
while IFS= read -r line; do
  (
    id=$(printf '%s' "$line" | sed -E 's/.*"id":([0-9]+).*/\1/')
    answer() { printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$id" "$1"; }
    case "$line" in
      *'"method":"initialize"'*) answer '{"protocol":1,"capabilities":{"home":true,"library":["playlists"],"track_lyrics":1}}' ;;
      *'"method":"home"'*) answer '{"shelves":[]}' ;;
      *'"method":"library.list"'*) answer '{"items":[{"kind":"playlist","ref":"playlist:1","title":"Sung","tracks":3}],"total":1}' ;;
      *'"method":"playlist.get"'*) answer '{"playlist":{"kind":"playlist","ref":"playlist:1","title":"Sung"},"tracks":[{"kind":"track","ref":"track:found","title":"Source Found","artist":"Band","duration_ms":100},{"kind":"track","ref":"track:none","title":"Source None","artist":"Band","duration_ms":100},{"kind":"track","ref":"track:error","title":"Source Error","artist":"Band","duration_ms":100}]}' ;;
      *'"method":"track.resolve"'*) answer '{"url":"@ORIGIN@/audio.wav","format":{"codec":"wav"}}' ;;
      *'"method":"track.lyrics"'*)
        case "$line" in
          *'"ref":"track:found"'*) answer '{"status":"found","lyrics":{"format":"lrc","text":"[00:00.05]From the source"},"fingerprint":{"duration_ms":100}}' ;;
          *'"ref":"track:none"'*) answer '{"status":"none"}' ;;
          *) printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32000,"message":"The session ended"}}\n' "$id" ;;
        esac ;;
      *'"method":"inspect"'*|*'"method":"tick"'*) printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"No compatibility methods"}}\n' "$id" ;;
      *) answer '{}' ;;
    esac
  ) &
done
wait
"#;

/// A short WAV that says what song it is, in its INFO chunk.
fn wave(title: &str, artist: &str) -> Vec<u8> {
    let frames = 4_410u32;
    let data = frames * 4;
    let mut info = b"INFO".to_vec();
    for (id, value) in [(b"INAM", title), (b"IART", artist)] {
        let mut text = value.as_bytes().to_vec();
        text.push(0);
        info.extend_from_slice(id);
        info.extend_from_slice(&(text.len() as u32).to_le_bytes());
        if text.len() % 2 == 1 {
            text.push(0);
        }
        info.extend_from_slice(&text);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(4 + 24 + 8 + data + 8 + info.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&176_400u32.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        let v = (i % 3_000) as i16;
        out.extend_from_slice(&v.to_le_bytes());
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(b"LIST");
    out.extend_from_slice(&(info.len() as u32).to_le_bytes());
    out.extend_from_slice(&info);
    out
}

fn song(dir: &Path, title: &str) -> String {
    let path: PathBuf = dir.join(format!("{title}.wav"));
    std::fs::write(&path, wave(title, "Someone")).unwrap();
    path.display().to_string()
}

/// The stand-in service: the titles it was asked about, in order.
struct Service {
    origin: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Service {
    fn asked_about(&self, title: &str) -> usize {
        self.asked.lock().unwrap().iter().filter(|t| t.as_str() == title).count()
    }
}

/// Answers the contract by the title asked: "Slow" takes its time, "Missing"
/// has none, "Broken" fails, anything else is found. It also hands out the
/// audio of the source's tracks.
fn serve() -> Service {
    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let origin = format!("http://{}", server.server_addr().to_ip().unwrap());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let audio = Arc::new(wave("", ""));
    for _ in 0..4 {
        let (server, asked, audio) = (server.clone(), asked.clone(), audio.clone());
        std::thread::spawn(move || {
            while let Ok(mut request) = server.recv() {
                if request.method() == &tiny_http::Method::Get {
                    let range = request.headers().iter().find(|h| h.field.equiv("Range")).map(|h| h.value.to_string());
                    let _ = match range.and_then(|r| chilpa_cast::media_server::range(&r, audio.len() as u64)) {
                        Some((start, length)) => {
                            let header = tiny_http::Header::from_bytes("Content-Range", format!("bytes {start}-{}/{}", start + length - 1, audio.len())).unwrap();
                            request.respond(tiny_http::Response::from_data(audio[start as usize..(start + length) as usize].to_vec()).with_status_code(206).with_header(header))
                        }
                        None => request.respond(tiny_http::Response::from_data(audio.to_vec())),
                    };
                    continue;
                }
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);
                let question: Value = serde_json::from_str(&body).unwrap_or_default();
                let title = question["track"]["title"].as_str().unwrap_or_default().to_string();
                asked.lock().unwrap().push(title.clone());
                let (status, answer) = if title.starts_with("Slow") {
                    std::thread::sleep(Duration::from_millis(1_200));
                    (200, found(&question))
                } else if title.starts_with("Missing") {
                    (200, json!({"schema": 1, "status": "none"}))
                } else if title.starts_with("Broken") {
                    (503, json!({"schema": 1, "error": "The lyrics providers could not be asked"}))
                } else {
                    (200, found(&question))
                };
                let _ = request.respond(tiny_http::Response::from_string(answer.to_string()).with_status_code(status));
            }
        });
    }
    Service { origin, asked }
}

fn found(question: &Value) -> Value {
    let track = &question["track"];
    json!({"schema": 1, "status": "found", "document": {
        "schema": 1, "timing": "line",
        "lines": [{"start_ms": 50, "text": format!("From the service for {}", track["title"].as_str().unwrap_or_default())}],
        "source": {"provider": "fixture", "name": "Fixture Lyrics",
            "claim": {"title": track["title"], "artist": track["artist"], "duration_ms": track["duration_ms"]}},
    }})
}

/// Plays one entry and waits for its lyrics: the text and who gave it.
/// `None` when none show in that time.
fn lyrics_of(chilpa: &mut chilpa_api::Chilpa, path: &str, title: &str, wait: Duration) -> Option<(String, String)> {
    chilpa.play_tracks(&[path.to_string()], OUTPUT, false);
    let deadline = Instant::now() + wait;
    loop {
        let now = chilpa.poll();
        if now.title == title && !now.lyrics.is_empty() {
            return Some((now.lyrics.join("\n"), now.lyrics_source.clone()));
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn lyrics_come_from_the_file_the_source_or_the_service_in_that_order() {
    let dir = std::env::temp_dir().join(format!("chilpa-api-lyrics-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    let service = serve();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
    std::env::set_var("CHILPA_LYRICS_SERVICE", &service.origin);
    let music = dir.join("music");
    let long = Duration::from_secs(10);

    let mut chilpa = chilpa_api::Chilpa::new();

    // What came with the file wins, and nobody is asked.
    let local = song(&music, "Local Song");
    std::fs::write(music.join("Local Song.lrc"), "[00:00.05]From next door\n").unwrap();
    let (text, source) = lyrics_of(&mut chilpa, &local, "Local Song", long).expect("the file next to the track");
    assert_eq!(text, "From next door");
    assert_ne!(source, "Fixture Lyrics");
    assert_eq!(service.asked_about("Local Song"), 0, "a track with its own lyrics is not looked up");

    // A file with none is looked up at the service, with all that is known.
    let remote = song(&music, "Remote Song");
    let (text, source) = lyrics_of(&mut chilpa, &remote, "Remote Song", long).expect("the service has them");
    assert_eq!(text, "From the service for Remote Song");
    assert_eq!(source, "Fixture Lyrics", "the screen says who the service says gave them");
    assert_eq!(service.asked_about("Remote Song"), 1);
    // The second time they come from Chilpa's own cache.
    lyrics_of(&mut chilpa, &remote, "Remote Song", long).expect("still there");
    assert_eq!(service.asked_about("Remote Song"), 1, "found lyrics are remembered");

    // A track skipped before its lyrics arrive never shows them on the next.
    let slow = song(&music, "Slow Song");
    let quick = song(&music, "Quick Song");
    chilpa.play_tracks(&[slow], OUTPUT, false);
    let deadline = Instant::now() + long;
    while service.asked_about("Slow Song") == 0 {
        chilpa.poll();
        assert!(Instant::now() < deadline, "the slow song was never looked up");
        std::thread::sleep(Duration::from_millis(10));
    }
    let (text, _) = lyrics_of(&mut chilpa, &quick, "Quick Song", long).expect("the next track's lyrics");
    assert_eq!(text, "From the service for Quick Song");
    let until = Instant::now() + Duration::from_millis(1_800);
    while Instant::now() < until {
        let now = chilpa.poll();
        assert!(!now.lyrics.join("\n").contains("Slow Song"), "lyrics that arrived late were shown over another track");
        std::thread::sleep(Duration::from_millis(20));
    }

    // "None" is remembered; a failure is asked again.
    for (title, twice) in [("Missing Song", 1), ("Broken Song", 2)] {
        let path = song(&music, title);
        for expected in [1, twice] {
            assert!(lyrics_of(&mut chilpa, &path, title, Duration::from_millis(900)).is_none(), "{title} has no lyrics to show");
            assert_eq!(service.asked_about(title), expected, "{title}");
        }
    }

    // A source's own tracks.
    let folder = chilpa_api::Chilpa::plugins_dir().join(PLUGIN_ID);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("manifest.json"), format!(r#"{{"id": "{PLUGIN_ID}", "name": "Sung Source", "version": "0.1.0",
        "api_version": 1, "kind": "source", "runtime": "process", "capabilities": [{{"type":"register_source"}},{{"type":"register_panel"}}]}}"#)).unwrap();
    std::fs::write(folder.join("plugin"), PLUGIN.replace("@ORIGIN@", &service.origin)).unwrap();
    std::fs::set_permissions(folder.join("plugin"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(chilpa.load_plugins(), 0, "nothing runs before it is approved");
    assert_eq!(chilpa.approve_plugin(PLUGIN_ID), 1);

    let deadline = Instant::now() + long;
    let playlist = loop {
        let sidebar: Value = serde_json::from_str(&chilpa.sidebar_playlists("")).unwrap();
        if let Some(id) = sidebar["playlists"].as_array().unwrap().iter().find(|p| p["name"] == "Sung").and_then(|p| p["id"].as_i64()) {
            break id;
        }
        assert!(Instant::now() < deadline, "the source's playlist did not reach the sidebar");
        std::thread::sleep(Duration::from_millis(50));
    };
    let rows = loop {
        let rows = chilpa.album_track_rows(playlist);
        if rows.len() == 3 {
            break rows;
        }
        assert!(Instant::now() < deadline, "the source's tracks did not load");
        std::thread::sleep(Duration::from_millis(50));
    };
    let path_of = |title: &str| rows.iter().find(|r| r.title == title).map(|r| r.path.clone()).unwrap();

    // The source has them: they win, though the service has lyrics too.
    let (text, source) = lyrics_of(&mut chilpa, &path_of("Source Found"), "Source Found", long).expect("the source's lyrics");
    assert_eq!(text, "From the source");
    assert_eq!(source, "Sung Source");

    // The source has none: the service's show.
    let (text, source) = lyrics_of(&mut chilpa, &path_of("Source None"), "Source None", long).expect("the service's lyrics");
    assert_eq!(text, "From the service for Source None");
    assert_eq!(source, "Fixture Lyrics");

    // The source fails: the service's show, and the track plays all the same.
    let (text, _) = lyrics_of(&mut chilpa, &path_of("Source Error"), "Source Error", long).expect("the service's lyrics");
    assert_eq!(text, "From the service for Source Error");
    assert!(chilpa.now_playing().error.is_empty(), "{}", chilpa.now_playing().error);

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}
