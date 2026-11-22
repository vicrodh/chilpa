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
    /// `path` (no query), or none when it does not sign for that host.
    fn sign(&self, method: &str, host: &str, path: &str) -> Vec<(&'static str, String)>;
    /// The headers of the reply to a request it signed.
    fn answered(&self, host: &str, headers: &ureq::http::HeaderMap);
}

static SIGNER: std::sync::OnceLock<Box<dyn Signer>> = std::sync::OnceLock::new();

/// Installs the signer. There is one per process: a second is refused.
pub fn sign_with(signer: Box<dyn Signer>) -> bool {
    SIGNER.set(signer).is_ok()
}

pub(crate) fn guard(mut request: ureq::http::Request<ureq::SendBody>, next: ureq::middleware::MiddlewareNext)
    -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    check(&request.uri().to_string()).map_err(|why| ureq::Error::Io(std::io::Error::new(std::io::ErrorKind::PermissionDenied, why)))?;
    let Some(signer) = SIGNER.get() else { return next.handle(request) };
    let host = request.uri().host().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    let headers = signer.sign(request.method().as_str(), &host, request.uri().path());
    if headers.is_empty() {
        return next.handle(request);
    }
    // Set, not appended: a header of the same name the caller wrote is replaced.
    for (name, value) in headers {
        if let (Ok(name), Ok(value)) = (ureq::http::HeaderName::from_bytes(name.as_bytes()), ureq::http::HeaderValue::from_str(&value)) {
            request.headers_mut().insert(name, value);
        }
    }
    let reply = next.handle(request)?;
    signer.answered(&host, reply.headers());
    Ok(reply)
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
            vec![("X-Test-Signature", format!("{method} {host} {path}")), ("x-test-replaced", "from the signer".into())]
        }
        fn answered(&self, host: &str, headers: &ureq::http::HeaderMap) {
            let time = headers.get("x-test-time").and_then(|value| value.to_str().ok()).unwrap_or("none");
            self.answered.lock().unwrap().push(format!("{host} {time}"));
        }
    }

    /// Answers each of `replies` connections with `reply` and hands back what was asked.
    fn answering(replies: usize, reply: &'static str) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let asked = std::thread::spawn(move || (0..replies).map(|_| {
            let (mut peer, _) = server.accept().unwrap();
            let mut asked = [0u8; 4096];
            let n = peer.read(&mut asked).unwrap_or(0);
            peer.write_all(reply.as_bytes()).unwrap();
            String::from_utf8_lossy(&asked[..n]).to_ascii_lowercase()
        }).collect());
        (port, asked)
    }

    #[test]
    fn a_signer_sets_its_headers_on_what_it_signs_and_reads_the_reply() {
        let signer = Box::leak(Box::new(OnePath { answered: std::sync::Mutex::new(Vec::new()) }));
        assert!(sign_with(Box::new(Forward(signer))), "the first signer is installed");
        assert!(!sign_with(Box::new(Forward(signer))), "and a second is refused");

        let (port, asked) = answering(2, "HTTP/1.1 200 OK\r\nX-Test-Time: 1700000000\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        let request = |path: &str| crate::send_request(serde_json::json!({
            "url": format!("http://127.0.0.1:{port}{path}?query=left-out"), "method": "GET",
            "headers": {"X-Test-Replaced": "from the plugin"}
        }).to_string().as_bytes(), None);
        request("/signed-by-the-test");
        request("/not-signed");
        let asked = asked.join().unwrap();

        assert!(asked[0].contains("x-test-signature: get 127.0.0.1 /signed-by-the-test\r\n"), "{}", asked[0]);
        assert!(asked[0].contains("x-test-replaced: from the signer\r\n"), "{}", asked[0]);
        assert!(!asked[0].contains("from the plugin"), "the signer's header replaces the caller's: {}", asked[0]);
        assert!(!asked[1].contains("x-test-signature"), "a path it does not sign goes out as it was: {}", asked[1]);
        assert!(asked[1].contains("x-test-replaced: from the plugin\r\n"), "{}", asked[1]);
        assert_eq!(*signer.answered.lock().unwrap(), ["127.0.0.1 1700000000"], "only the signed reply is read");
    }

    /// The installed signer is a `Box`; this one hands each call to a signer the test keeps.
    struct Forward(&'static OnePath);

    impl Signer for Forward {
        fn sign(&self, method: &str, host: &str, path: &str) -> Vec<(&'static str, String)> { self.0.sign(method, host, path) }
        fn answered(&self, host: &str, headers: &ureq::http::HeaderMap) { self.0.answered(host, headers) }
    }
}
