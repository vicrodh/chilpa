use chilpa_cast::media_server::{ExternalAsset, MediaServer, RangeSource, range};
use std::{io::{Read, Write}, net::TcpStream, sync::Arc, time::Duration};
struct Bytes(Vec<u8>);
impl RangeSource for Bytes {
    fn open(&self, start: u64, length: u64) -> std::io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(std::io::Cursor::new(self.0[start as usize..(start + length) as usize].to_vec())))
    }
}
fn request(server: &MediaServer, url: &str, method: &str, range: &str) -> (String, Vec<u8>) {
    let path = url.split_once("/audio/").map(|(_, p)| format!("/audio/{p}")).unwrap_or_else(|| url.into());
    let mut socket = TcpStream::connect(server.address()).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    write!(socket, "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{range}\r\n").unwrap();
    let mut answer = Vec::new(); socket.read_to_end(&mut answer).unwrap();
    let end = answer.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    (String::from_utf8(answer[..end].to_vec()).unwrap(), answer[end..].to_vec())
}
#[test]
fn ranges_cover_closed_open_suffix_and_invalid_inputs() {
    for (input, expected) in [("bytes=1-3", Some((1,3))), ("bytes=8-", Some((8,2))), ("bytes=-3", Some((7,3))), ("bytes=0-999", Some((0,10))), ("bytes=10-", None), ("bytes=5-2", None), ("bytes=-0", None), ("bytes=0-1,4-5", None), ("items=0-1", None), ("bytes=0-18446744073709551616", None)] { assert_eq!(range(input, 10), expected, "{input}"); }
}
#[test]
fn original_bytes_head_ranges_revocation_and_no_chunked_transfer() {
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    assert!(server.address().ip().is_loopback());
    let bytes: Vec<u8> = (0..131072).map(|i| (i % 251) as u8).collect();
    let asset = ExternalAsset { length: bytes.len() as u64, mime: "audio/flac".into(), source: Arc::new(Bytes(bytes.clone())) };
    let url = server.publish(asset.clone()).unwrap();
    let token = url.split("/audio/").nth(1).unwrap().split('/').next().unwrap();
    assert_eq!(token.len(), 32);
    let (headers, body) = request(&server, &url, "GET", "");
    assert!(headers.starts_with("HTTP/1.1 200")); assert!(headers.contains("Content-Length: 131072")); assert!(!headers.to_ascii_lowercase().contains("chunked")); assert_eq!(body, bytes);
    let (headers, body) = request(&server, &url, "HEAD", ""); assert!(body.is_empty()); assert!(headers.contains("Content-Length: 131072"));
    for (header, from, end) in [("bytes=1-3",1,4), ("bytes=131069-",131069,131072), ("bytes=-3",131069,131072)] {
        let (headers, body) = request(&server, &url, "GET", &format!("Range: {header}\r\n")); assert!(headers.starts_with("HTTP/1.1 206")); assert_eq!(body, bytes[from..end]);
    }
    assert!(request(&server, &url, "GET", "Range: bytes=999999-\r\n").0.starts_with("HTTP/1.1 416"));
    assert!(request(&server, &url, "POST", "").0.starts_with("HTTP/1.1 405"));
    assert!(request(&server, "/audio/wrong/track", "GET", "").0.starts_with("HTTP/1.1 404"));
    let next = server.publish(asset).unwrap(); assert_ne!(url, next);
    assert!(request(&server, &url, "GET", "").0.starts_with("HTTP/1.1 404"));
    server.revoke(); assert!(request(&server, &next, "GET", "").0.starts_with("HTTP/1.1 404"));
}

#[test]
fn a_new_generation_revokes_before_the_worker_can_answer() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let generation = Arc::new(AtomicU64::new(1));
    let server = MediaServer::start_guarded("127.0.0.1:9".parse().unwrap(), Some(generation.clone())).unwrap();
    let url = server.publish(ExternalAsset { length: 1, mime: "audio/flac".into(), source: Arc::new(Bytes(vec![42])) }).unwrap();
    generation.store(2, Ordering::Release);
    assert!(request(&server, &url, "GET", "").0.starts_with("HTTP/1.1 404"));
}
