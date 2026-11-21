use crate::{dlna, Device, Error, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{collections::HashSet, net::UdpSocket, time::{Duration, Instant}};

/// All SSDP replies are collected before any description is fetched, so a
/// broken description cannot hide another renderer in the same window.
pub fn dlna(window: Duration) -> Result<Vec<Device>> { dlna_until(window, &AtomicBool::new(true)) }
pub fn dlna_until(window: Duration, running: &AtomicBool) -> Result<Vec<Device>> {
    let locations = renderers(window, running)?;
    let mut seen = HashSet::new();
    Ok(locations.into_iter().take_while(|_| running.load(Ordering::Acquire)).filter_map(|url| dlna::describe(&url).ok()).filter(|d| seen.insert(d.id.clone())).collect())
}
/// The description addresses of the renderers that answer within `window`,
/// each once, asked with the reference player's search: rupnp's M-SEARCH for
/// `MediaRenderer:1` with MX 3. A reply that cannot be read is skipped, not
/// the end of the window.
pub fn renderers(window: Duration, running: &AtomicBool) -> Result<Vec<String>> {
    use futures_util::TryStreamExt;
    use rupnp::ssdp::{SearchTarget, URN};
    let target = SearchTarget::URN(URN::Device("schemas-upnp-org".into(), "MediaRenderer".into(), 1));
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_| Error::Network)?;
    runtime.block_on(async {
        let stream = rupnp::ssdp::search(&target, window, 3, None).await.map_err(|_| Error::Network)?;
        let mut stream = std::pin::pin!(stream);
        let (mut locations, mut seen) = (Vec::new(), HashSet::new());
        while running.load(Ordering::Acquire) && locations.len() < 128 {
            match stream.try_next().await {
                Ok(Some(reply)) => {
                    let location = reply.location().trim().to_string();
                    if seen.insert(location.clone()) { locations.push(location); }
                }
                Ok(None) => break,
                Err(error) => crate::trace::event("cast.dlna", "search_reply_skipped", format_args!("{error}")),
            }
        }
        Ok(locations)
    })
}
/// The description addresses of the devices of one type that answer an SSDP
/// search within `window`, each once. A media server is found the same way
/// as a renderer, with its own search target.
pub fn search(target: &str, window: Duration, running: &AtomicBool) -> Result<Vec<String>> {
    if target.contains(['\r', '\n']) { return Err(Error::Network); }
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| Error::Network)?;
    socket.set_read_timeout(Some(Duration::from_millis(200))).map_err(|_| Error::Network)?;
    let request = format!("M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: {target}\r\n\r\n");
    socket.send_to(request.as_bytes(), "239.255.255.250:1900").map_err(|_| Error::Network)?;
    let until = Instant::now() + window;
    let mut locations = HashSet::new();
    let mut buf = [0u8; 8192];
    while running.load(Ordering::Acquire) && Instant::now() < until && locations.len() < 128 {
        if let Ok((n, peer)) = socket.recv_from(&mut buf) {
            if let Some(location) = accepted(&String::from_utf8_lossy(&buf[..n]), peer.ip()) { locations.insert(location); }
        }
    }
    Ok(locations.into_iter().collect())
}
/// The location an SSDP reply gives, when it is plain http on the address
/// that sent the reply: a device only names itself.
pub fn accepted(reply: &str, peer: std::net::IpAddr) -> Option<String> {
    location(reply).filter(|location| location.parse::<http::Uri>().ok().is_some_and(|u| u.scheme_str() == Some("http") && u.host().and_then(|s| s.parse::<std::net::IpAddr>().ok()) == Some(peer)))
}
pub fn location(reply: &str) -> Option<String> {
    reply.lines().filter_map(|s| s.split_once(':')).find(|(key, _)| key.eq_ignore_ascii_case("location")).map(|(_, value)| value.trim().to_string())
}

pub fn chromecast(window: Duration) -> Result<Vec<Device>> { chromecast_until(window, &AtomicBool::new(true)) }
pub fn chromecast_until(window: Duration, running: &AtomicBool) -> Result<Vec<Device>> {
    let mut devices = std::collections::HashMap::new();
    browse(window, running, &mut |sighting| { devices.insert(sighting.device.id.clone(), sighting.device); true })?;
    Ok(devices.into_values().collect())
}
/// A Cast receiver as one mDNS answer describes it: the TXT `id` is its
/// identity, and the instance name is kept to resolve it again later.
#[derive(Clone, Debug)]
pub struct Sighting { pub device: Device, pub fullname: String }
/// The most receivers one browse reports.
pub const BROWSE_LIMIT: usize = 128;
/// Browses for Cast receivers and hands each one over as it resolves, again
/// when its address changes. Ends when `window` runs out, `running` clears,
/// `found` returns false or `BROWSE_LIMIT` receivers were seen.
pub fn browse(window: Duration, running: &AtomicBool, found: &mut dyn FnMut(Sighting) -> bool) -> Result<()> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(|_| Error::Network)?;
    let result = (|| {
        let receiver = daemon.browse("_googlecast._tcp.local.").map_err(|_| Error::Network)?;
        let deadline = Instant::now() + window;
        let mut seen = std::collections::HashMap::new();
        while running.load(Ordering::Acquire) && Instant::now() < deadline {
            if let Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) = receiver.recv_timeout(Duration::from_millis(100)) {
                let Some(id) = info.get_property_val_str("id").filter(|s| !s.is_empty()) else { continue };
                // The same receiver gives the same address every time: IPv4 first, then the lowest.
                let Some(ip) = info.get_addresses().iter().filter(|a| !a.is_loopback()).min_by_key(|a| (!a.is_ipv4(), a.to_ip_addr())) else { continue };
                let address = match ip {
                    mdns_sd::ScopedIp::V4(ip) => std::net::SocketAddr::new((*ip.addr()).into(), info.get_port()),
                    mdns_sd::ScopedIp::V6(ip) => std::net::SocketAddr::V6(std::net::SocketAddrV6::new(*ip.addr(), info.get_port(), 0, ip.scope_id().index)),
                    _ => continue,
                };
                if seen.get(id) == Some(&address) || (seen.len() >= BROWSE_LIMIT && !seen.contains_key(id)) { continue; }
                seen.insert(id.to_string(), address);
                let device = Device { id: id.into(), name: info.get_property_val_str("fn").unwrap_or(id).into(), model: info.get_property_val_str("md").unwrap_or("Chromecast").into(), kind: crate::Kind::Chromecast, address, description: String::new() };
                if !found(Sighting { device, fullname: info.get_fullname().into() }) { break; }
            }
        }
        Ok(())
    })();
    let _ = daemon.shutdown();
    result
}
