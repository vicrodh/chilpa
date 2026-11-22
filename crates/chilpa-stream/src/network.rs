//! I apply the listener's network preferences before opening a connection.

use std::sync::atomic::{AtomicU8, Ordering};
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub offline: bool,
    pub proxy: bool,
    pub lyrics: bool,
    pub lyrics_service: bool,
    pub musicbrainz: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self { offline: false, proxy: true, lyrics: true, lyrics_service: true, musicbrainz: true }
    }
}

static POLICY: AtomicU8 = AtomicU8::new(30);

pub fn configure(policy: Policy) {
    let bits = u8::from(policy.offline) | (u8::from(policy.proxy) << 1)
        | (u8::from(policy.lyrics) << 2) | (u8::from(policy.lyrics_service) << 3)
        | (u8::from(policy.musicbrainz) << 4);
    POLICY.store(bits, Ordering::Release);
}

pub fn current() -> Policy {
    let bits = POLICY.load(Ordering::Acquire);
    Policy { offline: bits & 1 != 0, proxy: bits & 2 != 0, lyrics: bits & 4 != 0,
        lyrics_service: bits & 8 != 0, musicbrainz: bits & 16 != 0 }
}

impl Policy {
    pub fn lyrics_online(self) -> bool { !self.offline && self.lyrics && self.lyrics_service }
    pub fn artist_network(self) -> bool { !self.offline && (self.proxy || self.musicbrainz) }

    pub fn allows(self, address: &str) -> bool {
        if self.offline { return false; }
        let Ok(uri) = address.parse::<ureq::http::Uri>() else { return false; };
        let Some(host) = uri.host() else { return false; };
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        let within = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
        if within("api.chilpa.app") { return self.proxy; }
        if within("lyrics.chilpa.app") { return self.lyrics_online(); }
        if within("musicbrainz.org") || within("coverartarchive.org") { return self.musicbrainz; }
        true
    }
}

pub fn check(address: &str) -> Result<(), String> {
    if current().allows(address) { Ok(()) } else { Err(blocked()) }
}

pub fn blocked() -> String { chilpa_i18n::t("Network access is disabled in Settings > Integrations.") }

#[derive(Debug)]
struct GuardedResolver;

impl Resolver for GuardedResolver {
    fn resolve(&self, uri: &ureq::http::Uri, config: &ureq::config::Config, timeout: NextTimeout)
        -> Result<ResolvedSocketAddrs, ureq::Error> {
        check(&uri.to_string()).map_err(|why| ureq::Error::Io(std::io::Error::new(std::io::ErrorKind::PermissionDenied, why)))?;
        DefaultResolver::default().resolve(uri, config, timeout)
    }
}

pub(crate) fn agent(config: ureq::config::Config) -> ureq::Agent {
    ureq::Agent::with_parts(config, DefaultConnector::default(), GuardedResolver)
}

/// Adds headers to the requests it vouches for and reads their replies.
///
/// The program installs one before its first request. Which hosts it signs
/// for is its own decision; this module only asks, on every request that
/// passes `guard`, plugins' included.
pub trait Signer: Send + Sync {
    /// The headers for a request to `host` (lower case, no trailing dot) at
    /// `path` (no query), or none when it does not sign for that host. A
    /// header the request already carries is kept, not replaced.
    fn sign(&self, method: &str, host: &str, path: &str) -> Vec<(&'static str, String)>;
    /// The headers of the reply to a request it signed, and the host that
    /// sent them, which is another one when a redirect was followed.
    fn answered(&self, host: &str, headers: &ureq::http::HeaderMap);
}

static SIGNER: std::sync::OnceLock<Box<dyn Signer>> = std::sync::OnceLock::new();

/// Installs the signer. There is one per process: a second is refused.
pub fn sign_with(signer: Box<dyn Signer>) -> bool {
    SIGNER.set(signer).is_ok()
}

pub(crate) fn guard(request: ureq::http::Request<ureq::SendBody>, next: ureq::middleware::MiddlewareNext)
    -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    check(&request.uri().to_string()).map_err(|why| ureq::Error::Io(std::io::Error::new(std::io::ErrorKind::PermissionDenied, why)))?;
    signing(SIGNER.get().map(|signer| signer.as_ref()), request, next)
}

/// What `guard` does once the request is allowed, with the signer given.
fn signing(signer: Option<&dyn Signer>, mut request: ureq::http::Request<ureq::SendBody>, next: ureq::middleware::MiddlewareNext)
    -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let Some(signer) = signer else { return next.handle(request) };
    let mut signed = false;
    for (name, value) in signer.sign(request.method().as_str(), &host_of(request.uri()), request.uri().path()) {
        let (Ok(name), Ok(value)) = (ureq::http::HeaderName::from_bytes(name.as_bytes()), ureq::http::HeaderValue::from_str(&value)) else { continue };
        // A header the caller set stays: its own credential goes out instead.
        if let ureq::http::header::Entry::Vacant(slot) = request.headers_mut().entry(name) {
            slot.insert(value);
            signed = true;
        }
    }
    if !signed {
        return next.handle(request);
    }
    let reply = next.handle(request)?;
    // ureq follows redirects inside `handle`, so the reply can be another
    // host's; it records whose on every reply it returns (ResponseUri).
    signer.answered(&host_of(ureq::ResponseExt::get_uri(&reply)), reply.headers());
    Ok(reply)
}

fn host_of(uri: &ureq::http::Uri) -> String {
    uri.host().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_switches_match_hosts_without_blocking_unrelated_sources() {
        let p = Policy { proxy: false, musicbrainz: false, lyrics_service: false, ..Policy::default() };
        for url in ["https://api.chilpa.app/lastfm", "https://API.CHILPA.APP./x", "https://musicbrainz.org/ws/2",
            "https://coverartarchive.org/release/x", "https://lyrics.chilpa.app/resolve"] {
            assert!(!p.allows(url), "{url}");
        }
        for url in ["https://api.chilpa.app.example/x", "https://example.org/music", "http://127.0.0.1:8765/file"] {
            assert!(p.allows(url), "{url}");
        }
        assert!(!Policy { offline: true, ..Policy::default() }.allows("http://127.0.0.1:8765/file"));
    }

    /// Signs one path of this machine, so no other test's request is touched.
    struct OnePath {
        answered: std::sync::Mutex<Vec<String>>,
    }

    impl Signer for OnePath {
        fn sign(&self, method: &str, host: &str, path: &str) -> Vec<(&'static str, String)> {
            if host != "127.0.0.1" || path != "/signed-by-the-test" { return Vec::new(); }
            vec![("authorization", format!("Test {method} {host} {path}"))]
        }
        fn answered(&self, host: &str, headers: &ureq::http::HeaderMap) {
            let time = headers.get("x-test-time").and_then(|value| value.to_str().ok()).unwrap_or("none");
            self.answered.lock().unwrap().push(format!("{host} {time}"));
        }
    }

    fn loopback() -> std::net::TcpListener {
        std::net::TcpListener::bind("127.0.0.1:0").unwrap()
    }

    /// A listener on an address `localhost` names, so a request to it has
    /// another host than `127.0.0.1`. ureq tries every address the name has.
    fn localhost() -> std::net::TcpListener {
        use std::net::ToSocketAddrs;
        ("localhost", 0).to_socket_addrs().unwrap().find_map(|address| std::net::TcpListener::bind(address).ok())
            .expect("localhost names an address of this machine")
    }

    /// The request's head, read until the blank line that ends it.
    fn head_of(peer: &mut std::net::TcpStream) -> String {
        use std::io::Read;
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

    /// Answers one connection per reply, in order, and hands back the head
    /// each one asked with. It gives up after ten seconds, so a request that
    /// never comes fails the test instead of hanging it.
    fn answering(server: std::net::TcpListener, replies: Vec<String>) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        use std::io::Write;
        use std::time::{Duration, Instant};
        server.set_nonblocking(true).unwrap();
        let port = server.local_addr().unwrap().port();
        let asked = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut asked = Vec::new();
            for reply in replies {
                let mut peer = loop {
                    match server.accept() {
                        Ok((peer, _)) => break peer,
                        Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                        Err(_) => return asked,
                    }
                };
                // An accepted socket inherits non-blocking on some systems.
                peer.set_nonblocking(false).unwrap();
                peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                asked.push(head_of(&mut peer));
                peer.write_all(reply.as_bytes()).unwrap();
            }
            asked
        });
        (port, asked)
    }

    fn ok(time: Option<u64>) -> String {
        let time = time.map(|time| format!("X-Test-Time: {time}\r\n")).unwrap_or_default();
        format!("HTTP/1.1 200 OK\r\n{time}Content-Length: 2\r\nConnection: close\r\n\r\nok")
    }

    #[test]
    fn a_signer_adds_what_the_caller_did_not_set_and_its_signature_stops_at_a_redirect() {
        // Its own agent: the process-wide signer stays free.
        let signer: &'static OnePath = Box::leak(Box::new(OnePath { answered: std::sync::Mutex::new(Vec::new()) }));
        let agent = agent(ureq::Agent::config_builder().http_status_as_error(false)
            .middleware(move |request: ureq::http::Request<ureq::SendBody>, next: ureq::middleware::MiddlewareNext| signing(Some(signer as &dyn Signer), request, next))
            .build());

        let (elsewhere, after) = answering(localhost(), vec![ok(Some(1_800_000_000))]);
        let redirect = format!("HTTP/1.1 302 Found\r\nLocation: http://localhost:{elsewhere}/elsewhere\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let (port, asked) = answering(loopback(), vec![ok(Some(1_700_000_000)), ok(None), ok(None), redirect]);
        let url = |path: &str| format!("http://127.0.0.1:{port}{path}?query=left-out");
        agent.get(&url("/signed-by-the-test")).call().unwrap();
        agent.get(&url("/signed-by-the-test")).header("Authorization", "Caller credential").call().unwrap();
        agent.get(&url("/not-signed")).call().unwrap();
        agent.get(&url("/signed-by-the-test")).call().unwrap();
        let asked = asked.join().unwrap();
        let after = after.join().unwrap();
        assert_eq!((asked.len(), after.len()), (4, 1), "every request arrived");

        assert!(asked[0].contains("authorization: test get 127.0.0.1 /signed-by-the-test\r\n"), "{}", asked[0]);
        assert!(asked[1].contains("authorization: caller credential\r\n") && !asked[1].contains("test get"),
            "the caller's own header is kept: {}", asked[1]);
        assert!(!asked[2].contains("authorization"), "a path it does not sign goes out as it was: {}", asked[2]);
        assert!(asked[3].contains("authorization: test get"), "{}", asked[3]);
        assert!(!after[0].contains("authorization"), "the signature does not follow a redirect: {}", after[0]);
        assert_eq!(*signer.answered.lock().unwrap(), ["127.0.0.1 1700000000", "localhost 1800000000"],
            "only signed requests are read, each from the host whose reply ended it");
    }

    #[test]
    fn without_a_signer_the_request_goes_out_as_it_was() {
        let agent = agent(ureq::Agent::config_builder().http_status_as_error(false)
            .middleware(|request: ureq::http::Request<ureq::SendBody>, next: ureq::middleware::MiddlewareNext| signing(None, request, next))
            .build());
        let (port, asked) = answering(loopback(), vec![ok(None)]);
        agent.get(&format!("http://127.0.0.1:{port}/signed-by-the-test")).call().unwrap();
        let asked = asked.join().unwrap();
        assert!(!asked[0].contains("authorization"), "{}", asked[0]);
    }
}
