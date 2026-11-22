//! The installed signer reaches requests through the real agents: a plugin's
//! request, Chilpa's own fetch, and a stream. This file is its own test
//! process, so the process-wide signer it installs touches no other test.
use chilpa_stream::{network::{sign_with, Signer}, *};
use std::{io::{Read, Write}, net::{TcpListener, TcpStream}, sync::Mutex, time::{Duration, Instant}};

static ANSWERED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Signs the paths of this machine that start with /signed.
struct ThisMachine;

impl Signer for ThisMachine {
    fn sign(&self, method: &str, host: &str, path: &str) -> Vec<(&'static str, String)> {
        if host != "127.0.0.1" || !path.starts_with("/signed") { return Vec::new(); }
        vec![("authorization", format!("Test {method} {path}"))]
    }
    fn answered(&self, host: &str, _headers: &ureq::http::HeaderMap) {
        ANSWERED.lock().unwrap().push(host.to_string());
    }
}

/// The request's head, read until the blank line that ends it.
fn head_of(peer: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    while !head.windows(4).any(|end| end == b"\r\n\r\n") && head.len() < 16 * 1024 {
        match peer.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => head.extend_from_slice(&chunk[..n]),
        }
    }
    String::from_utf8_lossy(&head).to_ascii_lowercase()
}

/// Answers `count` connections with a short audio reply and hands back each head.
fn answering(count: usize) -> (u16, std::thread::JoinHandle<Vec<String>>) {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let port = server.local_addr().unwrap().port();
    let asked = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut asked = Vec::new();
        while asked.len() < count {
            let mut peer = match server.accept() {
                Ok((peer, _)) => peer,
                Err(_) if Instant::now() < deadline => { std::thread::sleep(Duration::from_millis(5)); continue; }
                Err(_) => break,
            };
            peer.set_nonblocking(false).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            asked.push(head_of(&mut peer));
            peer.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 4\r\nConnection: close\r\n\r\nsong").unwrap();
        }
        asked
    });
    (port, asked)
}

#[test]
fn the_installed_signer_signs_plugin_requests_fetches_and_streams() {
    assert!(sign_with(Box::new(ThisMachine)), "the first signer is installed");
    assert!(!sign_with(Box::new(ThisMachine)), "and a second is refused");

    let (port, asked) = answering(4);
    let url = |path: &str| format!("http://127.0.0.1:{port}{path}");
    let request = serde_json::json!({"url": url("/signed/plugin"), "method": "GET"});
    let reply: serde_json::Value = serde_json::from_slice(&send_request(request.to_string().as_bytes(), None)).unwrap();
    assert_eq!(reply["status"], 200, "{reply}");
    let fetched = fetch(&Fetch { url: &url("/signed/fetch"), user_agent: "fixture", accept: "*/*",
        timeout: Duration::from_secs(5), max_bytes: 64, authorization: None }).unwrap();
    assert_eq!(fetched.status, 200);
    drop(open(&url("/signed/stream")).unwrap());
    let request = serde_json::json!({"url": url("/unsigned"), "method": "GET"});
    send_request(request.to_string().as_bytes(), None);
    let asked = asked.join().unwrap();
    assert_eq!(asked.len(), 4, "every request arrived");

    assert!(asked[0].contains("authorization: test get /signed/plugin\r\n"), "{}", asked[0]);
    assert!(asked[1].contains("authorization: test get /signed/fetch\r\n"), "{}", asked[1]);
    assert!(asked[2].contains("authorization: test get /signed/stream\r\n"), "{}", asked[2]);
    assert!(!asked[3].contains("authorization"), "{}", asked[3]);
    assert_eq!(*ANSWERED.lock().unwrap(), ["127.0.0.1"; 3], "each signed reply is read, the unsigned one is not");
}
