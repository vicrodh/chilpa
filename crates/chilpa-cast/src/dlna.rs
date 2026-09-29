use crate::{Device, Error, Kind, Metadata, RemoteTransport, Result, State, Status, CONTENT_FEATURES};
use std::{collections::HashMap, time::Duration};
use http::Uri;

#[derive(Clone, Debug)]
pub struct Service { pub kind: String, pub endpoint: String }
#[derive(Clone, Debug)]
pub struct Description {
    pub device: Device, pub transport: Service, pub rendering: Option<Service>, pub connection: Option<Service>,
}
fn field(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.children().find(|n| n.has_tag_name(name)).and_then(|n| n.text()).unwrap_or("").trim().into()
}
pub fn parse(location: &str, xml: &str) -> Result<Description> {
    let url = location.parse::<Uri>().map_err(|_| Error::Description)?;
    if url.scheme_str() != Some("http") || url.authority().is_none_or(|a| a.as_str().contains('@')) { return Err(Error::Description); }
    let doc = roxmltree::Document::parse(xml).map_err(|_| Error::Description)?;
    let node = doc.descendants().find(|n| n.has_tag_name("device") && field(*n, "deviceType").contains(":device:MediaRenderer:")).ok_or(Error::Description)?;
    let base = doc.descendants().find(|n| n.has_tag_name("URLBase")).and_then(|n| n.text()).filter(|s| !s.trim().is_empty()).unwrap_or(location);
    let base = base.parse::<Uri>().map_err(|_| Error::Description)?;
    let services: Vec<Service> = node.descendants().filter(|n| n.has_tag_name("service")).filter_map(|n| {
        let path = field(n, "controlURL");
        if path.is_empty() || path.len() > 8192 || path.chars().any(char::is_whitespace) { return None; }
        // Legacy renderers advertise paths without the leading slash.
        let path = if path.starts_with('/') || path.contains("://") { path } else { format!("/{path}") };
        let endpoint = if path.starts_with('/') {
            format!("http://{}{path}", base.authority()?).parse::<Uri>().ok()?
        } else { path.parse::<Uri>().ok()? };
        if endpoint.host() != url.host() || endpoint.scheme_str() != Some("http") || endpoint.authority()?.as_str().contains('@') { return None; }
        Some(Service { kind: field(n, "serviceType"), endpoint: endpoint.to_string() })
    }).collect();
    let find = |name: &str| services.iter().find(|s| s.kind.starts_with(&format!("urn:schemas-upnp-org:service:{name}:"))).cloned();
    let transport = find("AVTransport").ok_or(Error::Description)?;
    let id = field(node, "UDN");
    if id.is_empty() { return Err(Error::Description); }
    let ip = url.host().ok_or(Error::Description)?.trim_matches(['[', ']']).parse().map_err(|_| Error::Description)?;
    Ok(Description {
        device: Device { id, name: field(node, "friendlyName"), model: field(node, "modelName"), kind: Kind::Dlna,
            address: std::net::SocketAddr::new(ip, url.port_u16().unwrap_or(80)), description: location.into() },
        transport, rendering: find("RenderingControl"), connection: find("ConnectionManager"),
    })
}
pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(4))).http_status_as_error(false).max_redirects(0).build().into()
}
pub fn describe(location: &str) -> Result<Description> {
    let mut reply = agent().get(location).call().map_err(|_| Error::Network)?;
    if reply.status() != 200 { return Err(Error::Description); }
    let xml = reply.body_mut().with_config().limit(512 * 1024).read_to_string().map_err(|_| Error::Description)?;
    parse(location, &xml)
}
pub fn escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;") }

pub struct Dlna { pub description: Description, agent: ureq::Agent, payload: Option<String>, fresh: bool }
impl Dlna {
    pub fn connect(description: Description) -> Self { Self { description, agent: agent(), payload: None, fresh: false } }
    fn action(&self, service: &Service, name: &str, args: &str) -> Result<HashMap<String, String>> {
        let body = format!("<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{name} xmlns:u=\"{}\">{args}</u:{name}></s:Body></s:Envelope>", service.kind);
        let mut reply = self.agent.post(&service.endpoint).header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("SOAPAction", format!("\"{}#{name}\"", service.kind)).send(body.as_bytes()).map_err(|_| Error::Network)?;
        let status = reply.status().as_u16();
        let xml = reply.body_mut().with_config().limit(512 * 1024).read_to_string().map_err(|_| Error::Protocol)?;
        let doc = roxmltree::Document::parse(&xml).map_err(|_| if status == 701 || status == 702 { Error::Fault(status) } else { Error::Protocol })?;
        if let Some(code) = doc.descendants().find(|n| n.has_tag_name("errorCode")).and_then(|n| n.text()).and_then(|s| s.parse().ok()) { return Err(Error::Fault(code)); }
        if status != 200 { return Err(Error::Fault(status)); }
        Ok(doc.descendants().filter(|n| n.is_element()).filter_map(|n| n.text().map(|t| (n.tag_name().name().into(), t.into()))).collect())
    }
    fn transport_action(&self, name: &str, args: &str) -> Result<HashMap<String, String>> { self.action(&self.description.transport, name, &format!("<InstanceID>0</InstanceID>{args}")) }
    fn rc(&self, name: &str, args: &str) -> Result<HashMap<String, String>> {
        self.action(self.description.rendering.as_ref().ok_or(Error::Unsupported)?, name, &format!("<InstanceID>0</InstanceID><Channel>Master</Channel>{args}"))
    }
    fn accepts(&self, mime: &str) -> Result<()> {
        if let Some(service) = &self.description.connection {
            let info = self.action(service, "GetProtocolInfo", "")?;
            let sink = info.get("Sink").ok_or(Error::Unsupported)?;
            if !sink.split(',').any(|s| { let p: Vec<_> = s.split(':').collect(); p.len() >= 4 && p[0] == "http-get" && (p[2] == mime || p[2] == "*") }) { return Err(Error::Unsupported); }
        } else if !matches!(mime, "audio/flac" | "audio/mpeg" | "audio/mp4" | "audio/wav" | "audio/aac") { return Err(Error::Unsupported); }
        Ok(())
    }
}
impl RemoteTransport for Dlna {
    fn load(&mut self, url: &str, mime: &str, m: &Metadata) -> Result<()> {
        self.accepts(mime)?;
        let didl = format!("<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"><item id=\"1\" parentID=\"0\" restricted=\"1\"><dc:title>{}</dc:title><upnp:artist>{}</upnp:artist><upnp:album>{}</upnp:album><upnp:class>object.item.audioItem.musicTrack</upnp:class><res protocolInfo=\"http-get:*:{}:{CONTENT_FEATURES}\" duration=\"{}\">{}</res></item></DIDL-Lite>", escape(&m.title), escape(&m.artist), escape(&m.album), escape(mime), time(m.duration), escape(url));
        let payload = format!("<CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>", escape(url), escape(&didl));
        self.transport_action("SetAVTransportURI", &payload)?; self.payload = Some(payload); self.fresh = true;
        self.play()
    }
    fn play(&mut self) -> Result<()> {
        if !self.fresh && self.transport_action("GetTransportInfo", "").ok().and_then(|s| s.get("CurrentTransportState").cloned()).is_some_and(|s| s == "STOPPED" || s == "NO_MEDIA_PRESENT") {
            if let Some(payload) = &self.payload { self.transport_action("SetAVTransportURI", payload)?; }
        }
        self.fresh = false;
        for attempt in 0..3 {
            match self.transport_action("Play", "<Speed>1</Speed>") {
                Ok(_) => return Ok(()),
                Err(Error::Fault(code @ (701 | 702))) if attempt < 2 => {
                    if code == 702 { if let Some(payload) = &self.payload { self.transport_action("SetAVTransportURI", payload)?; } }
                    std::thread::sleep(Duration::from_millis(300 * (attempt + 1)));
                }
                Err(e) => return Err(e),
            }
        }
        Err(Error::Protocol)
    }
    fn pause(&mut self) -> Result<()> { self.transport_action("Pause", "").map(|_| ()) }
    fn stop(&mut self) -> Result<()> { self.payload = None; self.transport_action("Stop", "").map(|_| ()) }
    fn seek(&mut self, seconds: f64) -> Result<()> { self.transport_action("Seek", &format!("<Unit>REL_TIME</Unit><Target>{}</Target>", time(seconds))).map(|_| ()) }
    fn volume(&mut self, level: f64) -> Result<()> { self.rc("SetVolume", &format!("<DesiredVolume>{}</DesiredVolume>", (level.clamp(0.0, 1.0) * 100.0).round() as u32)).map(|_| ()) }
    fn mute(&mut self, muted: bool) -> Result<()> { self.rc("SetMute", &format!("<DesiredMute>{}</DesiredMute>", u8::from(muted))).map(|_| ()) }
    fn status(&mut self) -> Result<Status> {
        let info = self.transport_action("GetTransportInfo", "")?;
        let state = match info.get("CurrentTransportState").map(String::as_str) {
            Some("PLAYING") => State::Playing, Some("PAUSED_PLAYBACK") => State::Paused, Some("TRANSITIONING") => State::Buffering, _ => State::Stopped,
        };
        let pos = self.transport_action("GetPositionInfo", "").unwrap_or_default();
        let volume = self.rc("GetVolume", "").ok().and_then(|r| r.get("CurrentVolume").and_then(|s| s.parse::<f64>().ok())).map(|v| v / 100.0);
        let muted = self.rc("GetMute", "").ok().and_then(|r| r.get("CurrentMute").map(|s| s == "1" || s == "true"));
        Ok(Status { state, position: pos.get("RelTime").and_then(|s| seconds(s)), duration: pos.get("TrackDuration").and_then(|s| seconds(s)), volume, muted })
    }
}
fn time(s: f64) -> String { let n = s.max(0.0) as u64; format!("{:02}:{:02}:{:02}", n / 3600, n / 60 % 60, n % 60) }
fn seconds(s: &str) -> Option<f64> { let p: Vec<_> = s.split(':').collect(); if p.len() != 3 { return None; } Some(p[0].parse::<f64>().ok()? * 3600.0 + p[1].parse::<f64>().ok()? * 60.0 + p[2].parse::<f64>().ok()?) }
