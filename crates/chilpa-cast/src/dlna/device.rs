//! AVTransport and RenderingControl over SOAP, ported from the reference
//! player's `qbz-cast/src/dlna/device.rs` with its timeouts, retries and
//! fallbacks. Diagnostics go to `crate::trace` with the media token redacted.
use crate::trace::{event, redact};
use http::Uri;
use rupnp::{Device, Service};
use std::{collections::HashMap, sync::atomic::{AtomicU32, Ordering}, time::Duration};

#[derive(Debug)]
pub enum DlnaError {
    Connection(String),
    Playback(String),
    /// A UPnP fault code or the raw HTTP status a renderer answered with.
    Fault(u16, String),
    Timeout(String),
    NoService(&'static str),
    NotConnected,
}
impl std::fmt::Display for DlnaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(e) => write!(f, "connection: {e}"),
            Self::Playback(e) => write!(f, "playback: {e}"),
            Self::Fault(code, e) => write!(f, "fault {code}: {e}"),
            Self::Timeout(e) => write!(f, "timeout: {e}"),
            Self::NoService(name) => write!(f, "no {name} service"),
            Self::NotConnected => f.write_str("not connected"),
        }
    }
}

/// Metadata for DLNA playback.
#[derive(Debug, Clone, Default)]
pub struct DlnaMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork_url: Option<String>,
    pub duration_secs: Option<u64>,
}

/// Playback position and transport state.
#[derive(Debug, Clone)]
pub struct DlnaPositionInfo {
    pub position_secs: u64,
    pub duration_secs: u64,
    /// PLAYING, PAUSED_PLAYBACK, STOPPED, NO_MEDIA_PRESENT, TRANSITIONING or UNKNOWN.
    pub transport_state: String,
}

/// A connected renderer with its AVTransport and RenderingControl services.
pub struct DlnaConnection {
    name: String,
    connected: bool,
    device_url: Uri,
    transport_service: Option<Service>,
    rendering_control_service: Option<Service>,
    current_uri: Option<String>,
    // Last SetAVTransportURI payload (URI + DIDL), kept so `play()` can
    // re-assert the content on a 702 "no contents" fault: some renderers clear
    // CurrentURI when a track ends, which would otherwise make a bare Play
    // (the play button, or a late auto-advance) fail permanently.
    last_set_uri_payload: Option<String>,
    // Last 0-100 volume actually sent, so a drag that quantizes to the same
    // step does not repeat the SOAP call.
    last_volume_sent: Option<u32>,
    // Set by `load_media`, cleared by `play`. When true the URI was just set,
    // so `play` skips its idle pre-check and a redundant SetAVTransportURI.
    uri_freshly_set: bool,
    is_playing: bool,
    // Consecutive GetTransportInfo failures while GetPositionInfo still
    // answers; only the first and every 60th are traced (the poll is 1 Hz).
    transport_query_failures: AtomicU32,
}

impl DlnaConnection {
    /// Reads the description at `url`, finds the services at any UPnP version
    /// and stops whatever the renderer kept from an earlier session.
    pub async fn connect(url: &str, name: &str) -> Result<Self, DlnaError> {
        let device_url: Uri = url.parse().map_err(|e| DlnaError::Connection(format!("Invalid device URL: {e}")))?;
        let parsed_device = Device::from_url(device_url.clone()).await
            .map_err(|e| DlnaError::Connection(format!("Failed to load device description: {e}")))?;
        let transport_service = find_service_any_version(&parsed_device, "AVTransport");
        let rendering_control_service = find_service_any_version(&parsed_device, "RenderingControl");
        event("cast.dlna", "connected", format_args!("avt={} rc={}", transport_service.is_some(), rendering_control_service.is_some()));
        // Clear residual transport state from a previous session or app run. A
        // renderer left mid-playback re-requests its orphaned CurrentURI
        // against the fresh media server (new token: a 403 loop) and reports a
        // stale PLAYING position that trips the end detection into advancing
        // the first cast track. Best effort: a renderer with nothing loaded
        // may reject Stop.
        if let Some(transport) = transport_service.as_ref() {
            let _ = Self::run_action(transport, &device_url, "Stop", "<InstanceID>0</InstanceID>", 5).await;
        }
        Ok(Self {
            name: name.to_string(), connected: true, device_url, transport_service, rendering_control_service,
            current_uri: None, last_set_uri_payload: None, last_volume_sent: None, uri_freshly_set: false,
            is_playing: false, transport_query_failures: AtomicU32::new(0),
        })
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
        self.current_uri = None;
        self.is_playing = false;
        event("cast.dlna", "disconnected", format_args!("{}", self.name));
    }
    pub fn is_playing(&self) -> bool { self.is_playing }
    pub fn current_uri(&self) -> Option<&str> { self.current_uri.as_deref() }
    pub fn has_rendering_control(&self) -> bool { self.rendering_control_service.is_some() }

    fn transport(&self) -> Result<&Service, DlnaError> {
        if !self.connected { return Err(DlnaError::NotConnected); }
        self.transport_service.as_ref().ok_or(DlnaError::NoService("AVTransport"))
    }
    fn rc(&self) -> Result<&Service, DlnaError> {
        if !self.connected { return Err(DlnaError::NotConnected); }
        self.rendering_control_service.as_ref().ok_or(DlnaError::NoService("RenderingControl"))
    }

    /// Sets the media URI with its DIDL-Lite. Play is a separate command.
    pub async fn load_media(&mut self, uri: &str, metadata: &DlnaMetadata, content_type: &str) -> Result<(), DlnaError> {
        let transport = self.transport()?;
        let didl_metadata = build_didl_metadata(uri, metadata, content_type);
        event("cast.dlna", "load", format_args!("uri={} content_type={content_type}", redact(uri)));
        let payload = format!(
            "<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
            xml_escape(uri), xml_escape(&didl_metadata)
        );
        tokio::time::timeout(Duration::from_secs(10), transport.action(&self.device_url, "SetAVTransportURI", &payload)).await
            .map_err(|_| {
                event("cast.dlna", "set_uri_failed", format_args!("timed out after 10s"));
                DlnaError::Timeout("SetAVTransportURI timed out".into())
            })?
            .map_err(|e| {
                event("cast.dlna", "set_uri_failed", format_args!("{e}"));
                fault(&e)
            })?;
        self.current_uri = Some(uri.to_string());
        // Remembered so play() can re-assert it on a 702 "no contents".
        self.last_set_uri_payload = Some(payload);
        self.uri_freshly_set = true;
        Ok(())
    }

    /// A SOAP action with a timeout: a hung renderer gives `Timeout` instead
    /// of blocking the caller forever.
    async fn run_action(service: &Service, device_url: &Uri, name: &str, payload: &str, timeout_secs: u64) -> Result<HashMap<String, String>, DlnaError> {
        tokio::time::timeout(Duration::from_secs(timeout_secs), service.action(device_url, name, payload)).await
            .map_err(|_| {
                event("cast.dlna", "action_failed", format_args!("action={name} timed out after {timeout_secs}s"));
                DlnaError::Timeout(format!("{name} action timed out"))
            })?
            .map_err(|e| {
                event("cast.dlna", "action_failed", format_args!("action={name} error={e}"));
                fault(&e)
            })
    }

    /// Starts or resumes playback.
    pub async fn play(&mut self) -> Result<(), DlnaError> {
        let transport = self.transport()?.clone();
        let set_uri_payload = self.last_set_uri_payload.clone();
        // PRE-CHECK. At a track's natural end some renderers finalize and clear
        // the transport, so a bare Play (the play button, a resume after a
        // stop) either faults 702 or answers 200 and does nothing. Re-assert
        // the current URI first, as the manual recovery does, but only when the
        // URI was not just set and the transport is idle (STOPPED or
        // NO_MEDIA_PRESENT): a PAUSED transport keeps its position.
        let uri_freshly_set = std::mem::replace(&mut self.uri_freshly_set, false);
        if !uri_freshly_set {
            if let Some(payload) = set_uri_payload.as_deref() {
                if Self::transport_is_idle(&transport, &self.device_url).await {
                    event("cast.dlna", "reassert_uri", format_args!("transport idle before Play"));
                    let _ = tokio::time::timeout(Duration::from_secs(10), transport.action(&self.device_url, "SetAVTransportURI", payload)).await;
                }
            }
        }
        // Retry net for the settle race at a boundary:
        //   701 "Transition not available": wait and retry Play.
        //   702 "No contents": re-assert the URI, then retry.
        // rupnp turns any non-200 answer into `HttpErrorCode` before reading
        // the SOAP body, so a renderer that wraps its fault in HTTP 500 is not
        // retried; the idle pre-check above covers those.
        const PLAY_SETTLE_CODE: u16 = 701;
        const PLAY_NO_CONTENT_CODE: u16 = 702;
        const PLAY_MAX_ATTEMPTS: u32 = 6;
        for attempt in 1..=PLAY_MAX_ATTEMPTS {
            let result = tokio::time::timeout(Duration::from_secs(10), transport.action(&self.device_url, "Play", "<InstanceID>0</InstanceID><Speed>1</Speed>")).await
                .map_err(|_| {
                    event("cast.dlna", "play_failed", format_args!("timed out after 10s"));
                    DlnaError::Timeout("Play action timed out".into())
                })?;
            let e = match result {
                Ok(_) => { self.is_playing = true; return Ok(()); }
                Err(e) => e,
            };
            let code = code_of(&e);
            let reassert_uri = match code {
                Some(PLAY_NO_CONTENT_CODE) => true,
                Some(PLAY_SETTLE_CODE) => false,
                _ => {
                    event("cast.dlna", "play_failed", format_args!("{e}"));
                    return Err(fault(&e));
                }
            };
            if attempt == PLAY_MAX_ATTEMPTS {
                event("cast.dlna", "play_failed", format_args!("still faulting after {PLAY_MAX_ATTEMPTS} attempts: {e}"));
                return Err(fault(&e));
            }
            if reassert_uri {
                if let Some(payload) = set_uri_payload.as_deref() {
                    let _ = tokio::time::timeout(Duration::from_secs(10), transport.action(&self.device_url, "SetAVTransportURI", payload)).await;
                }
            }
            // Linear backoff: ~300 ms * attempt, recovering within 1-2 s and
            // giving up after ~4.5 s.
            let backoff = Duration::from_millis(300 * attempt as u64);
            event("cast.dlna", "play_retry", format_args!("fault={e} attempt={attempt}/{PLAY_MAX_ATTEMPTS} backoff={backoff:?}"));
            tokio::time::sleep(backoff).await;
        }
        unreachable!("play retry loop exited without returning")
    }

    /// True only when the transport is idle (STOPPED or NO_MEDIA_PRESENT). Any
    /// query error or timeout is "not known to be idle", so the URI is never
    /// re-asserted under uncertainty and a paused resume is not disturbed.
    async fn transport_is_idle(transport: &Service, device_url: &Uri) -> bool {
        match tokio::time::timeout(Duration::from_secs(3), transport.action(device_url, "GetTransportInfo", "<InstanceID>0</InstanceID>")).await {
            Ok(Ok(response)) => matches!(response.get("CurrentTransportState").map(|s| s.trim().to_ascii_uppercase()).as_deref(), Some("STOPPED") | Some("NO_MEDIA_PRESENT")),
            _ => false,
        }
    }

    pub async fn pause(&mut self) -> Result<(), DlnaError> {
        let transport = self.transport()?;
        Self::run_action(transport, &self.device_url, "Pause", "<InstanceID>0</InstanceID>", 10).await?;
        self.is_playing = false;
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), DlnaError> {
        let transport = self.transport()?;
        Self::run_action(transport, &self.device_url, "Stop", "<InstanceID>0</InstanceID>", 10).await?;
        self.is_playing = false;
        self.current_uri = None;
        Ok(())
    }

    pub async fn seek(&mut self, position_secs: u64) -> Result<(), DlnaError> {
        let time_str = format!("{:02}:{:02}:{:02}", position_secs / 3600, (position_secs % 3600) / 60, position_secs % 60);
        let transport = self.transport()?;
        let payload = format!("<InstanceID>0</InstanceID><Unit>REL_TIME</Unit><Target>{time_str}</Target>");
        Self::run_action(transport, &self.device_url, "Seek", &payload, 10).await?;
        Ok(())
    }

    /// Volume from 0.0 to 1.0, sent as 0-100.
    pub async fn set_volume(&mut self, volume: f32) -> Result<(), DlnaError> {
        let rc_service = self.rc()?;
        let dlna_volume = ((volume.clamp(0.0, 1.0) * 100.0) as u32).min(100);
        if self.last_volume_sent == Some(dlna_volume) { return Ok(()); }
        let payload = format!("<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredVolume>{dlna_volume}</DesiredVolume>");
        Self::run_action(rc_service, &self.device_url, "SetVolume", &payload, 10).await?;
        self.last_volume_sent = Some(dlna_volume);
        Ok(())
    }

    pub async fn set_mute(&mut self, mute: bool) -> Result<(), DlnaError> {
        let rc_service = self.rc()?;
        let payload = format!("<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredMute>{}</DesiredMute>", if mute { 1 } else { 0 });
        Self::run_action(rc_service, &self.device_url, "SetMute", &payload, 10).await?;
        Ok(())
    }

    pub async fn get_mute(&self) -> Result<bool, DlnaError> {
        let rc_service = self.rc()?;
        let response = Self::run_action(rc_service, &self.device_url, "GetMute", "<InstanceID>0</InstanceID><Channel>Master</Channel>", 5).await?;
        Ok(response.get("CurrentMute").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false))
    }

    /// RenderingControl GetVolume, 0-100.
    pub async fn get_volume(&self) -> Result<u32, DlnaError> {
        let rc_service = self.rc()?;
        let response = Self::run_action(rc_service, &self.device_url, "GetVolume", "<InstanceID>0</InstanceID><Channel>Master</Channel>", 5).await?;
        response.get("CurrentVolume").and_then(|v| v.trim().parse().ok()).ok_or_else(|| DlnaError::Playback("GetVolume without CurrentVolume".into()))
    }

    /// Current position and transport state.
    pub async fn get_position_info(&self) -> Result<DlnaPositionInfo, DlnaError> {
        let transport = self.transport()?;
        let position_response = tokio::time::timeout(Duration::from_secs(5), transport.action(&self.device_url, "GetPositionInfo", "<InstanceID>0</InstanceID>")).await
            .map_err(|_| DlnaError::Timeout("GetPositionInfo timed out".into()))?
            .map_err(|e| fault(&e))?;
        // A renderer that answers GetPositionInfo but faults on
        // GetTransportInfo is alive, so this degrades to UNKNOWN instead of
        // failing the whole read; the caller keeps the position.
        let transport_response = match tokio::time::timeout(Duration::from_secs(5), transport.action(&self.device_url, "GetTransportInfo", "<InstanceID>0</InstanceID>")).await {
            Ok(Ok(response)) => { self.transport_query_failures.store(0, Ordering::Relaxed); Some(response) }
            Ok(Err(e)) => { self.note_transport_query_failure(&e.to_string()); None }
            Err(_) => { self.note_transport_query_failure("timed out"); None }
        };
        let position_secs = parse_time_string(position_response.get("RelTime").map(|s| s.as_str()).unwrap_or("0:00:00"));
        let duration_secs = parse_time_string(position_response.get("TrackDuration").map(|s| s.as_str()).unwrap_or("0:00:00"));
        // When the query itself faulted, fall back to the last transport
        // command sent: the position keeps flowing and the end-stall rule of
        // the poll takes over if it freezes at the end.
        let transport_state = match transport_response.as_ref().and_then(|r| r.get("CurrentTransportState")) {
            Some(state) => state.to_string(),
            None if transport_response.is_some() => "UNKNOWN".to_string(),
            None if self.is_playing => "PLAYING".to_string(),
            None => "PAUSED_PLAYBACK".to_string(),
        };
        Ok(DlnaPositionInfo { position_secs, duration_secs, transport_state })
    }

    fn note_transport_query_failure(&self, reason: &str) {
        let n = self.transport_query_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if n == 1 || n % 60 == 0 {
            event("cast.dlna", "transport_query_failed", format_args!("in_a_row={n} position_still_answers=true reason={reason}"));
        }
    }
}

fn code_of(error: &rupnp::Error) -> Option<u16> {
    match error {
        rupnp::Error::UPnPError(u) => Some(u.err_code()),
        rupnp::Error::HttpErrorCode(c) => Some(c.as_u16()),
        _ => None,
    }
}
fn fault(error: &rupnp::Error) -> DlnaError {
    match code_of(error) { Some(code) => DlnaError::Fault(code, error.to_string()), None => DlnaError::Playback(error.to_string()) }
}

/// "HH:MM:SS" or "H:MM:SS", with an optional fraction, to whole seconds.
pub fn parse_time_string(time: &str) -> u64 {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() != 3 { return 0; }
    let hours: u64 = parts[0].parse().unwrap_or(0);
    let minutes: u64 = parts[1].parse().unwrap_or(0);
    let seconds: u64 = parts[2].split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0);
    hours * 3600 + minutes * 60 + seconds
}

/// DIDL-Lite for one track, byte for byte as the reference player builds it.
pub fn build_didl_metadata(uri: &str, metadata: &DlnaMetadata, content_type: &str) -> String {
    let duration = metadata.duration_secs
        .map(|d| format!("{:02}:{:02}:{:02}", d / 3600, (d % 3600) / 60, d % 60))
        .unwrap_or_else(|| "00:00:00".to_string());
    let artwork = metadata.artwork_url.as_ref()
        .map(|url| format!(r#"<upnp:albumArtURI>{}</upnp:albumArtURI>"#, xml_escape(url)))
        .unwrap_or_default();
    // The real content type, with the same content features the media server
    // sends on GET and HEAD: strict renderers cross-check them.
    let protocol_info = format!("http-get:*:{}:{}", content_type, crate::CONTENT_FEATURES);
    format!(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">
  <item id="0" parentID="-1" restricted="1">
    <dc:title>{}</dc:title>
    <dc:creator>{}</dc:creator>
    <upnp:album>{}</upnp:album>
    <upnp:artist>{}</upnp:artist>
    {}
    <res duration="{}" protocolInfo="{}">{}</res>
    <upnp:class>object.item.audioItem.musicTrack</upnp:class>
  </item>
</DIDL-Lite>"#,
        xml_escape(&metadata.title), xml_escape(&metadata.artist), xml_escape(&metadata.album), xml_escape(&metadata.artist),
        artwork, duration, protocol_info, xml_escape(uri)
    )
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// `service_type` names `service` at any UPnP version.
pub fn service_type_matches(service_type: &str, service: &str) -> bool {
    service_type.contains(&format!(":service:{service}:"))
}

/// A service by name at any UPnP version (`:1`, `:2`, `:3`), so a renderer
/// that only advertises a newer version still connects.
pub fn find_service_any_version(device: &Device, service: &str) -> Option<Service> {
    device.services_iter().find(|s| service_type_matches(&s.service_type().to_string(), service)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_any_upnp_version() {
        for st in ["urn:schemas-upnp-org:service:AVTransport:1", "urn:schemas-upnp-org:service:AVTransport:2", "urn:schemas-upnp-org:service:AVTransport:3"] {
            assert!(service_type_matches(st, "AVTransport"), "expected {st} to match AVTransport");
        }
        assert!(!service_type_matches("urn:schemas-upnp-org:service:ConnectionManager:1", "AVTransport"));
    }
    #[test]
    fn times_parse_with_and_without_fractions() {
        assert_eq!(parse_time_string("0:01:23"), 83);
        assert_eq!(parse_time_string("01:02:03.500"), 3723);
        assert_eq!(parse_time_string("NOT_IMPLEMENTED"), 0);
    }
}
