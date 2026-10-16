use crate::{dlna, Device, Error, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{collections::HashSet, net::UdpSocket, time::{Duration, Instant}};

/// I collect all SSDP replies before fetching descriptions, so a broken
/// description cannot hide another renderer in the same search window.
pub fn dlna(window: Duration) -> Result<Vec<Device>> { dlna_until(window, &AtomicBool::new(true)) }
pub fn dlna_until(window: Duration, running: &AtomicBool) -> Result<Vec<Device>> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| Error::Network)?;
    socket.set_read_timeout(Some(Duration::from_millis(200))).map_err(|_| Error::Network)?;
    let request = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
    socket.send_to(request.as_bytes(), "239.255.255.250:1900").map_err(|_| Error::Network)?;
    let until = Instant::now() + window;
    let mut locations = HashSet::new();
    let mut buf = [0u8; 8192];
    while running.load(Ordering::Acquire) && Instant::now() < until && locations.len() < 128 {
        if let Ok((n, peer)) = socket.recv_from(&mut buf) {
            if let Some(location) = location(&String::from_utf8_lossy(&buf[..n])) {
                if location.parse::<http::Uri>().ok().is_some_and(|u| u.scheme_str() == Some("http") && u.host().and_then(|s| s.parse::<std::net::IpAddr>().ok()) == Some(peer.ip())) { locations.insert(location); }
            }
        }
    }
    let mut seen = HashSet::new();
    Ok(locations.into_iter().take_while(|_| running.load(Ordering::Acquire)).filter_map(|url| dlna::describe(&url).ok()).map(|d| d.device).filter(|d| seen.insert(d.id.clone())).collect())
}
pub fn location(reply: &str) -> Option<String> {
    reply.lines().filter_map(|s| s.split_once(':')).find(|(key, _)| key.eq_ignore_ascii_case("location")).map(|(_, value)| value.trim().to_string())
}

pub fn chromecast(window: Duration) -> Result<Vec<Device>> { chromecast_until(window, &AtomicBool::new(true)) }
pub fn chromecast_until(window: Duration, running: &AtomicBool) -> Result<Vec<Device>> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(|_| Error::Network)?;
    let result = (|| {
        let receiver = daemon.browse("_googlecast._tcp.local.").map_err(|_| Error::Network)?;
        let deadline = Instant::now() + window;
        let mut devices = std::collections::HashMap::new();
        while running.load(Ordering::Acquire) && Instant::now() < deadline {
            if let Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) = receiver.recv_timeout(Duration::from_millis(100)) {
                let Some(id) = info.get_property_val_str("id").filter(|s| !s.is_empty()) else { continue };
                let Some(ip) = info.get_addresses().iter().filter(|a| !a.is_loopback()).min_by_key(|a| !a.is_ipv4()) else { continue };
                let address = match ip {
                    mdns_sd::ScopedIp::V4(ip) => std::net::SocketAddr::new((*ip.addr()).into(), info.get_port()),
                    mdns_sd::ScopedIp::V6(ip) => std::net::SocketAddr::V6(std::net::SocketAddrV6::new(*ip.addr(), info.get_port(), 0, ip.scope_id().index)),
                    _ => continue,
                };
                devices.insert(id.to_string(), Device { id: id.into(), name: info.get_property_val_str("fn").unwrap_or(id).into(), model: info.get_property_val_str("md").unwrap_or("Chromecast").into(), kind: crate::Kind::Chromecast, address, description: String::new() });
            }
        }
        Ok(devices.into_values().collect())
    })();
    let _ = daemon.shutdown();
    result
}
