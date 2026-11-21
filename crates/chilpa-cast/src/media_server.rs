use crate::{Error, Result, CONTENT_FEATURES};
use std::{
    io::{self, Read},
    net::{IpAddr, SocketAddr, UdpSocket},
    sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}},
    thread::{self, JoinHandle},
    time::Duration,
};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

pub trait RangeSource: Send + Sync {
    fn open(&self, start: u64, length: u64) -> io::Result<Box<dyn Read + Send>>;
}
#[derive(Clone)]
pub struct ExternalAsset {
    pub length: u64,
    pub mime: String,
    pub source: Arc<dyn RangeSource>,
}
#[derive(Clone)]
struct Cancellation {
    epoch: Arc<AtomicU64>,
    stamp: u64,
    generation: Option<(Arc<AtomicU64>, u64)>,
}
impl Cancellation {
    fn active(&self) -> bool {
        self.epoch.load(Ordering::Acquire) == self.stamp
            && self.generation.as_ref().is_none_or(|(g, stamp)| g.load(Ordering::Acquire) == *stamp)
    }
}
#[derive(Clone)]
struct Current { id: u64, asset: ExternalAsset, cancellation: Cancellation }

pub struct MediaServer {
    address: SocketAddr,
    token: String,
    next_id: AtomicU64,
    generation: Option<Arc<AtomicU64>>,
    current: Arc<Mutex<Option<Current>>>,
    // One shared epoch cancels current and evicted bodies without retaining a
    // list of past publications. Each request releases its reader on completion.
    epoch: Arc<AtomicU64>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
/// Ask the routing table without sending a UDP datagram.
pub fn interface_to(renderer: SocketAddr) -> Result<IpAddr> { route(renderer).map(|address| address.ip()) }
fn route(renderer: SocketAddr) -> Result<SocketAddr> {
    let bind = if renderer.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
    let socket = UdpSocket::bind(bind).map_err(|_| Error::Network)?;
    socket.connect(renderer).map_err(|_| Error::Network)?;
    socket.local_addr().map_err(|_| Error::Network)
}
impl MediaServer {
    pub fn start(renderer: SocketAddr) -> Result<Self> { Self::start_guarded(renderer, None) }
    pub fn start_guarded(renderer: SocketAddr, generation: Option<Arc<AtomicU64>>) -> Result<Self> {
        let mut address = route(renderer)?;
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(|_| Error::Entropy)?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let (preferred, fallback) = if renderer.is_ipv4() { ("0.0.0.0:9876", "0.0.0.0:0") } else { ("[::]:9876", "[::]:0") };
        let server = Server::http(preferred).or_else(|error| {
            crate::trace::event("cast.http", "port_fallback", format_args!("port=9876 error={}", crate::trace::redact(&error.to_string())));
            Server::http(fallback)
        }).map_err(|_| Error::Network)?;
        address.set_port(server.server_addr().to_ip().ok_or(Error::Network)?.port());
        let current = Arc::new(Mutex::new(None));
        let stopped = Arc::new(AtomicBool::new(false));
        let (entry, done, expected_token) = (current.clone(), stopped.clone(), token.clone());
        let worker = thread::spawn(move || {
            crate::trace::event("cast.http", "started", format_args!("address={address}"));
            while !done.load(Ordering::Acquire) {
                match server.recv_timeout(Duration::from_millis(250)) {
                    Ok(Some(request)) => {
                        // A slow body must not block a HEAD probe or another seek.
                        // Match the reference server: one thread per request, no cap.
                        let (entry, token) = (entry.clone(), expected_token.clone());
                        thread::spawn(move || {
                            let response = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                handle_request(&request, &entry, &token)
                            })) {
                                Ok(response) => response,
                                Err(_) => failure(500, &request, format_args!("handler panicked")),
                            };
                            let _ = request.respond(response);
                        });
                    }
                    Ok(None) => {}
                    Err(error) => {
                        crate::trace::event("cast.http", "receive_failed", format_args!("error={}", crate::trace::redact(&error.to_string())));
                        break;
                    }
                }
            }
            crate::trace::event("cast.http", "stopped", format_args!("address={address}"));
        });
        Ok(Self { address, token, next_id: AtomicU64::new(1), generation, current, epoch: Arc::new(AtomicU64::new(0)), stopped, worker: Some(worker) })
    }
    pub fn publish(&self, asset: ExternalAsset) -> Result<String> {
        if asset.length == 0 || asset.mime.contains(['\r', '\n']) { return Err(Error::Asset); }
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancellation = Cancellation {
            epoch: self.epoch.clone(), stamp: self.epoch.load(Ordering::Acquire),
            generation: self.generation.as_ref().map(|g| (g.clone(), g.load(Ordering::Acquire))),
        };
        // Only the latest id is addressable. Existing bodies keep their source
        // until completion, revoke, a generation change, or server shutdown.
        *current = Some(Current { id, asset, cancellation });
        Ok(format!("http://{}/audio/{}/{id}", self.address, self.token))
    }
    pub fn revoke(&self) {
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
        self.epoch.fetch_add(1, Ordering::AcqRel);
        current.take();
    }
    pub fn address(&self) -> SocketAddr { self.address }
}
impl Drop for MediaServer {
    fn drop(&mut self) {
        self.revoke();
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

struct CancelledReader {
    reader: io::Take<Box<dyn Read + Send>>,
    cancellation: Cancellation,
}
impl Read for CancelledReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if !self.cancellation.active() { return Ok(0); }
        let read = self.reader.read(output);
        // A source may block waiting for bytes. Discard its result if cancelled
        // while it was blocked; the RangeSource API cannot interrupt that read.
        if !self.cancellation.active() { return Ok(0); }
        read
    }
}
fn handle_request(request: &Request, current: &Mutex<Option<Current>>, expected_token: &str) -> Response<Box<dyn Read>> {
    crate::trace::event("cast.http", "request", format_args!("method={} peer={:?} path={}", request.method(), request.remote_addr(), crate::trace::redact(request.url())));
    let is_head = request.method() == &Method::Head;
    if request.method() != &Method::Get && !is_head {
        return failure(405, request, format_args!("unsupported method {}", request.method()));
    }
    let Some((token, id)) = parse_audio_request(request.url()) else {
        return failure(404, request, format_args!("invalid audio path"));
    };
    if token != expected_token { return failure(403, request, format_args!("token mismatch")); }
    let entry = current.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        .filter(|entry| entry.id == id && entry.cancellation.active()).cloned();
    let Some(entry) = entry else { return failure(404, request, format_args!("missing or cancelled id={id}")); };
    let headers = vec![
        header("Content-Type", &entry.asset.mime),
        header("Accept-Ranges", "bytes"),
        header("transferMode.dlna.org", "Streaming"),
        header("contentFeatures.dlna.org", CONTENT_FEATURES),
    ];
    if is_head {
        crate::trace::event("cast.http", "serving", format_args!("id={id} status=200 bytes={} content_range=none", entry.asset.length));
        return Response::new(StatusCode(200), headers, Box::new(io::empty()) as Box<dyn Read>, Some(entry.asset.length as usize), None)
            .with_chunked_threshold(usize::MAX);
    }
    let range = request.headers().iter().find(|h| h.field.equiv("Range"))
        .and_then(|h| parse_range(h.value.as_str(), entry.asset.length));
    let (start, end, status, content_range) = match range {
        Some((start, end)) => (start, end, 206, Some(format!("bytes {start}-{end}/{}", entry.asset.length))),
        None => (0, entry.asset.length - 1, 200, None),
    };
    // Preserve the reference reader's clamping, including reversed ranges.
    let end = end.min(entry.asset.length - 1);
    let start = start.min(end);
    let length = end - start + 1;
    let reader = match entry.asset.source.open(start, length) {
        Ok(reader) => reader,
        Err(error) => return failure(500, request, format_args!("cannot open id={id} range={range:?} bytes={} error={}", entry.asset.length, crate::trace::redact(&error.to_string()))),
    };
    let body = CancelledReader { reader: reader.take(length), cancellation: entry.cancellation };
    crate::trace::event("cast.http", "serving", format_args!("id={id} status={status} bytes={length} content_range={}", content_range.as_deref().unwrap_or("none")));
    let mut response = Response::new(StatusCode(status), headers, Box::new(body) as Box<dyn Read>, Some(length as usize), None)
        .with_chunked_threshold(usize::MAX);
    if let Some(content_range) = content_range { response = response.with_header(header("Content-Range", &content_range)); }
    response
}
fn failure(code: u16, request: &Request, reason: std::fmt::Arguments<'_>) -> Response<Box<dyn Read>> {
    crate::trace::event("cast.http", "rejected", format_args!("status={code} reason={reason} path={}", crate::trace::redact(request.url())));
    Response::new(StatusCode(code), Vec::new(), Box::new(io::empty()), Some(0), None)
}
fn header(name: &str, value: &str) -> Header { Header::from_bytes(name, value).unwrap() }
fn parse_audio_request(url: &str) -> Option<(&str, u64)> {
    let path = url.split('?').next().unwrap_or(url);
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    if parts.len() != 3 || parts[0] != "audio" { return None; }
    Some((parts[1], parts[2].parse().ok()?))
}
// Wire compatibility follows the reference parser. Rejected ranges mean a
// full 200 response; the public helper below retains its (start, length) API.
fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    if !header.starts_with("bytes=") { return None; }
    let range = header.trim_start_matches("bytes=");
    let mut parts = range.split('-');
    let start_str = parts.next().unwrap_or("");
    let end_str = parts.next().unwrap_or("");
    if start_str.is_empty() {
        let suffix = end_str.parse::<u64>().ok()?;
        if suffix == 0 { return None; }
        return Some((total.saturating_sub(suffix), total.saturating_sub(1)));
    }
    let start = start_str.parse::<u64>().ok()?;
    if start >= total { return None; }
    let end = if end_str.is_empty() { total.saturating_sub(1) } else { end_str.parse::<u64>().ok()?.min(total.saturating_sub(1)) };
    Some((start, end))
}
/// Return (start, length) for one valid byte range; reject malformed ranges.
pub fn range(value: &str, length: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    let (first, last) = value.split_once('-')?;
    if length == 0 { return None; }
    if first.is_empty() {
        let suffix: u64 = last.parse().ok()?;
        if suffix == 0 { return None; }
        let start = length.saturating_sub(suffix); return Some((start, length - start));
    }
    let start: u64 = first.parse().ok()?;
    let end: u64 = if last.is_empty() { length - 1 } else { last.parse::<u64>().ok()?.min(length - 1) };
    if start >= length || start > end { return None; }
    Some((start, end - start + 1))
}
