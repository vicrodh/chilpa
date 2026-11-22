use chilpa_cast::{CONTENT_FEATURES, Error, media_server::{extension, range, ranged, ExternalAsset, MediaServer, RangeSource, Ranged}};
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
/// Every line the cast trace says while this test binary runs.
static TRACE: Mutex<Vec<String>> = Mutex::new(Vec::new());
fn sink(scope: &str, event: &str, details: std::fmt::Arguments<'_>) { TRACE.lock().unwrap_or_else(|e| e.into_inner()).push(format!("{scope}.{event} {details}")); }
/// Where the trace stands now: `finished` looks only past it, since every
/// server numbers its bodies from 1.
fn mark() -> usize { TRACE.lock().unwrap_or_else(|e| e.into_inner()).len() }
/// Waits for the `finished` line of body `id` said after `from`.
fn finished(id: &str, from: usize) -> String {
    let want = format!("cast.http.finished id={id} ");
    for _ in 0..200 {
        if let Some(line) = TRACE.lock().unwrap_or_else(|e| e.into_inner()).iter().skip(from).find(|l| l.starts_with(&want)) { return line.clone(); }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no finished line for id {id}");
}
fn id_of(url: &str) -> String { url.rsplit('/').next().unwrap().split('.').next().unwrap().to_string() }
#[test]
fn ranges_are_read_as_rfc_9110_reads_them() {
    use Ranged::{Part, Unsatisfiable, Whole};
    for (input, expected) in [
        ("bytes=1-3", Part { start: 1, length: 3 }), ("bytes=8-", Part { start: 8, length: 2 }), ("bytes=-3", Part { start: 7, length: 3 }),
        ("bytes=0-999", Part { start: 0, length: 10 }), ("bytes=-20", Part { start: 0, length: 10 }), ("bytes=9-9", Part { start: 9, length: 1 }),
        ("Bytes=0-0", Part { start: 0, length: 1 }),
        // Valid, but past the last byte: 416.
        ("bytes=10-", Unsatisfiable), ("bytes=10-20", Unsatisfiable), ("bytes=-0", Unsatisfiable),
        // Invalid, another unit or several ranges: the field is ignored.
        ("bytes=5-2", Whole), ("bytes=0-1,4-5", Whole), ("items=0-1", Whole), ("bytes=0-18446744073709551616", Whole),
        ("bytes=bytes=1-3", Whole), ("bytes=1-3-9", Whole), ("bytes=-", Whole), ("bytes=a-3", Whole), ("bytes=+1-3", Whole), ("bytes 1-3", Whole), ("", Whole),
    ] { assert_eq!(ranged(input, 10), expected, "{input}"); }
    assert_eq!((range("bytes=1-3", 10), range("bytes=10-", 10), range("bytes=5-2", 10)), (Some((1, 3)), None, None));
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
fn an_ignored_range_answers_the_whole_body_and_an_unsatisfiable_one_416() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    let url = server.publish(asset(vec![1,2,3])).unwrap();
    // Never a 206 whose Content-Range disagrees with its bytes.
    for value in ["bytes=5-2", "bytes=0-1,4-5", "items=0-1", "bytes=0-18446744073709551616", "bytes=bytes=1-3", "bytes=1-3-9"] {
        let (headers, body) = request(&server, &url, "GET", &format!("Range: {value}\r\n"));
        media_headers(&headers, 200, 3, None);
        assert_eq!(body, [1,2,3], "{value}");
    }
    for value in ["bytes=999999-", "bytes=3-", "bytes=-0"] {
        let (headers, body) = request(&server, &url, "GET", &format!("Range: {value}\r\n"));
        assert!(headers.starts_with("HTTP/1.1 416"), "{value}: {headers}");
        assert_eq!((field(&headers, "Content-Range"), field(&headers, "Content-Length")), (Some("bytes */3"), Some("0")), "{value}");
        assert!(body.is_empty());
    }
    // A 416 keeps the connection: the next request on it is answered.
    let mut socket = connect(&server);
    send(&mut socket, &url, "GET", "Range: bytes=9-\r\n");
    assert!(answer(&mut socket, "GET").0.starts_with("HTTP/1.1 416"));
    send(&mut socket, &url, "GET", "Connection: close\r\n");
    let (headers, body) = answer(&mut socket, "GET");
    media_headers(&headers, 200, 3, None);
    assert_eq!(body, [1,2,3]);
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
    let (id, format) = parts[3].split_once('.').unwrap();
    assert!(id.parse::<u64>().is_ok());
    assert_eq!(format, "flac");
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
    chilpa_cast::trace::install(sink);
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let generation = Arc::new(AtomicU64::new(1));
    let server = MediaServer::start_guarded("127.0.0.1:9".parse().unwrap(), Some(generation.clone())).unwrap();
    let (entered, blocked) = mpsc::channel();
    let (release, waiting) = mpsc::channel();
    let url = server.publish(ExternalAsset { length: 1024 * 1024, mime: "audio/flac".into(), source: Arc::new(GatedSource { entered, release: Mutex::new(Some(waiting)) }) }).unwrap();
    let from = mark();
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
    assert!(finished(&id_of(&url), from).ends_with("outcome=revoked"), "a revoked body says so");
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
fn the_address_names_the_format_it_serves() {
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    for (mime, format) in [("audio/flac", Some("flac")), ("audio/mpeg", Some("mp3")), ("audio/wav", Some("wav")), ("audio/mp4", Some("m4a")),
        ("audio/aac", Some("aac")), ("audio/x-dsf", Some("dsf")), ("audio/x-dff", Some("dff")), ("application/octet-stream", None)] {
        let url = server.publish(ExternalAsset { length: 3, mime: mime.into(), source: Arc::new(Bytes(vec![1, 2, 3])) }).unwrap();
        let bare = match format {
            Some(format) => { assert!(url.ends_with(&format!(".{format}")), "{mime}: {url}"); url.trim_end_matches(&format!(".{format}")).to_string() }
            None => { assert!(!url.rsplit('/').next().unwrap().contains('.'), "no extension is invented for {mime}: {url}"); url.clone() }
        };
        // The published address and the bare id serve the same bytes.
        for asked in [url.clone(), bare.clone()] {
            let (headers, body) = request(&server, &asked, "GET", "");
            assert!(headers.starts_with("HTTP/1.1 200"), "{asked}: {headers}");
            assert_eq!((field(&headers, "Content-Type"), body.as_slice()), (Some(mime), &[1, 2, 3][..]), "{asked}");
        }
        // Another format's name is not this resource.
        assert!(request(&server, &format!("{bare}.ogg"), "GET", "").0.starts_with("HTTP/1.1 404"), "{mime}");
    }
    assert_eq!((extension("AUDIO/FLAC"), extension("audio/flac; rate=44100"), extension("text/plain")), (Some("flac"), Some("flac"), None));
}

/// Hands over `good` bytes, then fails, as a source cut off upstream.
struct BreakingSource { good: usize }
impl RangeSource for BreakingSource {
    fn open(&self, _: u64, _: u64) -> io::Result<Box<dyn Read + Send>> {
        let good = self.good;
        Ok(Box::new(io::Cursor::new(vec![7u8; good]).chain(Breaks)))
    }
}
struct Breaks;
impl Read for Breaks { fn read(&mut self, _: &mut [u8]) -> io::Result<usize> { Err(io::Error::other("upstream gone")) } }
#[test]
fn every_body_says_how_it_ended() {
    chilpa_cast::trace::install(sink);
    let _serial = SERVERS.lock().unwrap_or_else(|e| e.into_inner());
    let server = MediaServer::start("127.0.0.1:9".parse().unwrap()).unwrap();
    let whole: Vec<u8> = (0..7777).map(|i| (i % 251) as u8).collect();
    let url = server.publish(asset(whole.clone())).unwrap();
    let from = mark();
    assert_eq!(request(&server, &url, "GET", "").1, whole);
    assert!(finished(&id_of(&url), from).ends_with("bytes=7777/7777 outcome=complete"));
    let (_, part) = request(&server, &url, "GET", "Range: bytes=100-199\r\n");
    assert_eq!(part, whole[100..200]);
    let url = server.publish(ExternalAsset { length: 5000, mime: "audio/flac".into(), source: Arc::new(BreakingSource { good: 100 }) }).unwrap();
    let from = mark();
    let mut socket = connect(&server);
    send(&mut socket, &url, "GET", "Connection: close\r\n");
    let headers = read_headers(&mut socket);
    assert_eq!(field(&headers, "Content-Length"), Some("5000"));
    let mut body = Vec::new();
    let _ = socket.read_to_end(&mut body);
    assert!(body.len() < 5000, "a cut source cannot look delivered");
    assert!(finished(&id_of(&url), from).ends_with("bytes=100/5000 outcome=source_error"));
    let lines = TRACE.lock().unwrap().clone();
    assert!(lines.iter().all(|l| !l.contains(&url.split('/').rev().nth(1).unwrap().to_string())), "the token never reaches the trace");
}
