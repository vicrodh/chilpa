use chilpa_cast::{chromecast::Chromecast, Metadata, RemoteTransport, State};
use serde_json::json;
use std::{net::TcpListener, time::Duration};
#[path = "../src/cast_wire.rs"]
mod wire;
// The shared wire module uses the crate's error type.
use chilpa_cast::{Error, Result};

#[test]
fn self_signed_receiver_loads_controls_reports_and_closes_without_audio() {
    let cert = include_bytes!("fixtures/cast-test.pem");
    let key = include_bytes!("fixtures/cast-test.key");
    let identity = native_tls::Identity::from_pkcs8(cert, key).unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
        let mut stream = acceptor.accept(socket).unwrap();
        let mut calls = Vec::new();
        loop {
            let (sender, namespace, request) = wire::receive(&mut stream).unwrap();
            let kind = request["type"].as_str().unwrap();
            calls.push(kind.to_string());
            if kind == "CLOSE" { break; }
            if matches!(kind, "CONNECT" | "PONG" | "PING") { continue; }
            let mut response = if namespace.ends_with("receiver") {
                json!({"type":"RECEIVER_STATUS","status":{"volume":{"level":0.42,"muted":true},"applications":[{"appId":"CC1AD845","sessionId":"app-session","transportId":"transport-1"}]}})
            } else {
                if kind == "LOAD" {
                    assert_eq!(request["media"]["contentType"], if calls.iter().filter(|name| *name == "LOAD").count() == 1 { "audio/flac" } else { "audio/opus" });
                    assert_eq!(request["media"]["contentId"], "http://127.0.0.1/audio/test/1");
                    assert_eq!(request["media"]["metadata"]["title"], "Fixture");
                } else { assert_eq!(request["mediaSessionId"], 42); }
                json!({"type":"MEDIA_STATUS","status":[{"mediaSessionId":42,"playerState":"PLAYING","currentTime":83,"media":{"duration":300}}]})
            };
            response["requestId"] = request["requestId"].clone();
            wire::send(&mut stream, "receiver-0", &sender, "urn:x-cast:com.google.cast.tp.heartbeat", &json!({"type":"PING"})).unwrap();
            wire::send(&mut stream, "receiver-0", &sender, &namespace, &response).unwrap();
        }
        calls
    });
    let mut cast = Chromecast::connect(addr).unwrap();
    cast.load("http://127.0.0.1/audio/test/1", "audio/flac", &Metadata { title: "Fixture".into(), ..Default::default() }).unwrap();
    cast.load("http://127.0.0.1/audio/test/1", "audio/opus", &Metadata { title: "Fixture".into(), ..Default::default() }).unwrap();
    cast.pause().unwrap(); cast.play().unwrap(); cast.seek(83.0).unwrap(); cast.volume(0.42).unwrap(); cast.mute(true).unwrap();
    let status = cast.status().unwrap();
    assert_eq!(status.state, State::Playing); assert_eq!(status.position, Some(83.0)); assert_eq!(status.duration, Some(300.0)); assert_eq!(status.volume, Some(0.42)); assert_eq!(status.muted, Some(true));
    cast.stop().unwrap(); cast.disconnect().unwrap();
    let calls = worker.join().unwrap();
    for name in ["LAUNCH", "LOAD", "PLAY", "PAUSE", "SEEK", "SET_VOLUME", "GET_STATUS", "STOP", "CLOSE", "PONG"] { assert!(calls.contains(&name.to_string()), "missing {name}"); }
}
#[test]
fn malformed_cast_frames_are_bounded() {
    assert!(wire::receive(&mut std::io::Cursor::new(u32::MAX.to_be_bytes())).is_err());
    assert!(wire::receive(&mut std::io::Cursor::new([0,0,0,3,18,255,255])).is_err());
}
