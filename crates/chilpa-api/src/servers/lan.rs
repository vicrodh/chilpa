//! The media servers on this network, found the way casting finds a
//! renderer: an SSDP search, then each description.
//!
//! It runs on the setup's own thread. The search is one UDP socket for
//! `LISTEN`; the descriptions are read one at a time, at most `DESCRIBED` of
//! them and for at most `DESCRIBING` in all, and only the servers they
//! describe are kept.

use super::net::Network;
use chilpa_servers::dlna::{self, Found};
use chilpa_servers::{Failure, Net};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// How long the network is listened to after asking.
const LISTEN: Duration = Duration::from_secs(3);

/// The most descriptions read after a search, and the time all of them get.
const DESCRIBED: usize = 12;
const DESCRIBING: Duration = Duration::from_secs(10);

/// The media servers that answer a search on this network, each once.
pub(crate) fn media_servers() -> Result<Vec<Found>, Failure> {
    // The search goes out over UDP, where the network policy does not
    // look: offline, it does not go out at all.
    if chilpa_core::network::current().offline {
        return Err(Failure::Unreachable(chilpa_core::network::blocked()));
    }
    let locations = chilpa_cast::discovery::search(dlna::DEVICE, LISTEN, &AtomicBool::new(true))
        .map_err(|_| {
            Failure::Unreachable(chilpa_i18n::t("this computer could not search the network"))
        })?;
    Ok(described(&Network, &locations))
}

/// What the locations describe, in the order they came. A device that is
/// not a media server, or does not answer in time, is left out.
pub(crate) fn described(net: &dyn Net, locations: &[String]) -> Vec<Found> {
    let until = Instant::now() + DESCRIBING;
    let mut found: Vec<Found> = Vec::new();
    for location in locations.iter().take(DESCRIBED) {
        if Instant::now() >= until {
            break;
        }
        if let Ok(server) = dlna::describe(net, location) {
            if !found.iter().any(|f| f.identity == server.identity) {
                found.push(server);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_servers::{Reply, Request};
    use std::sync::Mutex;

    /// Answers each address with the saved description, or a 404.
    struct Saved {
        asked: Mutex<Vec<String>>,
    }

    impl Net for Saved {
        fn send(&self, request: &Request) -> Result<Reply, String> {
            self.asked.lock().unwrap().push(request.url.clone());
            if request.url.ends_with("/DeviceDescription.xml") {
                let path = format!(
                    "{}/../chilpa-servers/tests/answers/dlna/plex-description.xml",
                    env!("CARGO_MANIFEST_DIR")
                );
                return Ok(Reply {
                    status: 200,
                    body: std::fs::read(path).unwrap(),
                    ..Reply::default()
                });
            }
            Ok(Reply {
                status: 404,
                ..Reply::default()
            })
        }
    }

    #[test]
    fn each_media_server_is_listed_once_and_other_devices_are_left_out() {
        let net = Saved {
            asked: Mutex::new(Vec::new()),
        };
        let locations = [
            "http://192.168.0.69:32469/DeviceDescription.xml".to_string(),
            "http://192.168.0.30:49152/printer.xml".to_string(),
            "http://192.168.0.69:32469/DeviceDescription.xml".to_string(),
        ];
        let found = described(&net, &locations);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "Plex Media Server: Lab");
        assert_eq!(
            net.asked.lock().unwrap().len(),
            3,
            "a description is never asked for its search capabilities here"
        );
    }
}

/// What a server's host name resolves to, in lower case: other names of
/// the same machine, under which its files and covers are its own
/// (`Details::hosts`). Nothing offline, and nothing for an address that is
/// already an IP.
pub(crate) fn other_names(address: &str) -> Vec<String> {
    use std::net::{IpAddr, ToSocketAddrs};
    if chilpa_core::network::current().offline {
        return Vec::new();
    }
    let Some((scheme, rest)) = address.split_once("://") else { return Vec::new() };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.starts_with('[') || authority.contains('@') {
        return Vec::new();
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().ok()),
        None => (authority, None),
    };
    if host.parse::<IpAddr>().is_ok() {
        return Vec::new();
    }
    let port = port.unwrap_or(if scheme.eq_ignore_ascii_case("https") { 443 } else { 80 });
    let mut names: Vec<String> = Vec::new();
    for at in (host, port).to_socket_addrs().into_iter().flatten() {
        let name = at.ip().to_string().to_ascii_lowercase();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}
