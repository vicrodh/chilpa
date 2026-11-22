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
struct Current { id: u64, asset: ExternalAsset, extension: Option<&'static str>, cancellation: Cancellation }

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
                            let (response, tally) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                handle_request(&request, &entry, &token)
                            })) {
                                Ok(answer) => answer,
                                Err(_) => (failure(500, &request, format_args!("handler panicked")), None),
                            };
                            let sent = request.respond(response);
                            if let Some(tally) = tally { tally.finish(&sent); }
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
        // The address ends in the extension of the bytes served, for renderers
        // that tell a format by it; a type without a known one gets none.
        let extension = extension(&asset.mime);
        // Only the latest id is addressable. Existing bodies keep their source
        // until completion, revoke, a generation change, or server shutdown.
        *current = Some(Current { id, asset, extension, cancellation });
        Ok(format!("http://{}/audio/{}/{id}{}", self.address, self.token, extension.map(|e| format!(".{e}")).unwrap_or_default()))
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

/// The file extension of the bytes served under `mime`; none for a type
/// without a single well-known one.
pub fn extension(mime: &str) -> Option<&'static str> {
    let base = mime.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    Some(match base.as_str() {
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/vnd.wave" => "wav",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "audio/aac" | "audio/x-aac" => "aac",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/aiff" | "audio/x-aiff" => "aiff",
        "audio/x-dsf" => "dsf",
        "audio/x-dff" => "dff",
        "audio/x-ape" => "ape",
        _ => return None,
    })
}

/// What one body sent and how it ended, said once when it ends.
struct Tally { id: u64, expected: u64, sent: Arc<AtomicU64>, source_failed: Arc<AtomicBool>, revoked: Arc<AtomicBool> }
impl Tally {
    /// Bytes handed to the socket are not bytes the renderer decoded: this
    /// says only whether the server delivered the whole range.
    fn finish(&self, written: &io::Result<()>) {
        let sent = self.sent.load(Ordering::Acquire);
        let outcome = if self.source_failed.load(Ordering::Acquire) { "source_error" }
            else if self.revoked.load(Ordering::Acquire) { "revoked" }
            else if written.is_err() { "client_closed" }
            else if sent == self.expected { "complete" }
            else { "short" };
        crate::trace::event("cast.http", "finished", format_args!("id={} bytes={sent}/{} outcome={outcome}", self.id, self.expected));
    }
}

struct CancelledReader {
    reader: io::Take<Box<dyn Read + Send>>,
    cancellation: Cancellation,
    sent: Arc<AtomicU64>,
    source_failed: Arc<AtomicBool>,
    revoked: Arc<AtomicBool>,
}
impl Read for CancelledReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if !self.cancellation.active() { self.revoked.store(true, Ordering::Release); return Ok(0); }
        let read = self.reader.read(output);
        // A source may block waiting for bytes. Discard its result if cancelled
        // while it was blocked; the RangeSource API cannot interrupt that read.
        if !self.cancellation.active() { self.revoked.store(true, Ordering::Release); return Ok(0); }
        match &read {
            Ok(count) => { self.sent.fetch_add(*count as u64, Ordering::AcqRel); }
            Err(_) => self.source_failed.store(true, Ordering::Release),
        }
        read
    }
}
fn handle_request(request: &Request, current: &Mutex<Option<Current>>, expected_token: &str) -> (Response<Box<dyn Read>>, Option<Tally>) {
    crate::trace::event("cast.http", "request", format_args!("method={} peer={:?} path={}", request.method(), request.remote_addr(), crate::trace::redact(request.url())));
    let is_head = request.method() == &Method::Head;
    if request.method() != &Method::Get && !is_head {
        return (failure(405, request, format_args!("unsupported method {}", request.method())), None);
    }
    let Some((token, id, asked)) = parse_audio_request(request.url()) else {
        return (failure(404, request, format_args!("invalid audio path")), None);
    };
    if token != expected_token { return (failure(403, request, format_args!("token mismatch")), None); }
    let entry = current.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        .filter(|entry| entry.id == id && entry.cancellation.active()).cloned();
    let Some(entry) = entry else { return (failure(404, request, format_args!("missing or cancelled id={id}")), None); };
    // The address published, or the bare id; never another format's name.
    if asked.is_some() && asked != entry.extension {
        return (failure(404, request, format_args!("id={id} is not served as {}", asked.unwrap_or_default())), None);
    }
    let total = entry.asset.length;
    let headers = vec![
        header("Content-Type", &entry.asset.mime),
        header("Accept-Ranges", "bytes"),
        header("transferMode.dlna.org", "Streaming"),
        header("contentFeatures.dlna.org", CONTENT_FEATURES),
    ];
    // Range is defined for GET only (RFC 9110 14.2): a HEAD describes the whole.
    if is_head {
        crate::trace::event("cast.http", "serving", format_args!("id={id} method=HEAD status=200 bytes={total} content_range=none"));
        return (Response::new(StatusCode(200), headers, Box::new(io::empty()) as Box<dyn Read>, Some(total as usize), None)
            .with_chunked_threshold(usize::MAX), None);
    }
    let asked_range = request.headers().iter().find(|h| h.field.equiv("Range")).map(|h| h.value.as_str().to_string());
    let (start, length, content_range) = match asked_range.as_deref().map_or(Ranged::Whole, |value| ranged(value, total)) {
        Ranged::Whole => (0, total, None),
        Ranged::Part { start, length } => (start, length, Some(format!("bytes {start}-{}/{total}", start + length - 1))),
        Ranged::Unsatisfiable => {
            crate::trace::event("cast.http", "rejected", format_args!("status=416 id={id} range={} bytes={total}", asked_range.as_deref().unwrap_or_default()));
            return (Response::new(StatusCode(416), vec![header("Content-Range", &format!("bytes */{total}"))], Box::new(io::empty()) as Box<dyn Read>, Some(0), None), None);
        }
    };
    let status = if content_range.is_some() { 206 } else { 200 };
    let reader = match entry.asset.source.open(start, length) {
        Ok(reader) => reader,
        Err(error) => return (failure(500, request, format_args!("cannot open id={id} range={} bytes={total} error={}", asked_range.as_deref().unwrap_or("none"), crate::trace::redact(&error.to_string()))), None),
    };
    let tally = Tally { id, expected: length, sent: Arc::default(), source_failed: Arc::default(), revoked: Arc::default() };
    let body = CancelledReader {
        reader: reader.take(length), cancellation: entry.cancellation,
        sent: tally.sent.clone(), source_failed: tally.source_failed.clone(), revoked: tally.revoked.clone(),
    };
    crate::trace::event("cast.http", "serving", format_args!("id={id} method=GET status={status} bytes={length} range={} content_range={}", asked_range.as_deref().unwrap_or("none"), content_range.as_deref().unwrap_or("none")));
    let mut response = Response::new(StatusCode(status), headers, Box::new(body) as Box<dyn Read>, Some(length as usize), None)
        .with_chunked_threshold(usize::MAX);
    if let Some(content_range) = content_range { response = response.with_header(header("Content-Range", &content_range)); }
    (response, Some(tally))
}
fn failure(code: u16, request: &Request, reason: std::fmt::Arguments<'_>) -> Response<Box<dyn Read>> {
    crate::trace::event("cast.http", "rejected", format_args!("status={code} reason={reason} path={}", crate::trace::redact(request.url())));
    Response::new(StatusCode(code), Vec::new(), Box::new(io::empty()), Some(0), None)
}
fn header(name: &str, value: &str) -> Header { Header::from_bytes(name, value).unwrap() }
/// `/audio/<token>/<id>` or `/audio/<token>/<id>.<extension>`, the query
/// left aside: the token, the id and the extension asked for.
fn parse_audio_request(url: &str) -> Option<(&str, u64, Option<&str>)> {
    let path = url.split('?').next().unwrap_or(url);
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    if parts.len() != 3 || parts[0] != "audio" { return None; }
    let (id, extension) = match parts[2].split_once('.') { Some((id, extension)) => (id, Some(extension)), None => (parts[2], None) };
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) { return None; }
    Some((parts[1], id.parse().ok()?, extension))
}

/// What a Range header asks of a body of some length, read as RFC 9110 14
/// reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ranged {
    /// No range this server honors: another unit, several ranges (it sends
    /// no multipart), a last byte before the first, or a malformed value.
    /// The field is ignored and the whole body is the answer (14.2).
    Whole,
    /// One satisfiable range, clamped to the body.
    Part { start: u64, length: u64 },
    /// A valid range no byte of the body satisfies: 416 (15.5.17).
    Unsatisfiable,
}
pub fn ranged(value: &str, total: u64) -> Ranged {
    let Some((unit, set)) = value.trim().split_once('=') else { return Ranged::Whole };
    // Range units are case-insensitive (14.1).
    if !unit.trim().eq_ignore_ascii_case("bytes") || set.contains(',') { return Ranged::Whole; }
    let Some((first, last)) = set.trim().split_once('-') else { return Ranged::Whole };
    let number = |text: &str| -> Option<u64> { if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) { None } else { text.parse().ok() } };
    match (first.is_empty(), last.is_empty()) {
        (true, true) => Ranged::Whole,
        // A suffix: the last n bytes, or all of them when n is larger.
        (true, false) => match number(last) {
            None => Ranged::Whole,
            Some(0) => Ranged::Unsatisfiable,
            Some(_) if total == 0 => Ranged::Unsatisfiable,
            Some(n) => { let start = total.saturating_sub(n); Ranged::Part { start, length: total - start } }
        },
        (false, open) => {
            let Some(start) = number(first) else { return Ranged::Whole };
            let end = if open { None } else { match number(last) { Some(end) => Some(end), None => return Ranged::Whole } };
            // A last position before the first makes the range-spec invalid (14.1.1).
            if end.is_some_and(|end| end < start) { return Ranged::Whole; }
            if start >= total { return Ranged::Unsatisfiable; }
            let end = end.map_or(total - 1, |end| end.min(total - 1));
            Ranged::Part { start, length: end - start + 1 }
        }
    }
}
/// (start, length) of the one satisfiable range a header asks for.
pub fn range(value: &str, length: u64) -> Option<(u64, u64)> {
    match ranged(value, length) { Ranged::Part { start, length } => Some((start, length)), _ => None }
}
