//! The renderer wire contract, exercised through the public connection as
//! the reference player's `dlna_wire_compatibility.rs` does: service
//! versions, SOAP paths, headers, payloads and state replies; then the
//! description tolerance and the DIDL a renderer receives.
use chilpa_cast::{dlna::{self, device::{build_didl_metadata, DlnaConnection, DlnaMetadata}, Dlna}, discovery, Device, Error, Kind, Metadata, RemoteTransport, State};
use std::{sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};

#[derive(Debug)]
struct Call { path: String, action: String, body: String }
struct Renderer { addr: std::net::SocketAddr, done: Arc<AtomicBool>, worker: Option<std::thread::JoinHandle<Vec<Call>>> }
impl Renderer {
    fn new(version: u8, empty_optional_urls: bool, relative_urls: bool) -> Self { Self::rejecting(version, empty_optional_urls, relative_urls, None) }
    /// `reject` answers one action with an HTTP status and a UPnP fault body.
    fn rejecting(version: u8, empty_optional_urls: bool, relative_urls: bool, reject: Option<(&'static str, u16)>) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let stopped = done.clone();
        let worker = std::thread::spawn(move || {
            let mut services = String::new();
            for name in ["AVTransport", "RenderingControl"] {
                let scpd = if empty_optional_urls { String::new() } else { format!("/{name}/scpd.xml") };
                let event = if empty_optional_urls { String::new() } else { format!("/{name}/event") };
                services.push_str(&format!("<service><serviceType>urn:schemas-upnp-org:service:{name}:{version}</serviceType><serviceId>urn:upnp-org:serviceId:{name}</serviceId><SCPDURL>{scpd}</SCPDURL><controlURL>/{name}/control?zone=main</controlURL><eventSubURL>{event}</eventSubURL></service>"));
            }
            if relative_urls { services = services.replace(">/", ">"); }
            let description = format!("<root xmlns=\"urn:schemas-upnp-org:device-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Reference renderer</friendlyName><manufacturer>Test</manufacturer><modelName>Reference</modelName><UDN>uuid:reference</UDN><serviceList>{services}</serviceList></device></root>");
            let mut calls = Vec::new();
            while !stopped.load(Ordering::Relaxed) {
                let Some(mut request) = server.recv_timeout(Duration::from_millis(100)).unwrap() else { continue };
                let mut status = 200;
                let response = if request.method() == &tiny_http::Method::Get {
                    assert_eq!(request.url(), "/device/description.xml");
                    description.clone()
                } else {
                    assert_eq!(request.method(), &tiny_http::Method::Post);
                    let header = request.headers().iter().find(|h| h.field.equiv("SOAPAction")).unwrap().value.as_str().trim_matches('"').to_string();
                    let (service, action) = header.rsplit_once('#').unwrap();
                    let body = match action {
                        "GetPositionInfo" => "<RelTime>00:01:23</RelTime><TrackDuration>00:05:00</TrackDuration>",
                        "GetTransportInfo" => "<CurrentTransportState>PLAYING</CurrentTransportState>",
                        "GetMute" => "<CurrentMute>1</CurrentMute>",
                        "GetVolume" => "<CurrentVolume>42</CurrentVolume>",
                        _ => "",
                    };
                    let mut response = format!("<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><u:{action}Response xmlns:u=\"{service}\">{body}</u:{action}Response></s:Body></s:Envelope>");
                    if let Some((_, code)) = reject.filter(|(rejected, _)| *rejected == action) {
                        status = code;
                        response = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail><UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>714</errorCode><errorDescription>Illegal MIME-type</errorDescription></UPnPError></detail></s:Fault></s:Body></s:Envelope>".into();
                    }
                    let mut body = String::new();
                    request.as_reader().read_to_string(&mut body).unwrap();
                    let name = if matches!(action, "SetVolume" | "SetMute" | "GetMute" | "GetVolume") { "RenderingControl" } else { "AVTransport" };
                    assert_eq!(service, format!("urn:schemas-upnp-org:service:{name}:{version}"));
                    assert_eq!(request.url(), format!("/{name}/control?zone=main"));
                    assert!(body.contains(&format!("<u:{action} xmlns:u=\"{service}\">")), "{body}");
                    calls.push(Call { path: request.url().into(), action: action.into(), body });
                    response
                };
                request.respond(tiny_http::Response::from_string(response).with_status_code(status).with_header(tiny_http::Header::from_bytes("Content-Type", "text/xml").unwrap())).unwrap();
            }
            calls
        });
        Self { addr, done, worker: Some(worker) }
    }
    fn url(&self) -> String { format!("http://{}/device/description.xml", self.addr) }
    fn device(&self) -> Device {
        Device { id: "uuid:reference".into(), name: "Reference renderer".into(), model: "Reference".into(), kind: Kind::Dlna, address: self.addr, description: self.url() }
    }
    fn finish(mut self) -> Vec<Call> {
        self.done.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap()
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}
fn actions(calls: &[Call]) -> Vec<&str> { calls.iter().map(|c| c.action.as_str()).collect() }

async fn exercise(version: u8, empty_optional_urls: bool, relative_urls: bool) {
    let renderer = Renderer::new(version, empty_optional_urls, relative_urls);
    let mut connection = DlnaConnection::connect(&renderer.url(), "Reference renderer").await.unwrap();
    let uri = "http://127.0.0.1:54321/audio/1?token=test&quality=hires";
    let metadata = DlnaMetadata { title: "Title & more".into(), artist: "Artist".into(), album: "Album".into(), artwork_url: None, duration_secs: Some(300) };
    connection.load_media(uri, &metadata, "audio/flac").await.unwrap();
    connection.play().await.unwrap();
    assert!(connection.is_playing());
    let position = connection.get_position_info().await.unwrap();
    assert_eq!((position.position_secs, position.duration_secs, position.transport_state.as_str()), (83, 300, "PLAYING"));
    connection.pause().await.unwrap();
    assert!(!connection.is_playing());
    connection.seek(83).await.unwrap();
    connection.set_volume(0.42).await.unwrap();
    connection.set_volume(0.42).await.unwrap();
    connection.set_mute(true).await.unwrap();
    assert!(connection.get_mute().await.unwrap());
    connection.stop().await.unwrap();
    assert!(connection.current_uri().is_none());
    let calls = renderer.finish();
    assert_eq!(actions(&calls), ["Stop", "SetAVTransportURI", "Play", "GetPositionInfo", "GetTransportInfo", "Pause", "Seek", "SetVolume", "SetMute", "GetMute", "Stop"]);
    assert!(calls[1].body.contains("&amp;quality=hires"));
    assert!(calls[6].body.contains("<Target>00:01:23</Target>"));
    assert!(calls[7].body.contains("<DesiredVolume>42</DesiredVolume>"));
    assert_eq!(calls[7].path, "/RenderingControl/control?zone=main");
}
#[tokio::test]
async fn normal_renderer_v1_preserves_transport_and_volume_wire_contract() { exercise(1, false, false).await; }
#[tokio::test]
async fn normal_renderer_v3_preserves_transport_and_volume_wire_contract() { exercise(3, false, false).await; }
#[tokio::test]
async fn empty_optional_urls_do_not_change_valid_control_actions() { exercise(1, true, false).await; }
#[tokio::test]
async fn relative_urls_are_normalized_to_root_relative_control_paths() { exercise(1, false, true).await; }

#[test]
fn the_transport_drives_the_renderer_as_the_reference_player_does() {
    let renderer = Renderer::new(1, false, false);
    let mut remote = Dlna::connect(&renderer.device()).unwrap();
    assert!(!remote.loaded(), "a fresh connection has nothing to stop before its first load");
    remote.load("http://127.0.0.1:9876/audio/private/1", "audio/flac", &Metadata { title: "A & B".into(), duration: 300.0, ..Default::default() }).unwrap();
    assert!(remote.loaded());
    let status = remote.status().unwrap();
    assert_eq!((status.state, status.position, status.duration), (State::Playing, Some(83.0), Some(300.0)));
    assert_eq!((status.volume, status.muted, status.transport.as_deref()), (Some(0.42), Some(true), Some("PLAYING")), "the level read once at connect, never polled");
    remote.load("http://127.0.0.1:9876/audio/private/2", "audio/flac", &Metadata::default()).unwrap();
    remote.volume(0.5).unwrap();
    assert_eq!(remote.status().unwrap().volume, Some(0.5));
    remote.disconnect().unwrap();
    let calls = renderer.finish();
    assert_eq!(actions(&calls), [
        "Stop", "GetVolume", "GetMute",
        "SetAVTransportURI", "Play",
        "GetPositionInfo", "GetTransportInfo",
        "Stop", "SetAVTransportURI", "Play",
        "SetVolume",
        "GetPositionInfo", "GetTransportInfo",
        "Stop",
    ]);
    assert!(calls[3].body.contains("A &amp;amp; B"), "{}", calls[3].body);
    assert!(calls[3].body.contains("&lt;dc:creator&gt;"), "the DIDL carries the creator as the reference player's does");
}

#[test]
fn a_rejected_uri_is_never_played_and_the_renderer_is_reset() {
    let renderer = Renderer::rejecting(1, false, false, Some(("SetAVTransportURI", 500)));
    let mut remote = Dlna::connect(&renderer.device()).unwrap();
    assert_eq!(remote.load("http://127.0.0.1:9876/audio/private/1", "audio/x-dsf", &Metadata::default()), Err(Error::Fault(500)));
    assert!(!remote.loaded());
    let calls = renderer.finish();
    assert_eq!(actions(&calls), ["Stop", "GetVolume", "GetMute", "SetAVTransportURI", "Stop"]);
}

/// A description served once on loopback, as a renderer would.
fn serve_once(body: String, status: u16) -> (String, std::thread::JoinHandle<()>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let handle = std::thread::spawn(move || {
        if let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(5)) {
            let _ = request.respond(tiny_http::Response::from_string(body).with_status_code(status).with_header(tiny_http::Header::from_bytes("Content-Type", "text/xml").unwrap()));
        }
    });
    (format!("http://{addr}/description.xml"), handle)
}
fn describe_answered(body: String, status: u16) -> chilpa_cast::Result<Device> {
    let (url, handle) = serve_once(body, status);
    let result = dlna::describe_within(&url, Duration::from_secs(4));
    handle.join().unwrap();
    result
}
fn describe(body: String) -> chilpa_cast::Result<Device> { describe_answered(body, 200) }
#[test]
fn an_address_that_answers_without_a_description_is_not_a_renderer() {
    // The registry reads Description as "not a renderer" and Network as "no answer".
    for status in [404, 500] {
        assert_eq!(describe_answered(renderer_description("/control", ""), status).unwrap_err(), Error::Description, "{status}");
    }
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/description.xml", closed.local_addr().unwrap());
    drop(closed);
    assert_eq!(dlna::describe_within(&url, Duration::from_secs(4)).unwrap_err(), Error::Network, "nobody listening is no answer");
}
#[test]
fn a_description_past_the_byte_budget_is_cut_off() {
    let started = std::time::Instant::now();
    assert_eq!(describe(format!("<root>{}</root>", "x".repeat(600 * 1024))).unwrap_err(), Error::Description);
    assert!(started.elapsed() < Duration::from_secs(4), "the budget ends the read, not the time limit");
}
fn renderer_description(control: &str, scpd: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"utf-8\"?><root xmlns=\"urn:schemas-upnp-org:device-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Legacy Renderer</friendlyName><manufacturer>Acme</manufacturer><modelName>Streamer</modelName><UDN>uuid:00000000-0000-0000-0000-000000000733</UDN><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><serviceId>urn:upnp-org:serviceId:AVTransport</serviceId><SCPDURL>{scpd}</SCPDURL><controlURL>{control}</controlURL><eventSubURL></eventSubURL></service></serviceList></device></root>")
}
#[test]
fn descriptions_the_reference_player_tolerates_still_describe_a_renderer() {
    for (control, scpd) in [("/AVTransport/control", ""), ("MediaRenderer/AVTransport/control", "MediaRenderer/AVTransport/desc.xml")] {
        let device = describe(renderer_description(control, scpd)).unwrap();
        assert_eq!((device.id.as_str(), device.name.as_str(), device.model.as_str(), device.kind), ("uuid:00000000-0000-0000-0000-000000000733", "Legacy Renderer", "Streamer", Kind::Dlna));
    }
    assert_eq!(describe(String::new()).unwrap_err(), Error::Description, "an empty body is a description error");
    for invalid in ["bad path".to_string(), "/bad path".to_string(), "x".repeat(70_000)] {
        assert_eq!(describe(renderer_description(&invalid, "")).unwrap_err(), Error::Description, "{}", &invalid[..invalid.len().min(20)]);
    }
}
#[test]
fn an_embedded_renderer_keeps_its_own_identity() {
    // A network module's root device that embeds the renderer: the stored id
    // and name are the renderer's, as they were before.
    let body = "<?xml version=\"1.0\"?><root xmlns=\"urn:schemas-upnp-org:device-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><device><deviceType>urn:schemas-acme-com:device:NetworkModule:1</deviceType><friendlyName>Module</friendlyName><manufacturer>Acme</manufacturer><modelName>Module</modelName><UDN>uuid:root</UDN><deviceList><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Living room</friendlyName><manufacturer>Acme</manufacturer><modelName>Amplifier</modelName><UDN>uuid:renderer</UDN><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><serviceId>urn:upnp-org:serviceId:AVTransport</serviceId><SCPDURL>/scpd.xml</SCPDURL><controlURL>/control</controlURL><eventSubURL>/event</eventSubURL></service></serviceList></device></deviceList></device></root>";
    let device = describe(body.into()).unwrap();
    assert_eq!((device.id.as_str(), device.name.as_str(), device.model.as_str()), ("uuid:renderer", "Living room", "Amplifier"));
    let server = body.replace("urn:schemas-upnp-org:device:MediaRenderer:1", "urn:schemas-upnp-org:device:MediaServer:1");
    assert_eq!(describe(server).unwrap_err(), Error::Description, "a device that renders nothing is not a renderer");
}
#[test]
fn the_didl_is_the_reference_players_byte_for_byte() {
    let metadata = DlnaMetadata { title: "A & B".into(), artist: "Artist".into(), album: "Album".into(), artwork_url: None, duration_secs: Some(3723) };
    // The empty line is the album art slot, indented as the reference player leaves it.
    let expected = concat!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\">\n",
        "  <item id=\"0\" parentID=\"-1\" restricted=\"1\">\n",
        "    <dc:title>A &amp; B</dc:title>\n",
        "    <dc:creator>Artist</dc:creator>\n",
        "    <upnp:album>Album</upnp:album>\n",
        "    <upnp:artist>Artist</upnp:artist>\n",
        "    \n",
        "    <res duration=\"01:02:03\" protocolInfo=\"http-get:*:audio/flac:DLNA.ORG_OP=01;DLNA.ORG_FLAGS=01700000000000000000000000000000\">http://192.168.1.2:9876/audio/token/7</res>\n",
        "    <upnp:class>object.item.audioItem.musicTrack</upnp:class>\n",
        "  </item>\n",
        "</DIDL-Lite>",
    );
    assert_eq!(build_didl_metadata("http://192.168.1.2:9876/audio/token/7", &metadata, "audio/flac"), expected);
    let unknown = build_didl_metadata("http://h/audio/t/1", &DlnaMetadata::default(), "audio/flac");
    assert!(unknown.contains(r#"<res duration="00:00:00""#), "an unknown length is announced as zero");
}

#[test]
fn ssdp_header_case_is_not_significant() {
    assert_eq!(discovery::location("HTTP/1.1 200 OK\r\nlOcAtIoN: http://127.0.0.1/device.xml\r\n"), Some("http://127.0.0.1/device.xml".into()));
}
#[test]
fn an_ssdp_reply_counts_only_for_the_host_that_sent_it() {
    // The shape a media server answered with on a LAN, 2026-11-03.
    let reply = "HTTP/1.1 200 OK\r\nLocation: http://192.168.0.69:32469/DeviceDescription.xml\r\nCache-Control: max-age=1800\r\nST: urn:schemas-upnp-org:device:MediaServer:1\r\n\r\n";
    let peer: std::net::IpAddr = "192.168.0.69".parse().unwrap();
    assert_eq!(discovery::accepted(reply, peer), Some("http://192.168.0.69:32469/DeviceDescription.xml".into()));
    assert_eq!(discovery::accepted(reply, "192.168.0.70".parse().unwrap()), None, "a reply cannot point at another host");
    assert_eq!(discovery::accepted(&reply.replace("http://", "https://"), peer), None);
    assert_eq!(discovery::accepted("HTTP/1.1 200 OK\r\nST: upnp:rootdevice\r\n\r\n", peer), None);
    assert!(discovery::search("upnp:rootdevice\r\nMAN: x", Duration::from_millis(1), &AtomicBool::new(true)).is_err(), "a target cannot add a header");
}
