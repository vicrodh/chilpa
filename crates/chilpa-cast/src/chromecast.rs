use crate::{Error, Metadata, RemoteTransport, Result, State, Status};
use serde_json::{json, Value};
use std::{net::{SocketAddr, TcpStream}, time::{Duration, Instant}};
#[path = "cast_wire.rs"]
mod wire;
const RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
const MEDIA: &str = "urn:x-cast:com.google.cast.media";
const CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
const HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";

pub struct Chromecast {
    stream: native_tls::TlsStream<TcpStream>, request: u64,
    transport: String, session: String, media_session: Option<i64>,
}
impl Chromecast {
    pub fn connect(address: SocketAddr) -> Result<Self> {
        let socket = TcpStream::connect_timeout(&address, Duration::from_secs(4)).map_err(|_| Error::Network)?;
        socket.set_read_timeout(Some(Duration::from_secs(4))).map_err(|_| Error::Network)?;
        socket.set_write_timeout(Some(Duration::from_secs(4))).map_err(|_| Error::Network)?;
        // Cast uses self-signed device certificates, including X.509 v1.
        // I trust the selected LAN peer, not a public CA or DNS identity.
        // The TLS backend still checks handshake signatures. No origin
        // credential crosses this unauthenticated LAN connection.
        let tls = native_tls::TlsConnector::builder().danger_accept_invalid_certs(true).danger_accept_invalid_hostnames(true).build().map_err(|_| Error::Network)?;
        let stream = tls.connect(&address.ip().to_string(), socket).map_err(|_| Error::Network)?;
        let mut cast = Self { stream, request: 0, transport: String::new(), session: String::new(), media_session: None };
        cast.send("receiver-0", CONNECTION, &json!({"type":"CONNECT"}))?;
        Ok(cast)
    }
    fn send(&mut self, destination: &str, namespace: &str, payload: &Value) -> Result<()> {
        wire::send(&mut self.stream, "sender-chilpa", destination, namespace, payload)
    }
    fn ask(&mut self, destination: &str, namespace: &str, mut payload: Value) -> Result<Value> {
        self.request += 1;
        payload["requestId"] = self.request.into();
        self.send(destination, namespace, &payload)?;
        let deadline = Instant::now() + Duration::from_secs(6);
        while Instant::now() < deadline {
            let (source, channel, answer) = wire::receive(&mut self.stream)?;
            if channel == HEARTBEAT && answer["type"] == "PING" { self.send(&source, HEARTBEAT, &json!({"type":"PONG"}))?; continue; }
            if channel == CONNECTION && answer["type"] == "CLOSE" { return Err(Error::Network); }
            if channel != namespace || answer["requestId"].as_u64() != Some(self.request) { continue; }
            if matches!(answer["type"].as_str(), Some("LOAD_FAILED" | "LOAD_CANCELLED" | "INVALID_REQUEST" | "LAUNCH_ERROR")) { return Err(Error::Unsupported); }
            return Ok(answer);
        }
        Err(Error::Network)
    }
    fn media(&mut self, mut payload: Value) -> Result<Value> {
        if let Some(id) = self.media_session { payload["mediaSessionId"] = id.into(); }
        let target = self.transport.clone();
        if target.is_empty() { return Err(Error::Protocol); }
        self.ask(&target, MEDIA, payload)
    }
}
impl RemoteTransport for Chromecast {
    fn load(&mut self, url: &str, mime: &str, m: &Metadata) -> Result<()> {
        if !matches!(mime, "audio/flac" | "audio/mpeg" | "audio/mp4" | "audio/wav" | "audio/aac" | "audio/ogg" | "audio/opus") { return Err(Error::Unsupported); }
        if self.transport.is_empty() {
            let reply = self.ask("receiver-0", RECEIVER, json!({"type":"LAUNCH","appId":"CC1AD845"}))?;
            let app = reply["status"]["applications"].as_array().and_then(|a| a.iter().find(|a| a["appId"] == "CC1AD845")).ok_or(Error::Protocol)?;
            self.transport = app["transportId"].as_str().ok_or(Error::Protocol)?.into();
            self.session = app["sessionId"].as_str().ok_or(Error::Protocol)?.into();
            self.send(&self.transport.clone(), CONNECTION, &json!({"type":"CONNECT"}))?;
        }
        self.media_session = None;
        let reply = self.media(json!({"type":"LOAD","sessionId":self.session,"autoplay":true,"currentTime":0,"media":{"contentId":url,"contentType":mime,"streamType":"BUFFERED","metadata":{"metadataType":3,"title":m.title,"artist":m.artist,"albumName":m.album}}}))?;
        self.media_session = reply["status"][0]["mediaSessionId"].as_i64();
        if self.media_session.is_none() { return Err(Error::Protocol); }
        if reply["status"][0]["idleReason"] == "ERROR" { return Err(Error::Unsupported); }
        Ok(())
    }
    fn play(&mut self) -> Result<()> { self.media(json!({"type":"PLAY"})).map(|_| ()) }
    fn pause(&mut self) -> Result<()> { self.media(json!({"type":"PAUSE"})).map(|_| ()) }
    fn stop(&mut self) -> Result<()> {
        if self.media_session.is_none() { return Ok(()); }
        let result = self.media(json!({"type":"STOP"})).map(|_| ()); self.media_session = None; result
    }
    fn seek(&mut self, seconds: f64) -> Result<()> { self.media(json!({"type":"SEEK","currentTime":seconds.max(0.0)})).map(|_| ()) }
    fn volume(&mut self, level: f64) -> Result<()> { self.ask("receiver-0", RECEIVER, json!({"type":"SET_VOLUME","volume":{"level":level.clamp(0.0,1.0)}})).map(|_| ()) }
    fn mute(&mut self, muted: bool) -> Result<()> { self.ask("receiver-0", RECEIVER, json!({"type":"SET_VOLUME","volume":{"muted":muted}})).map(|_| ()) }
    fn status(&mut self) -> Result<Status> {
        self.send("receiver-0", HEARTBEAT, &json!({"type":"PING"}))?;
        let receiver = self.ask("receiver-0", RECEIVER, json!({"type":"GET_STATUS"}))?;
        let reply = if self.media_session.is_some() { self.media(json!({"type":"GET_STATUS"}))? } else { json!({}) };
        let entry = &reply["status"][0];
        if entry["idleReason"] == "ERROR" { return Err(Error::Unsupported); }
        let state = match entry["playerState"].as_str() {
            Some("PLAYING") => State::Playing, Some("PAUSED") => State::Paused,
            Some("BUFFERING" | "LOADING") => State::Buffering,
            Some("IDLE") if entry["idleReason"] == "FINISHED" => State::Finished, _ => State::Stopped,
        };
        Ok(Status { state, position: entry["currentTime"].as_f64(), duration: entry["media"]["duration"].as_f64(), volume: receiver["status"]["volume"]["level"].as_f64(), muted: receiver["status"]["volume"]["muted"].as_bool() })
    }
    fn disconnect(&mut self) -> Result<()> {
        if !self.session.is_empty() { self.ask("receiver-0", RECEIVER, json!({"type":"STOP","sessionId":self.session}))?; }
        self.send("receiver-0", CONNECTION, &json!({"type":"CLOSE"}))
    }
}
