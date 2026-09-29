use crate::{Error, Result, CONTENT_FEATURES};
use std::{io::{Read, Write}, net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket}, sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}}, thread::JoinHandle, time::Duration};

pub trait RangeSource: Send + Sync {
    fn open(&self, start: u64, length: u64) -> std::io::Result<Box<dyn Read + Send>>;
}
#[derive(Clone)]
pub struct ExternalAsset {
    pub length: u64, pub mime: String, pub source: Arc<dyn RangeSource>,
}
struct Current { path: String, asset: ExternalAsset, cancelled: Arc<AtomicBool>, generation: Option<(Arc<AtomicU64>, u64)> }
pub struct MediaServer {
    address: SocketAddr, generation: Option<Arc<AtomicU64>>, current: Arc<Mutex<Option<Current>>>,
    stopped: Arc<AtomicBool>, cancellations: Mutex<Vec<std::sync::Weak<AtomicBool>>>, worker: Option<JoinHandle<()>>,
}
/// I ask the routing table without sending a UDP datagram.
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
        let mut address = route(renderer)?; address.set_port(0);
        let listener = TcpListener::bind(address).map_err(|_| Error::Network)?;
        listener.set_nonblocking(true).map_err(|_| Error::Network)?;
        let address = listener.local_addr().map_err(|_| Error::Network)?;
        let current = Arc::new(Mutex::new(None));
        let stopped = Arc::new(AtomicBool::new(false));
        let (entry, done) = (current.clone(), stopped.clone());
        let worker = std::thread::spawn(move || {
            let active = Arc::new(AtomicUsize::new(0));
            while !done.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((socket, peer)) => {
                        if peer.ip() != renderer.ip() || active.load(Ordering::Acquire) >= 4 { continue; }
                        active.fetch_add(1, Ordering::AcqRel);
                        let (entry, active, done) = (entry.clone(), active.clone(), done.clone());
                        std::thread::spawn(move || {
                            struct Count(Arc<AtomicUsize>);
                            impl Drop for Count { fn drop(&mut self) { self.0.fetch_sub(1, Ordering::AcqRel); } }
                            let _count = Count(active);
                            let _ = respond(socket, entry, done);
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(20)),
                }
            }
        });
        Ok(Self { address, generation, current, stopped, cancellations: Mutex::new(Vec::new()), worker: Some(worker) })
    }
    pub fn publish(&self, asset: ExternalAsset) -> Result<String> {
        if asset.length == 0 || asset.mime.contains(['\r', '\n']) { return Err(Error::Asset); }
        let mut bytes = [0; 16]; getrandom::fill(&mut bytes).map_err(|_| Error::Entropy)?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let path = format!("/audio/{token}/track");
        let url = format!("http://{}{path}", self.address);
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut cancellations = self.cancellations.lock().unwrap_or_else(|e| e.into_inner());
        cancellations.retain(|weak| weak.strong_count() > 0);
        cancellations.push(Arc::downgrade(&cancelled));
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = Some(Current { path, asset, cancelled, generation: self.generation.as_ref().map(|g| (g.clone(), g.load(Ordering::Acquire))) });
        Ok(url)
    }
    pub fn revoke(&self) {
        for weak in self.cancellations.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
            if let Some(cancelled) = weak.upgrade() { cancelled.store(true, Ordering::Release); }
        }
        if let Some(entry) = self.current.lock().unwrap_or_else(|e| e.into_inner()).take() { entry.cancelled.store(true, Ordering::Release); }
    }
    pub fn address(&self) -> SocketAddr { self.address }
}
impl Drop for MediaServer {
    fn drop(&mut self) {
        self.revoke(); self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}
/// Only one RFC byte range is accepted. I reject malformed or multipart ranges.
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
fn empty(socket: &mut TcpStream, code: u16, extra: &str) -> std::io::Result<()> {
    write!(socket, "HTTP/1.1 {code} Error\r\nContent-Length: 0\r\nConnection: close\r\n{extra}\r\n")
}
fn respond(mut socket: TcpStream, current: Arc<Mutex<Option<Current>>>, stopped: Arc<AtomicBool>) -> std::io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(5)))?; socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut request = Vec::new();
    let mut byte = [0];
    while request.len() < 16 * 1024 && !request.ends_with(b"\r\n\r\n") {
        if stopped.load(Ordering::Acquire) || std::time::Instant::now() >= deadline { return Ok(()); }
        socket.read_exact(&mut byte)?; request.push(byte[0]);
    }
    if !request.ends_with(b"\r\n\r\n") { return empty(&mut socket, 431, ""); }
    let text = String::from_utf8_lossy(&request);
    let mut lines = text.lines();
    let parts: Vec<_> = lines.next().unwrap_or("").split_whitespace().collect();
    if parts.len() != 3 { return empty(&mut socket, 400, ""); }
    if !matches!(parts[0], "GET" | "HEAD") { return empty(&mut socket, 405, "Allow: GET, HEAD\r\n"); }
    let found = current.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|e| e.path == parts[1] && e.generation.as_ref().is_none_or(|(g, stamp)| g.load(Ordering::Acquire) == *stamp)).map(|e| (e.asset.clone(), e.cancelled.clone(), e.generation.clone()));
    let Some((asset, cancelled, generation)) = found else { return empty(&mut socket, 404, ""); };
    let ranges: Vec<_> = lines.filter_map(|s| s.split_once(':')).filter(|(k, _)| k.eq_ignore_ascii_case("range")).collect();
    let (start, length, partial) = match ranges.as_slice() {
        [] => (0, asset.length, false),
        [(_, value)] => match range(value.trim(), asset.length) {
            Some((start, length)) => (start, length, true),
            None => return empty(&mut socket, 416, &format!("Content-Range: bytes */{}\r\n", asset.length)),
        },
        _ => return empty(&mut socket, 416, &format!("Content-Range: bytes */{}\r\n", asset.length)),
    };
    let mut reader = if parts[0] == "GET" {
        match asset.source.open(start, length) { Ok(reader) => Some(reader), Err(_) => return empty(&mut socket, 502, "") }
    } else { None };
    write!(socket, "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {length}\r\nAccept-Ranges: bytes\r\ntransferMode.dlna.org: Streaming\r\ncontentFeatures.dlna.org: {CONTENT_FEATURES}\r\nConnection: close\r\n", if partial { "206 Partial Content" } else { "200 OK" }, asset.mime)?;
    if partial { write!(socket, "Content-Range: bytes {start}-{}/{}\r\n", start + length - 1, asset.length)?; }
    write!(socket, "\r\n")?;
    if let Some(reader) = reader.as_mut() {
        let mut left = length; let mut buffer = [0; 64 * 1024];
        while left > 0 && generation.as_ref().is_none_or(|(g, stamp)| g.load(Ordering::Acquire) == *stamp) && !cancelled.load(Ordering::Acquire) && !stopped.load(Ordering::Acquire) {
            let amount = left.min(buffer.len() as u64) as usize;
            let read = reader.read(&mut buffer[..amount])?;
            if read == 0 { break; }
            socket.write_all(&buffer[..read])?; left -= read as u64;
        }
    }
    Ok(())
}
