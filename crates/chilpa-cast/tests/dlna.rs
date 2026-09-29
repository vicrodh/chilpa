use chilpa_cast::{dlna::{self, Dlna}, discovery, Metadata, RemoteTransport, State};
use std::{sync::{Arc, Mutex}, time::Duration};

fn description(version: u8, endpoint: &str) -> String {
    format!("<root><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Test renderer</friendlyName><modelName>Silent</modelName><UDN>uuid:test</UDN><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:{version}</serviceType><SCPDURL/><controlURL>{endpoint}</controlURL><eventSubURL/></service><service><serviceType>urn:schemas-upnp-org:service:RenderingControl:{version}</serviceType><controlURL>/volume</controlURL></service></serviceList></device></root>")
}
#[test]
fn tolerant_descriptions_keep_valid_controls_and_service_versions() {
    for version in [1, 2, 3] {
        for endpoint in ["control?zone=main", "/control?zone=main"] {
            let parsed = dlna::parse("http://127.0.0.1:99/device/description.xml", &description(version, endpoint)).unwrap();
            assert_eq!(parsed.transport.endpoint, "http://127.0.0.1:99/control?zone=main");
            assert!(parsed.transport.kind.ends_with(&version.to_string()));
            assert!(parsed.rendering.is_some());
        }
    }
    for endpoint in ["", "bad path", "http://192.0.2.1/control"] {
        assert!(dlna::parse("http://127.0.0.1:99/device.xml", &description(1, endpoint)).is_err());
    }
    assert!(dlna::parse("http://127.0.0.1:99/device.xml", "").is_err());
    assert!(dlna::parse("http://127.0.0.1:99/device.xml", &description(1, &"x".repeat(70_000))).is_err());
}
#[test]
fn ssdp_header_case_is_not_significant() {
    assert_eq!(discovery::location("HTTP/1.1 200 OK\r\nlOcAtIoN: http://127.0.0.1/device.xml\r\n"), Some("http://127.0.0.1/device.xml".into()));
}
#[test]
fn loopback_renderer_controls_metadata_state_and_no_contents_recovery() {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = calls.clone();
    let worker = std::thread::spawn(move || {
        let mut failed = false;
        while let Some(mut request) = server.recv_timeout(Duration::from_secs(2)).unwrap() {
            let action = request.headers().iter().find(|h| h.field.equiv("SOAPAction")).unwrap().value.as_str().trim_matches('"').to_owned();
            let (service, name) = action.rsplit_once('#').unwrap();
            assert!(service.ends_with(":3"));
            let mut body = String::new(); request.as_reader().read_to_string(&mut body).unwrap();
            assert!(body.contains(&format!("<u:{name} xmlns:u=\"{service}\">")));
            assert_eq!(request.url(), if name.contains("Volume") || name.contains("Mute") { "/volume" } else { "/control?zone=main" });
            recorded.lock().unwrap().push((name.to_string(), body));
            let mut status = 200;
            let content = match name {
                "Play" if !failed => { failed = true; status = 500; "<errorCode>702</errorCode>" },
                "GetTransportInfo" => "<CurrentTransportState>PLAYING</CurrentTransportState>",
                "GetPositionInfo" => "<RelTime>00:01:23</RelTime><TrackDuration>00:05:00</TrackDuration>",
                "GetVolume" => "<CurrentVolume>42</CurrentVolume>",
                "GetMute" => "<CurrentMute>1</CurrentMute>",
                _ => "",
            };
            request.respond(tiny_http::Response::from_string(format!("<response>{content}</response>")).with_status_code(status)).unwrap();
            if name == "Stop" { break; }
        }
    });
    let parsed = dlna::parse(&format!("http://{addr}/device.xml"), &description(3, "control?zone=main")).unwrap();
    let mut remote = Dlna::connect(parsed);
    remote.load("http://127.0.0.1/audio/private/1", "audio/flac", &Metadata { title: "A & B".into(), duration: 300.0, ..Default::default() }).unwrap();
    remote.pause().unwrap(); remote.play().unwrap(); remote.seek(83.0).unwrap(); remote.volume(0.42).unwrap(); remote.mute(true).unwrap();
    let status = remote.status().unwrap();
    assert_eq!(status.state, State::Playing); assert_eq!(status.position, Some(83.0)); assert_eq!(status.duration, Some(300.0)); assert_eq!(status.volume, Some(0.42)); assert_eq!(status.muted, Some(true));
    remote.stop().unwrap(); worker.join().unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(calls.iter().filter(|(n, _)| n == "SetAVTransportURI").count(), 2);
    assert!(calls[0].1.contains("A &amp;amp; B"));
    assert!(calls.iter().any(|(_, body)| body.contains("<Target>00:01:23</Target>")));
    assert!(calls.iter().any(|(_, body)| body.contains("<DesiredVolume>42</DesiredVolume>")));
}
