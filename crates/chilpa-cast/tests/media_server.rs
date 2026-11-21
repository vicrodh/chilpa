use chilpa_cast::{CONTENT_FEATURES, Error, media_server::{ExternalAsset, MediaServer, RangeSource, range}};
use std::{io::{self, Read, Write}, net::TcpStream, sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}, mpsc}, time::Duration};

// Serialize listeners so the preferred-port assertion cannot race another test.
static SERVERS: Mutex<()> = Mutex::new(());
struct Bytes(Vec<u8>);
impl RangeSource for Bytes {
    fn open(&self, start: u64, length: u64) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(io::Cursor::new(self.0[start as usize..(start + length) as usize].to_vec())))
    }
}
fn asset(bytes: Vec<u8>) -> ExternalAsset {
    ExternalAsset { length: bytes.len() as u64, mime: "audio/flac".into(), source: Arc::new(Bytes(bytes)) }
}
fn path(url: &str) -> String {
    url.split_once("/audio/").map(|(_, p)| format!("/audio/{p}")).unwrap_or_else(|| url.into())
}
fn connect(server: &MediaServer) -> TcpStream {
    let socket = TcpStream::connect(server.address()).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    socket.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
    socket
}
fn send(socket: &mut TcpStream, url: &str, method: &str, headers: &str) {
    write!(socket, "{method} {} HTTP/1.1\r\nHost: localhost\r\n{headers}\r\n", path(url)).unwrap();
}
fn read_headers(socket: &mut TcpStream) -> String {
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        socket.read_exact(&mut byte).unwrap();
        headers.push(byte[0]);
        assert!(headers.len() < 16384);
    }
    String::from_utf8(headers).unwrap()
}
fn field<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.trim())
}
fn answer(socket: &mut TcpStream, method: &str) -> (String, Vec<u8>) {
    let headers = read_headers(socket);
    let length = if method == "HEAD" { 0 } else { field(&headers, "Content-Length").unwrap().parse().unwrap() };
    let mut body = vec![0; length];
    socket.read_exact(&mut body).unwrap();
    (headers, body)
}
fn request(server: &MediaServer, url: &str, method: &str, headers: &str) -> (String, Vec<u8>) {
    let mut socket = connect(server);
    send(&mut socket, url, method, &format!("Connection: close\r\n{headers}"));
    let result = answer(&mut socket, method);
    let mut extra = Vec::new();
    socket.read_to_end(&mut extra).unwrap();
    assert!(extra.is_empty(), "unexpected response body: {extra:?}");
    result
}
fn media_headers(headers: &str, status: u16, length: usize, partial: Option<&str>) {
    assert!(headers.starts_with(&format!("HTTP/1.1 {status}")), "{headers}");
    assert_eq!(field(headers, "Content-Type"), Some("audio/flac"));
    assert_eq!(field(headers, "Content-Length"), Some(length.to_string().as_str()));
    assert_eq!(field(headers, "Accept-Ranges"), Some("bytes"));
    assert_eq!(field(headers, "transferMode.dlna.org"), Some("Streaming"));
    assert_eq!(field(headers, "contentFeatures.dlna.org"), Some(CONTENT_FEATURES));
    assert_eq!(field(headers, "Content-Range"), partial);
    assert_eq!(field(headers, "Server"), Some("tiny-http (Rust)"));
    assert!(!field(headers, "Date").unwrap().is_empty());
    assert_eq!(field(headers, "Transfer-Encoding"), None);
    let mut expected = vec!["Server", "Date", "Content-Type", "Accept-Ranges", "transferMode.dlna.org", "contentFeatures.dlna.org"];
    if partial.is_some() { expected.push("Content-Range"); }
    expected.push("Content-Length");
    let actual: Vec<_> = headers.lines().skip(1).filter_map(|line| line.split_once(':').map(|(key, _)| key)).collect();
    assert_eq!(actual, expected);
}
#[test]
fn ranges_cover_closed_open_suffix_and_invalid_inputs() {
    for (input, expected) in [("bytes=1-3", Some((1,3))), ("bytes=8-", Some((8,2))), ("bytes=-3", Some((7,3))), ("bytes=0-999", Some((0,10))), ("bytes=10-", None), ("bytes=5-2", None), ("bytes=-0", None), ("bytes=0-1,4-5", None), ("items=0-1", None), ("bytes=0-18446744073709551616", None)] { assert_eq!(range(input, 10), expected, "{input}"); }
}
#[test]
fn media_responses_match_header_order_and_never_use_chunked_transfer() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    assert!(server.address().ip().is_loopback());
    let bytes: Vec<u8> = (0..131072).map(|i| (i % 251) as u8).collect();
    let url = server.publish(asset(bytes.clone())).unwrap();
    let (headers, body) = request(&server, &url, "GET", "");
    media_headers(&headers, 200, bytes.len(), None);
    assert_eq!(body, bytes);
    let (headers, body) = request(&server, &url, "HEAD", "Range: bytes=1-3\r\n");
    media_headers(&headers, 200, bytes.len(), None);
    assert!(body.is_empty());
    for (header, from, end) in [("bytes=1-65536",1,65537), ("bytes=131069-",131069,131072), ("bytes=-3",131069,131072)] {
        let (headers, body) = request(&server, &url, "GET", &format!("Range: {header}\r\n"));
        media_headers(&headers, 206, end - from, Some(&format!("bytes {from}-{}/131072", end - 1)));
        assert_eq!(body, bytes[from..end]);
    }
}
#[test]
fn rejected_ranges_serve_the_full_body() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    let url = server.publish(asset(vec![1,2,3])).unwrap();
    for value in ["bytes=999999-", "bytes=-0", "bytes=0-1,4-5", "items=0-1", "bytes=0-18446744073709551616"] {
        let (headers, body) = request(&server, &url, "GET", &format!("Range: {value}\r\n"));
        assert!(headers.starts_with("HTTP/1.1 200"), "{value}: {headers}");
        assert_eq!(body, [1,2,3]);
    }
}
#[test]
fn numeric_ids_token_errors_and_latest_publication() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    let url = server.publish(asset(vec![42])).unwrap();
    let path = path(&url);
    let parts: Vec<_> = path.split('/').collect();
    assert_eq!(parts[2].len(), 32);
    assert!(parts[2].chars().all(|c| c.is_ascii_hexdigit()));
    assert!(parts[3].parse::<u64>().is_ok());
    assert!(request(&server, "/audio/wrong/1", "GET", "").0.starts_with("HTTP/1.1 403"));
    assert!(request(&server, &format!("/audio/{}/999999", parts[2]), "GET", "").0.starts_with("HTTP/1.1 404"));
    for malformed in ["/audio/1", "/audio/token/notanum", "/other/token/1"] {
        assert!(request(&server, malformed, "GET", "").0.starts_with("HTTP/1.1 404"));
    }
    assert!(request(&server, &url, "POST", "").0.starts_with("HTTP/1.1 405"));
    let next = server.publish(asset(vec![43])).unwrap();
    assert_ne!(url, next);
    assert!(request(&server, &url, "GET", "").0.starts_with("HTTP/1.1 404"));
    server.revoke();
    assert!(request(&server, &next, "GET", "").0.starts_with("HTTP/1.1 404"));
}
#[test]
fn head_then_get_share_one_keep_alive_connection() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    let url = server.publish(asset(vec![1,2,3])).unwrap();
    let mut socket = connect(&server);
    send(&mut socket, &url, "HEAD", "Connection: keep-alive\r\n");
    let (headers, body) = answer(&mut socket, "HEAD");
    assert!(headers.starts_with("HTTP/1.1 200"));
    assert!(body.is_empty());
    send(&mut socket, &url, "GET", "Connection: close\r\n");
    let (headers, body) = answer(&mut socket, "GET");
    assert!(headers.starts_with("HTTP/1.1 200"));
    assert_eq!(body, [1,2,3]);
}
#[test]
fn query_strings_do_not_change_the_resource() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    let url = server.publish(asset(vec![1,2,3])).unwrap();
    let (headers, body) = request(&server, &format!("{url}?query=1"), "GET", "");
    assert!(headers.starts_with("HTTP/1.1 200"));
    assert_eq!(body, [1,2,3]);
}
#[test]
fn a_peer_other_than_the_renderer_can_fetch_media() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.2:9".parse().unwrap()).unwrap();
    let url = server.publish(asset(vec![1,2,3])).unwrap();
    let mut socket = connect(&server);
    assert_eq!(socket.local_addr().unwrap().ip().to_string(), "127.0.0.1");
    send(&mut socket, &url, "GET", "Connection: close\r\n");
    assert_eq!(answer(&mut socket, "GET").1, [1,2,3]);
}
#[test]
fn preferred_port_falls_back_while_another_server_holds_it() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let first = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    assert_eq!(first.address().port(), 9876);
    let second = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    assert_ne!(second.address().port(), 9876);
    let url = second.publish(asset(vec![42])).unwrap();
    assert_eq!(request(&second, &url, "GET", "").1, [42]);
}
#[test]
fn a_new_generation_revokes_before_the_worker_can_answer() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let generation = Arc::new(AtomicU64::new(1));
    let server = MediaServer::start_guarded("127.0.0.1:9".parse().unwrap(), Some(generation.clone())).unwrap();
    let url = server.publish(asset(vec![42])).unwrap();
    generation.store(2, Ordering::Release);
    assert!(request(&server, &url, "GET", "").0.starts_with("HTTP/1.1 404"));
    let next = server.publish(asset(vec![43])).unwrap();
    assert_eq!(request(&server, &next, "GET", "").1, [43]);
}
#[test]
fn ipv6_loopback_serves_original_bytes() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("[::1]:9".parse().unwrap()).unwrap();
    assert!(server.address().is_ipv6());
    let url = server.publish(asset(vec![1,2,3])).unwrap();
    assert!(url.starts_with("http://[::1]:"));
    assert_eq!(request(&server, &url, "GET", "").1, [1,2,3]);
}

// Block the second read after the first body bytes have reached the client.
// Cancellation must discard even bytes returned by an already blocked read.
struct GatedSource { entered: mpsc::Sender<usize>, release: Mutex<Option<mpsc::Receiver<()>>> }
struct GatedReader { first_length: Option<usize>, entered: mpsc::Sender<usize>, release: mpsc::Receiver<()> }
impl RangeSource for GatedSource {
    fn open(&self, _: u64, _: u64) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(GatedReader { first_length: None, entered: self.entered.clone(), release: self.release.lock().unwrap().take().unwrap() }))
    }
}
impl Read for GatedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if let Some(length) = self.first_length {
            self.entered.send(length).unwrap();
            if self.release.recv_timeout(Duration::from_secs(5)).is_err() { return Ok(0); }
        } else { self.first_length = Some(output.len()); }
        output.fill(42);
        Ok(output.len())
    }
}
fn streaming_cancel(change_generation: bool, replace: bool) {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let generation = Arc::new(AtomicU64::new(1));
    let server = MediaServer::start_guarded("127.0.0.1:9".parse().unwrap(), Some(generation.clone())).unwrap();
    let (entered, blocked) = mpsc::channel();
    let (release, waiting) = mpsc::channel();
    let url = server.publish(ExternalAsset { length: 1024 * 1024, mime: "audio/flac".into(), source: Arc::new(GatedSource { entered, release: Mutex::new(Some(waiting)) }) }).unwrap();
    let mut socket = connect(&server);
    send(&mut socket, &url, "GET", "Connection: close\r\n");
    assert!(read_headers(&mut socket).starts_with("HTTP/1.1 200"));
    let mut first = [0];
    socket.read_exact(&mut first).unwrap();
    assert_eq!(first, [42]);
    let first_length = blocked.recv_timeout(Duration::from_secs(3)).unwrap();
    if replace { server.publish(asset(vec![43])).unwrap(); }
    if change_generation { generation.store(2, Ordering::Release); } else { server.revoke(); }
    assert!(request(&server, &url, "GET", "").0.starts_with("HTTP/1.1 404"));
    release.send(()).unwrap();
    let mut rest = Vec::new();
    socket.read_to_end(&mut rest).unwrap();
    assert_eq!(rest.len() + 1, first_length, "cancelled reader leaked another body chunk");
}
#[test]
fn revoke_stops_an_in_flight_body() { streaming_cancel(false, false); }
#[test]
fn a_new_generation_stops_an_in_flight_body() { streaming_cancel(true, false); }
#[test]
fn revoke_also_stops_an_evicted_in_flight_body() { streaming_cancel(false, true); }

struct FailedSource { panic: bool }
impl RangeSource for FailedSource {
    fn open(&self, _: u64, _: u64) -> io::Result<Box<dyn Read + Send>> {
        assert!(!self.panic, "source open failed");
        Err(io::Error::other("source unavailable"))
    }
}
#[test]
fn source_errors_and_panics_answer_500_and_head_never_opens_the_source() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    for panic in [false, true] {
        let url = server.publish(ExternalAsset { length: 3, mime: "audio/flac".into(), source: Arc::new(FailedSource { panic }) }).unwrap();
        assert!(request(&server, &url, "HEAD", "").0.starts_with("HTTP/1.1 200"));
        assert!(request(&server, &url, "GET", "").0.starts_with("HTTP/1.1 500"));
    }
}
#[test]
fn publish_rejects_empty_assets_and_header_injection() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    assert!(matches!(server.publish(asset(vec![])), Err(Error::Asset)));
    for mime in ["audio/flac\rInjected: yes", "audio/flac\nInjected: yes"] {
        let mut invalid = asset(vec![42]);
        invalid.mime = mime.into();
        assert!(matches!(server.publish(invalid), Err(Error::Asset)));
    }
}

struct OpenGate { ready: Mutex<bool>, changed: std::sync::Condvar }
impl Drop for OpenGateRelease {
    fn drop(&mut self) {
        *self.0.ready.lock().unwrap() = true;
        self.0.changed.notify_all();
    }
}
struct OpenGateRelease(Arc<OpenGate>);
struct WaitingSource { gate: Arc<OpenGate>, entered: mpsc::Sender<()> }
impl RangeSource for WaitingSource {
    fn open(&self, _: u64, _: u64) -> io::Result<Box<dyn Read + Send>> {
        self.entered.send(()).unwrap();
        let (ready, _) = self.gate.changed.wait_timeout_while(self.gate.ready.lock().unwrap(), Duration::from_secs(5), |ready| !*ready).unwrap();
        if !*ready { return Err(io::Error::new(io::ErrorKind::TimedOut, "test gate timed out")); }
        Ok(Box::new(io::Cursor::new(vec![42])))
    }
}
#[test]
fn five_blocked_requests_do_not_prevent_a_head_probe() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    let gate = Arc::new(OpenGate { ready: Mutex::new(false), changed: std::sync::Condvar::new() });
    let release = OpenGateRelease(gate.clone());
    let (entered, waiting) = mpsc::channel();
    let url = server.publish(ExternalAsset { length: 1, mime: "audio/flac".into(), source: Arc::new(WaitingSource { gate, entered }) }).unwrap();
    let mut sockets = Vec::new();
    for _ in 0..5 {
        let mut socket = connect(&server);
        send(&mut socket, &url, "GET", "Connection: close\r\n");
        sockets.push(socket);
        waiting.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    assert!(request(&server, &url, "HEAD", "").0.starts_with("HTTP/1.1 200"));
    drop(release);
    for mut socket in sockets { assert_eq!(answer(&mut socket, "GET").1, [42]); }
}
#[test]
fn drop_releases_the_listener() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    let address = server.address();
    drop(server);
    assert!(TcpStream::connect(address).is_err());
}
#[test]
fn wire_ranges_preserve_the_reference_parser_and_reader_clamping() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    let url = server.publish(asset((0..10).collect())).unwrap();
    for (value, expected, content_range) in [
        ("bytes=5-2", vec![2], "bytes 5-2/10"),
        ("bytes=bytes=1-3", vec![1,2,3], "bytes 1-3/10"),
        ("bytes=1-3-9", vec![1,2,3], "bytes 1-3/10"),
    ] {
        let (headers, body) = request(&server, &url, "GET", &format!("Range: {value}\r\n"));
        assert!(headers.starts_with("HTTP/1.1 206"));
        assert_eq!(field(&headers, "Content-Range"), Some(content_range));
        assert_eq!(body, expected);
    }
}
