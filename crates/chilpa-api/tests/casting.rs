//! A silent renderer fetches the original bytes through the facade.
use chilpa_api::{CastDevice, CastKind, Chilpa};
use std::{io::Write, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};

#[derive(Default)]
struct State { uri: String, playing: bool, paused: bool, finish: bool, loads: usize, bytes: Vec<u8>, seeks: usize, volume: u8, muted: bool }
struct Renderer { device: CastDevice, state: Arc<Mutex<State>>, done: Arc<AtomicBool>, thread: Option<std::thread::JoinHandle<()>> }
impl Renderer {
    fn new() -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap(); let address = server.server_addr().to_ip().unwrap();
        let state = Arc::new(Mutex::new(State::default())); let shared = state.clone();
        let done = Arc::new(AtomicBool::new(false)); let stopped = done.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                let Some(mut request) = server.recv_timeout(Duration::from_millis(50)).unwrap() else { continue };
                let content = if request.method() == &tiny_http::Method::Get {
                    "<root><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Silent renderer</friendlyName><UDN>uuid:silent</UDN><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:3</serviceType><controlURL>/transport</controlURL></service><service><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType><controlURL>/volume</controlURL></service></serviceList></device></root>".to_string()
                } else {
                    let action = request.headers().iter().find(|h| h.field.equiv("SOAPAction")).unwrap().value.as_str().trim_matches('"').rsplit('#').next().unwrap().to_string();
                    let mut body = String::new(); request.as_reader().read_to_string(&mut body).unwrap();
                    let field = |name: &str| body.split(&format!("<{name}>")).nth(1).and_then(|s| s.split(&format!("</{name}>")).next()).unwrap_or("").to_string();
                    let mut state = shared.lock().unwrap();
                    match action.as_str() {
                        "SetAVTransportURI" => { state.uri = field("CurrentURI"); state.loads += 1; state.finish = false; "".into() }
                        "Play" => {
                            state.playing = true; state.paused = false;
                            let reply = chilpa_core::call_server(&chilpa_core::Call { method: "GET", url: &state.uri, headers: &[], body: None, timeout: Duration::from_secs(3), max_bytes: 1_000_000 }).unwrap();
                            assert_eq!(reply.status, 200); state.bytes = reply.body; "".into()
                        }
                        "Pause" => { state.paused = true; "".into() }
                        "Stop" => { state.playing = false; "".into() }
                        "Seek" => { state.seeks += 1; "".into() }
                        "SetVolume" => { state.volume = field("DesiredVolume").parse().unwrap(); "".into() }
                        "SetMute" => { state.muted = field("DesiredMute") == "1"; "".into() }
                        "GetVolume" => format!("<CurrentVolume>{}</CurrentVolume>", state.volume),
                        "GetMute" => format!("<CurrentMute>{}</CurrentMute>", u8::from(state.muted)),
                        "GetPositionInfo" => "<RelTime>00:00:09</RelTime><TrackDuration>00:00:10</TrackDuration>".into(),
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
fn wait(api: &mut Chilpa, test: impl Fn(&chilpa_api::NowPlaying) -> bool) {
    let until = Instant::now() + Duration::from_secs(18);
    loop { let now = api.poll(); if test(now) { return; } assert!(Instant::now() < until, "cast did not reach expected state: {}", now.error); std::thread::sleep(Duration::from_millis(40)); }
}
#[test]
fn facade_casts_queue_controls_and_remote_originals_without_local_playback() {
    let dir = std::env::temp_dir().join(format!("chilpa-cast-smoke-{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap();
    for key in ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"] { std::env::set_var(key, dir.join(key)); }
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
    api.set_paused(true); wait(&mut api, |n| n.paused);
    api.set_paused(false); wait(&mut api, |n| !n.paused && n.playing);
    api.seek(4.0); api.set_player_volume(42); api.set_player_muted(true);
    wait(&mut api, |_| { let s = renderer.state.lock().unwrap(); s.seeks > 0 && s.volume == 42 && s.muted });
    renderer.state.lock().unwrap().finish = true;
    wait(&mut api, |n| n.index == 1 && n.title == "second");
    api.previous_track("file:capture", false); wait(&mut api, |n| n.index == 0 && n.title == "first");
    api.next_track(); wait(&mut api, |n| n.index == 1 && n.title == "second");
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
    chilpa_core::remote::install(Arc::new(Opener(format!("http://{address}/private"))));
    let entry = chilpa_models::RemoteEntry::new(chilpa_models::SourceId::new("fixture", "instance"), "track").to_entry();
    api.play_file(std::path::Path::new(&entry), "file:capture", false);
    wait(&mut api, |n| n.title == "Remote fixture" && n.seconds > 0.0);
    assert_eq!(renderer.state.lock().unwrap().bytes, bytes);
    assert!(!renderer.state.lock().unwrap().uri.contains("private"));
    assert!(origin_thread.join().unwrap() >= 2);
    let unsupported = dir.join("unsupported.dsf"); std::fs::write(&unsupported, b"DSD ").unwrap();
    let loads = renderer.state.lock().unwrap().loads;
    api.play_file(&unsupported, "file:capture", false);
    wait(&mut api, |n| !n.error.is_empty());
    assert_eq!(renderer.state.lock().unwrap().loads, loads, "an unsupported format was handed to the renderer");
    api.cast_disconnect(); assert!(api.cast_destination().is_none()); assert!(!api.poll().playing);
    exercise_cast_receiver(&mut api, &path, &second, &bytes);
    drop(api); drop(renderer); let _ = std::fs::remove_dir_all(dir);
}

struct Opener(String);
impl chilpa_core::remote::RemoteOpener for Opener {
    fn locate(&self, _: &chilpa_models::RemoteEntry) -> std::result::Result<chilpa_core::remote::Located, String> {
        Ok(chilpa_core::remote::Located { request: chilpa_core::RangedRequest { url: self.0.clone(), headers: vec![("Authorization".into(), "Bearer fixture-only".into())] }, extension: Some("wav".into()), server: "Fixture".into() })
    }
    fn describe(&self, _: &chilpa_models::RemoteEntry) -> chilpa_core::remote::Described {
        chilpa_core::remote::Described { tags: chilpa_tags::Tags { title: Some("Remote fixture".into()), ..Default::default() }, artwork: None }
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
                        assert_eq!(reply.status, 200); state.bytes = reply.body;
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
