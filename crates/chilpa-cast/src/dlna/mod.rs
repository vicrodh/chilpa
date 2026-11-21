//! DLNA renderers, controlled the way the reference player controls them:
//! `rupnp` for descriptions and SOAP, each call driven to completion on a
//! private current-thread runtime, so the worker stays synchronous and no
//! thread outlives a call.
pub mod device;

use crate::{trace::event, Device, Error, Kind, Metadata, RemoteTransport, Result, State, Status};
use device::{DlnaConnection, DlnaError, DlnaMetadata};
use std::{future::Future, time::Duration};

/// The reference player's budget for reading a description and stopping
/// what the renderer kept from an earlier session.
const CONNECT_BUDGET: Duration = Duration::from_secs(10);
/// The reference player's budget for Stop and disconnect at teardown.
const TEARDOWN_BUDGET: Duration = Duration::from_millis(1500);
/// The one read of the receiver's volume and mute at connect, both together:
/// a silent RenderingControl must not hold the connection past this.
const LEVEL_BUDGET: Duration = Duration::from_secs(2);

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_| Error::Network)
}
fn error(e: DlnaError) -> Error {
    match e {
        DlnaError::Fault(code, _) => Error::Fault(code),
        DlnaError::Timeout(_) | DlnaError::Connection(_) | DlnaError::NotConnected => Error::Network,
        DlnaError::NoService("AVTransport") => Error::Description,
        DlnaError::NoService(_) => Error::Unsupported,
        DlnaError::Playback(_) => Error::Protocol,
    }
}

/// One description GET, within `limit`: the renderer it describes. The id,
/// name and model are those of the MediaRenderer device itself, embedded or
/// root, so a renderer stored before keeps its identity.
pub fn describe_within(location: &str, limit: Duration) -> Result<Device> {
    let url = location.parse::<http::Uri>().map_err(|_| Error::Description)?;
    if url.scheme_str() != Some("http") || url.authority().is_none_or(|a| a.as_str().contains('@')) { return Err(Error::Description); }
    let ip = url.host().ok_or(Error::Description)?.trim_matches(['[', ']']).parse().map_err(|_| Error::Description)?;
    let address = std::net::SocketAddr::new(ip, url.port_u16().unwrap_or(80));
    let parsed = runtime()?.block_on(async { tokio::time::timeout(limit, rupnp::Device::from_url(url)).await })
        .map_err(|_| Error::Network)?
        .map_err(|e| match e {
            // An address that answers, but not with a description, is not a renderer.
            rupnp::Error::HttpErrorCode(_) | rupnp::Error::XmlError(_) | rupnp::Error::XmlMissingElement(..) | rupnp::Error::ParseError(_) | rupnp::Error::InvalidResponse(_) | rupnp::Error::InvalidUtf8(_) => Error::Description,
            _ => Error::Network,
        })?;
    let renderer = |spec: &rupnp::DeviceSpec| spec.device_type().to_string().contains(":device:MediaRenderer:");
    let spec: &rupnp::DeviceSpec = if renderer(&parsed) { &parsed } else { parsed.devices_iter().find(|d| renderer(d)).ok_or(Error::Description)? };
    if device::find_service_any_version(&parsed, "AVTransport").is_none() || spec.udn().trim().is_empty() { return Err(Error::Description); }
    Ok(Device {
        id: spec.udn().trim().into(), name: spec.friendly_name().trim().into(), model: spec.model_name().trim().into(),
        kind: Kind::Dlna, address, description: location.into(),
    })
}
pub fn describe(location: &str) -> Result<Device> { describe_within(location, Duration::from_secs(4)) }

pub struct Dlna {
    runtime: tokio::runtime::Runtime,
    connection: DlnaConnection,
    /// A track was handed to the renderer and not stopped since: replacing it
    /// sends Stop first, as the reference player does.
    loaded: bool,
    /// The renderer's level when connected, then the last level sent. It is
    /// not polled: the reference player only ever sets it.
    volume: Option<f64>,
    muted: Option<bool>,
}
impl Dlna {
    pub fn connect(device: &Device) -> Result<Self> {
        let runtime = runtime()?;
        let connection = runtime.block_on(async {
            tokio::time::timeout(CONNECT_BUDGET, DlnaConnection::connect(&device.description, &device.name)).await
        }).map_err(|_| Error::Network)?.map_err(error)?;
        // The slider starts where the receiver is. One read each, never again.
        let (volume, muted) = if connection.has_rendering_control() {
            runtime.block_on(async { tokio::time::timeout(LEVEL_BUDGET, async { (connection.get_volume().await.ok(), connection.get_mute().await.ok()) }).await.unwrap_or((None, None)) })
        } else { (None, None) };
        Ok(Self { runtime, connection, loaded: false, volume: volume.map(|v| f64::from(v.min(100)) / 100.0), muted })
    }
    fn run<T>(&mut self, call: impl FnOnce(&mut DlnaConnection) -> std::pin::Pin<Box<dyn Future<Output = std::result::Result<T, DlnaError>> + '_>>) -> Result<T> {
        let Self { runtime, connection, .. } = self;
        runtime.block_on(call(connection)).map_err(error)
    }
}
impl RemoteTransport for Dlna {
    fn load(&mut self, url: &str, mime: &str, m: &Metadata) -> Result<()> {
        // A loaded renderer is stopped first: some ignore SetAVTransportURI
        // while paused.
        if self.loaded { let _ = self.stop(); }
        let metadata = DlnaMetadata {
            title: m.title.clone(), artist: m.artist.clone(), album: m.album.clone(), artwork_url: None,
            duration_secs: (m.duration.is_finite() && m.duration > 0.0).then_some(m.duration as u64),
        };
        let (url, mime) = (url.to_string(), mime.to_string());
        let result = self.run(|c| Box::pin(async move { c.load_media(&url, &metadata, &mime).await?; c.play().await }));
        match result {
            Ok(()) => { self.loaded = true; Ok(()) }
            Err(e) => {
                // Best-effort reset, so the renderer does not sit half-loaded
                // on a URI it already faulted.
                let _ = self.run(|c| Box::pin(c.stop()));
                self.loaded = false;
                Err(e)
            }
        }
    }
    fn loaded(&self) -> bool { self.loaded }
    fn play(&mut self) -> Result<()> { self.run(|c| Box::pin(c.play())) }
    fn pause(&mut self) -> Result<()> { self.run(|c| Box::pin(c.pause())) }
    fn stop(&mut self) -> Result<()> { self.loaded = false; self.run(|c| Box::pin(c.stop())) }
    fn seek(&mut self, seconds: f64) -> Result<()> {
        let position = seconds.max(0.0) as u64;
        self.run(|c| Box::pin(c.seek(position)))
    }
    fn volume(&mut self, level: f64) -> Result<()> {
        let value = level.clamp(0.0, 1.0);
        self.run(|c| Box::pin(c.set_volume(value as f32)))?;
        self.volume = Some(value);
        Ok(())
    }
    fn mute(&mut self, muted: bool) -> Result<()> {
        self.run(|c| Box::pin(c.set_mute(muted)))?;
        self.muted = Some(muted);
        Ok(())
    }
    /// GetPositionInfo and GetTransportInfo, as the reference player polls
    /// them; a failed position read is a failed poll.
    fn status(&mut self) -> Result<Status> {
        let info = self.run(|c| Box::pin(async move { c.get_position_info().await }))?;
        let transport = info.transport_state.trim().to_ascii_uppercase();
        let state = match transport.as_str() {
            "PLAYING" => State::Playing,
            "TRANSITIONING" => State::Buffering,
            "STOPPED" | "NO_MEDIA_PRESENT" => State::Stopped,
            // The reference player shows anything else as not playing.
            _ => State::Paused,
        };
        Ok(Status {
            state, position: Some(info.position_secs as f64),
            duration: (info.duration_secs > 0).then_some(info.duration_secs as f64),
            volume: self.volume, muted: self.muted, transport: Some(transport),
        })
    }
    fn disconnect(&mut self) -> Result<()> {
        self.loaded = false;
        let Self { runtime, connection, .. } = self;
        let stopped = runtime.block_on(async { tokio::time::timeout(TEARDOWN_BUDGET, connection.stop()).await });
        connection.disconnect();
        if !matches!(stopped, Ok(Ok(()))) { event("cast.dlna", "teardown_unconfirmed", format_args!("stop={stopped:?}")); }
        match stopped { Ok(result) => result.map_err(error), Err(_) => Err(Error::Network) }
    }
}
