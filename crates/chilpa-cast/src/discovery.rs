use crate::{dlna, Device, Error, Result};
use std::{collections::HashSet, net::UdpSocket, time::{Duration, Instant}};

/// I collect all SSDP replies before fetching descriptions, so a broken
/// description cannot hide another renderer in the same search window.
pub fn dlna(window: Duration) -> Result<Vec<Device>> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| Error::Network)?;
    socket.set_read_timeout(Some(Duration::from_millis(200))).map_err(|_| Error::Network)?;
    let request = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
    socket.send_to(request.as_bytes(), "239.255.255.250:1900").map_err(|_| Error::Network)?;
    let until = Instant::now() + window;
    let mut locations = HashSet::new();
    let mut buf = [0u8; 8192];
    while Instant::now() < until && locations.len() < 128 {
        if let Ok((n, peer)) = socket.recv_from(&mut buf) {
            if let Some(location) = location(&String::from_utf8_lossy(&buf[..n])) {
                if url::Url::parse(&location).ok().is_some_and(|u| u.scheme() == "http" && u.host_str().and_then(|s| s.parse::<std::net::IpAddr>().ok()) == Some(peer.ip())) { locations.insert(location); }
            }
        }
    }
    let mut seen = HashSet::new();
    Ok(locations.into_iter().filter_map(|url| dlna::describe(&url).ok()).map(|d| d.device).filter(|d| seen.insert(d.id.clone())).collect())
}
pub fn location(reply: &str) -> Option<String> {
    reply.lines().filter_map(|s| s.split_once(':')).find(|(key, _)| key.eq_ignore_ascii_case("location")).map(|(_, value)| value.trim().to_string())
}
