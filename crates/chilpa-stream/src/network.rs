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

pub(crate) fn guard(request: ureq::http::Request<ureq::SendBody>, next: ureq::middleware::MiddlewareNext)
    -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    check(&request.uri().to_string()).map_err(|why| ureq::Error::Io(std::io::Error::new(std::io::ErrorKind::PermissionDenied, why)))?;
    next.handle(request)
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
}
