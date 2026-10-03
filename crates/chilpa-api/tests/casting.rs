//! A silent renderer fetches the original bytes through the facade.
use chilpa_api::{CastDevice, CastKind, Chilpa};
use std::{io::Write, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};

#[derive(Default)]
struct State { uri: String, playing: bool, paused: bool, finish: bool, loads: usize, bytes: Vec<u8>, mime: String, seeks: usize, volume: u8, muted: bool, position: u32, duration: u32, reject_transport: bool, reject_seek: bool, stop_delay: bool }
struct Renderer { device: CastDevice, state: Arc<Mutex<State>>, done: Arc<AtomicBool>, thread: Option<std::thread::JoinHandle<()>> }
impl Renderer {
    fn new() -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap(); let address = server.server_addr().to_ip().unwrap();
        let state = Arc::new(Mutex::new(State { position: 9, duration: 10, ..Default::default() })); let shared = state.clone();
        let done = Arc::new(AtomicBool::new(false)); let stopped = done.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                let Some(mut request) = server.recv_timeout(Duration::from_millis(50)).unwrap() else { continue };
                let content = if request.method() == &tiny_http::Method::Get {
                    "<root><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Silent renderer</friendlyName><UDN>uuid:silent</UDN><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:3</serviceType><controlURL>/transport</controlURL></service><service><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType><controlURL>/volume</controlURL></service><service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType><controlURL>/connection</controlURL></service></serviceList></device></root>".to_string()
                } else {
                    let action = request.headers().iter().find(|h| h.field.equiv("SOAPAction")).unwrap().value.as_str().trim_matches('"').rsplit('#').next().unwrap().to_string();
                    let mut body = String::new(); request.as_reader().read_to_string(&mut body).unwrap();
                    let field = |name: &str| body.split(&format!("<{name}>")).nth(1).and_then(|s| s.split(&format!("</{name}>")).next()).unwrap_or("").to_string();
                    let mut state = shared.lock().unwrap();
                    match action.as_str() {
                        "GetProtocolInfo" => "<Source/><Sink>http-get:*:audio/*:*</Sink>".into(),
                        "SetAVTransportURI" => { state.uri = field("CurrentURI"); state.loads += 1; state.finish = false; "".into() }
                        "Play" => {
                            state.playing = true; state.paused = false;
                            let reply = chilpa_core::call_server(&chilpa_core::Call { method: "GET", url: &state.uri, headers: &[], body: None, timeout: Duration::from_secs(3), max_bytes: 1_000_000 }).unwrap();
                            assert_eq!(reply.status, 200); state.mime = reply.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case("content-type")).unwrap().1.clone(); state.bytes = reply.body; "".into()
                        }
                        "Pause" => { state.paused = true; "".into() }
                        "Stop" => { if state.stop_delay { std::thread::sleep(Duration::from_millis(500)); } state.playing = false; "".into() }
                        "Seek" => { state.seeks += 1; state.position = field("Target").rsplit(':').next().unwrap().parse().unwrap(); if state.reject_seek { "<errorCode>710</errorCode>".into() } else { "".into() } }
                        "SetVolume" => { state.volume = field("DesiredVolume").parse().unwrap(); "".into() }
                        "SetMute" => { state.muted = field("DesiredMute") == "1"; "".into() }
                        "GetVolume" => format!("<CurrentVolume>{}</CurrentVolume>", state.volume),
                        "GetMute" => format!("<CurrentMute>{}</CurrentMute>", u8::from(state.muted)),
                        "GetPositionInfo" => format!("<RelTime>00:00:{:02}</RelTime><TrackDuration>00:00:{:02}</TrackDuration>", state.position, state.duration),
                        "GetTransportInfo" if state.reject_transport => "<errorCode>501</errorCode>".into(),
                        "GetTransportInfo" => format!("<CurrentTransportState>{}</CurrentTransportState>", if state.finish || !state.playing { "STOPPED" } else if state.paused { "PAUSED_PLAYBACK" } else { "PLAYING" }),
                        _ => panic!("unexpected action: {action}"),
                    }
                };
                let xml = if content.starts_with("<root>") { content } else { format!("<response>{content}</response>") };
                let _ = request.respond(tiny_http::Response::from_string(xml));
            }
        });
        Self { device: CastDevice { id: "uuid:silent".into(), name: "Silent renderer".into(), model: "Test".into(), kind: CastKind::Dlna, address, description: format!("http://{address}/device.xml") }, state, done, thread: Some(thread) }
    }
}
impl Drop for Renderer { fn drop(&mut self) { self.done.store(true, Ordering::Release); self.thread.take().unwrap().join().unwrap(); } }
#[track_caller]
fn wait(api: &mut Chilpa, test: impl Fn(&chilpa_api::NowPlaying) -> bool) {
    let until = Instant::now() + Duration::from_secs(18);
    loop { let now = api.poll(); if test(now) { return; } assert!(Instant::now() < until, "cast did not reach expected state: playing={} paused={} title={:?} index={} seconds={} error={:?}", now.playing, now.paused, now.title, now.index, now.seconds, now.error); std::thread::sleep(Duration::from_millis(40)); }
}
#[test]
fn facade_casts_queue_controls_and_remote_originals_without_local_playback() {
    let dir = std::env::temp_dir().join(format!("chilpa-cast-smoke-{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap();
    for key in ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"] { std::env::set_var(key, dir.join(key)); }
    std::env::set_var("TMPDIR", &dir);
    let cover = dir.join("cover.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([40, 80, 120])).save(&cover).unwrap();
    let path = dir.join("first.wav");
    let mut file = std::fs::File::create(&path).unwrap();
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF"); bytes.extend((36u32 + 176400).to_le_bytes()); bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes()); bytes.extend(1u16.to_le_bytes()); bytes.extend(2u16.to_le_bytes());
    bytes.extend(44100u32.to_le_bytes()); bytes.extend(176400u32.to_le_bytes()); bytes.extend(4u16.to_le_bytes()); bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data"); bytes.extend(176400u32.to_le_bytes()); bytes.resize(176444, 0); file.write_all(&bytes).unwrap(); drop(file);
    let second = dir.join("second.wav"); std::fs::write(&second, &bytes).unwrap();
    let renderer = Renderer::new(); let mut api = Chilpa::new();
    api.cast_connect_device(renderer.device.clone());
    let start = Instant::now();
    api.play_tracks(&[path.display().to_string(), second.display().to_string()], "file:capture", false);
    assert!(start.elapsed() < Duration::from_secs(1), "dispatch waits on the network");
    wait(&mut api, |n| n.playing && n.title == "first" && n.seconds > 0.0);
    assert_eq!(renderer.state.lock().unwrap().bytes, bytes);
    assert!(!api.poll().bit_perfect);
    assert_eq!(api.now_playing().artwork_path, cover.display().to_string());
    assert_eq!(serde_json::from_str::<serde_json::Value>(&api.cast_json()).unwrap()["icon"], "dlna");
    assert_eq!(api.now_playing().quality.bits, Some(16));
    assert_eq!(api.now_playing().quality.rate_hz, Some(44100));
    api.set_paused(true); wait(&mut api, |n| n.paused && renderer.state.lock().unwrap().paused);
    api.set_paused(false); wait(&mut api, |n| !n.paused && n.playing && !renderer.state.lock().unwrap().paused);
    api.seek(4.0); api.set_player_volume(42); api.set_player_muted(true);
    wait(&mut api, |_| { let s = renderer.state.lock().unwrap(); s.seeks > 0 && s.volume == 42 && s.muted });
    wait(&mut api, |n| n.seconds == 4.0);
    assert_eq!(renderer.state.lock().unwrap().position, 4);
    // Preparing another tab cannot send audio; activating it replaces this one queue.
    let original = serde_json::from_str::<serde_json::Value>(&api.listen_json()).unwrap()["active"].as_str().unwrap().parse().unwrap();
    let loads = renderer.state.lock().unwrap().loads;
    let buffer = api.listen_create("Other", &[second.display().to_string()]);
    api.listen_select(buffer); api.listen_rename(buffer, "Later");
    api.poll();
    assert_eq!(renderer.state.lock().unwrap().loads, loads);
    assert_eq!(api.now_playing().title, "first");
    assert!(api.listen_play(buffer, ""));
    wait(&mut api, |n| n.title == "second" && renderer.state.lock().unwrap().loads > loads);
    let loads = renderer.state.lock().unwrap().loads;
    assert!(api.listen_play(original, ""));
    wait(&mut api, |n| n.title == "first" && n.total_seconds == 10.0 && n.seconds == 4.0 && renderer.state.lock().unwrap().loads > loads);
    assert_eq!(api.queue_paths().len(), 2);
    renderer.state.lock().unwrap().finish = true;
    wait(&mut api, |n| n.index == 1 && n.title == "second");
    api.previous_track("file:capture", false); wait(&mut api, |n| n.index == 0 && n.title == "first");
    api.next_track(); wait(&mut api, |n| n.index == 1 && n.title == "second");
    // The position query can work even when the transport query faults.
    renderer.state.lock().unwrap().reject_transport = true;
    api.set_paused(true); wait(&mut api, |n| n.paused && renderer.state.lock().unwrap().paused);
    let until = Instant::now() + Duration::from_secs(6);
    while Instant::now() < until {
        let now = api.poll();
        assert!(now.paused && now.error.is_empty(), "a live position query was treated as a lost renderer");
        assert!(api.cast_destination().is_some());
        std::thread::sleep(Duration::from_millis(40));
    }
    api.set_paused(false); wait(&mut api, |n| !n.paused && n.playing && !renderer.state.lock().unwrap().paused);
    renderer.state.lock().unwrap().reject_transport = false;

    // Selecting an output with a stopped queue must not start it.
    api.stop();
    api.cast_disconnect();
    let loads = renderer.state.lock().unwrap().loads;
    api.cast_connect_device(renderer.device.clone());
    let until = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < until { api.poll(); std::thread::sleep(Duration::from_millis(30)); }
    assert_eq!(renderer.state.lock().unwrap().loads, loads, "connecting started a stopped queue");
    assert!(!api.now_playing().playing);
    api.set_paused(false);
    wait(&mut api, |n| n.playing && n.title == "second" && n.seconds > 0.0);

    // A receiver that cannot seek still owns a loaded, controllable track.
    let other = Renderer::new();
    let mut other_device = other.device.clone(); other_device.id = "uuid:other".into();
    other.state.lock().unwrap().reject_seek = true;
    api.cast_connect_device(other_device);
    wait(&mut api, |n| n.title == "second" && n.playing && !n.error.is_empty());
    assert!(other.state.lock().unwrap().playing);
    api.set_paused(true); wait(&mut api, |n| n.paused && other.state.lock().unwrap().paused);
    api.set_paused(false); wait(&mut api, |n| !n.paused && n.playing && !other.state.lock().unwrap().paused);
    let loads = other.state.lock().unwrap().loads;
    assert_eq!(loads, 1, "resume reloaded an already playing track");
    api.cast_connect_device(renderer.device.clone());
    wait(&mut api, |n| n.title == "second" && n.playing && n.seconds > 0.0);
    drop(other);

    // Pause/resume queued behind LOAD controls that load, without replacing it.
    let loads = renderer.state.lock().unwrap().loads;
    api.play_file(&path, "file:capture", false);
    api.set_paused(true);
    api.set_paused(false);
    wait(&mut api, |n| n.title == "first" && n.playing && n.seconds > 0.0);
    assert_eq!(renderer.state.lock().unwrap().loads, loads + 1);
    let url = renderer.state.lock().unwrap().uri.clone(); api.stop();
    let reply = chilpa_core::call_server(&chilpa_core::Call { method: "GET", url: &url, headers: &[], body: None, timeout: Duration::from_secs(3), max_bytes: 1 }).unwrap();
    assert_eq!(reply.status, 404);
    let origin = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let address = origin.server_addr().to_ip().unwrap();
    let original = bytes.clone();
    let origin_thread = std::thread::spawn(move || {
        let mut count = 0;
        while let Some(request) = origin.recv_timeout(Duration::from_secs(3)).unwrap() {
            assert!(request.headers().iter().any(|h| h.field.equiv("Authorization") && h.value.as_str() == "Bearer fixture-only"));
            let value = request.headers().iter().find(|h| h.field.equiv("Range")).unwrap().value.as_str();
            let (start, length) = chilpa_cast::media_server::range(value, original.len() as u64).unwrap();
            let header = tiny_http::Header::from_bytes("Content-Range", format!("bytes {start}-{}/{}", start + length - 1, original.len())).unwrap();
            request.respond(tiny_http::Response::from_data(original[start as usize..(start + length) as usize].to_vec()).with_status_code(206).with_header(header)).unwrap();
            count += 1;
        }
        count
    });
    chilpa_core::remote::install(Arc::new(Opener(format!("http://{address}/private"), cover.clone())));
    let entry = chilpa_models::RemoteEntry::new(chilpa_models::SourceId::new("fixture", "instance"), "track").to_entry();
    api.play_file(std::path::Path::new(&entry), "file:capture", false);
    wait(&mut api, |n| n.title == "Remote fixture" && n.seconds > 0.0);
    assert_eq!(renderer.state.lock().unwrap().bytes, bytes);
    assert!(!renderer.state.lock().unwrap().uri.contains("private"));
    assert_eq!(api.now_playing().artwork_path, cover.display().to_string());
    assert_eq!(api.now_playing().artist, "Fixture artist");
    assert_eq!(api.now_playing().album, "Fixture album");
    renderer.state.lock().unwrap().duration = 30;
    wait(&mut api, |n| n.total_seconds == 30.0);
    assert!(origin_thread.join().unwrap() >= 2);
    // I verify original-byte delivery and MIME selection, not decoder support.
    for (extension, mime) in [("AIFF", "audio/aiff"), ("dsf", "audio/x-dsf"), ("dff", "audio/x-dff"),
        ("ape", "audio/x-ape"), ("alac", "audio/mp4"), ("oga", "audio/ogg"), ("opus", "audio/opus"), ("unknown", "application/octet-stream")] {
        let original = b"opaque original fixture";
        let path = dir.join(format!("original.{extension}")); std::fs::write(&path, original).unwrap();
        let loads = renderer.state.lock().unwrap().loads;
        api.play_file(&path, "file:capture", false);
        wait(&mut api, |n| n.playing && n.title == "original" && renderer.state.lock().unwrap().loads > loads);
        let state = renderer.state.lock().unwrap();
        assert_eq!(state.bytes, original);
        assert_eq!(state.mime, mime);
    }
    api.cast_disconnect(); assert!(api.cast_destination().is_none()); assert!(!api.poll().playing);
    exercise_cast_receiver(&mut api, &path, &second, &bytes);
    api.cast_connect_device(renderer.device.clone());
    api.play_file(&path, "file:capture", false);
    wait(&mut api, |n| n.playing && n.title == "first" && n.seconds > 0.0);
    renderer.state.lock().unwrap().stop_delay = true;
    api.cast_disconnect();
    api.play_file(&path, "file:capture", false);
    let capture = chilpa_core::capture_path();
    assert!(!capture.exists(), "the local output opened before renderer teardown");
    wait(&mut api, |_| capture.exists());
    assert!(!renderer.state.lock().unwrap().playing);
    drop(api); drop(renderer); let _ = std::fs::remove_dir_all(dir);
}

struct Opener(String, std::path::PathBuf);
impl chilpa_core::remote::RemoteOpener for Opener {
    fn locate(&self, _: &chilpa_models::RemoteEntry) -> std::result::Result<chilpa_core::remote::Located, String> {
        Ok(chilpa_core::remote::Located { playback: None, request: chilpa_core::RangedRequest { url: self.0.clone(), headers: vec![("Authorization".into(), "Bearer fixture-only".into())] }, extension: Some("wav".into()), server: "Fixture".into() })
    }
    fn describe(&self, _: &chilpa_models::RemoteEntry) -> chilpa_core::remote::Described {
        chilpa_core::remote::Described { tags: chilpa_tags::Tags { title: Some("Remote fixture".into()), artists: vec!["Fixture artist".into()], album: Some("Fixture album".into()), ..Default::default() }, artwork: Some(self.1.clone()) }
    }
}

#[path = "../../chilpa-cast/src/cast_wire.rs"]
mod cast_wire;
use chilpa_cast::{Error, Result};
fn exercise_cast_receiver(api: &mut Chilpa, first: &std::path::Path, second: &std::path::Path, bytes: &[u8]) {
    use serde_json::json;
    let identity = native_tls::Identity::from_pkcs8(include_bytes!("../../chilpa-cast/tests/fixtures/cast-test.pem"), include_bytes!("../../chilpa-cast/tests/fixtures/cast-test.key")).unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); let address = listener.local_addr().unwrap();
    let shared = Arc::new(Mutex::new(State::default())); let state = shared.clone();
    let thread = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap(); socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut stream = acceptor.accept(socket).unwrap();
        loop {
            let (sender, namespace, request) = cast_wire::receive(&mut stream).unwrap();
            let kind = request["type"].as_str().unwrap();
            if kind == "CLOSE" { break; }
            if matches!(kind, "PING" | "PONG" | "CONNECT") { continue; }
            let mut state = state.lock().unwrap();
            let mut response = if namespace.ends_with("receiver") {
                if kind == "SET_VOLUME" {
                    if let Some(v) = request["volume"]["level"].as_f64() { state.volume = (v * 100.0).round() as u8; }
                    if let Some(v) = request["volume"]["muted"].as_bool() { state.muted = v; }
                }
                json!({"type":"RECEIVER_STATUS","status":{"volume":{"level":f64::from(state.volume)/100.0,"muted":state.muted},"applications":[{"appId":"CC1AD845","transportId":"transport","sessionId":"app"}]}})
            } else {
                match kind {
                    "LOAD" => {
                        state.uri = request["media"]["contentId"].as_str().unwrap().into();
                        state.loads += 1; state.finish = false; state.playing = true; state.paused = false;
                        let reply = chilpa_core::call_server(&chilpa_core::Call { method: "GET", url: &state.uri, headers: &[], body: None, timeout: Duration::from_secs(3), max_bytes: 1_000_000 }).unwrap();
                        assert_eq!(reply.status, 200); state.mime = reply.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case("content-type")).unwrap().1.clone(); state.bytes = reply.body;
                    }
                    "PLAY" => state.paused = false,
                    "PAUSE" => state.paused = true,
                    "SEEK" => state.seeks += 1,
                    "STOP" => state.playing = false,
                    "GET_STATUS" => {},
                    _ => panic!("unexpected Cast command"),
                }
                json!({"type":"MEDIA_STATUS","status":[{"mediaSessionId":state.loads,"playerState":if state.finish || !state.playing {"IDLE"} else if state.paused {"PAUSED"} else {"PLAYING"},"idleReason":if state.finish {"FINISHED"} else {"CANCELLED"},"currentTime":9,"media":{"duration":10}}]})
            };
            response["requestId"] = request["requestId"].clone();
            cast_wire::send(&mut stream, "receiver-0", &sender, &namespace, &response).unwrap();
        }
    });
    api.cast_connect_device(CastDevice { id: "cast-test".into(), name: "Silent Cast".into(), model: "Test".into(), kind: CastKind::Chromecast, address, description: String::new() });
    api.play_tracks(&[first.display().to_string(), second.display().to_string()], "file:capture", false);
    wait(api, |n| n.title == "first" && n.seconds > 0.0); assert_eq!(shared.lock().unwrap().bytes, bytes);
    api.set_paused(true); wait(api, |n| n.paused); api.set_paused(false); wait(api, |n| n.playing && !n.paused);
    api.seek(3.0); api.set_player_volume(31); api.set_player_muted(true);
    wait(api, |_| { let s = shared.lock().unwrap(); s.seeks > 0 && s.volume == 31 && s.muted });
    shared.lock().unwrap().finish = true;
    wait(api, |n| n.index == 1 && n.title == "second");
    api.previous_track("file:capture", false); wait(api, |n| n.title == "first");
    api.next_track(); wait(api, |n| n.title == "second");
    api.cast_disconnect();
    wait(api, |_| !shared.lock().unwrap().uri.is_empty());
    // Shutdown joins the facade worker before the test receiver is joined.
    let until = Instant::now() + Duration::from_secs(8);
    while !thread.is_finished() { api.poll(); assert!(Instant::now() < until, "receiver was not closed"); std::thread::sleep(Duration::from_millis(30)); }
    thread.join().unwrap();
}
