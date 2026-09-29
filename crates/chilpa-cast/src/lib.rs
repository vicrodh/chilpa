//! Network renderers own their clock; I keep the queue in Chilpa.
pub mod dlna;
pub mod chromecast;
pub mod discovery;

use std::net::SocketAddr;
use serde::Serialize;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error { Network, Description, Unsupported, Protocol, Fault(u16), Entropy, Asset }

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Network => chilpa_i18n::t("The renderer did not answer. Check its connection."),
            Self::Description => chilpa_i18n::t("The renderer description could not be read."),
            Self::Unsupported => chilpa_i18n::t("The renderer does not support this format or control. Chilpa does not transcode."),
            Self::Protocol | Self::Fault(_) => chilpa_i18n::t("The renderer refused the command."),
            Self::Entropy => chilpa_i18n::t("A secure cast URL could not be created."),
            Self::Asset => chilpa_i18n::t("Casting needs an original file with a known size and byte ranges."),
        };
        f.write_str(&text)
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Kind { Dlna, Chromecast }
#[derive(Clone, Debug, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub model: String,
    pub kind: Kind,
    pub address: SocketAddr,
    pub description: String,
}
#[derive(Clone, Debug, Default)]
pub struct Metadata { pub title: String, pub artist: String, pub album: String, pub duration: f64 }
#[derive(Clone, Debug, Default, PartialEq)]
pub enum State { Playing, Paused, Buffering, #[default] Stopped, Finished }
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub state: State, pub position: Option<f64>, pub duration: Option<f64>,
    pub volume: Option<f64>, pub muted: Option<bool>,
}
/// I exchange only a public URL with the renderer, never origin credentials.
pub trait RemoteTransport {
    fn load(&mut self, url: &str, mime: &str, metadata: &Metadata) -> Result<()>;
    fn play(&mut self) -> Result<()>;
    fn pause(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn seek(&mut self, seconds: f64) -> Result<()>;
    fn volume(&mut self, level: f64) -> Result<()>;
    fn mute(&mut self, muted: bool) -> Result<()>;
    fn status(&mut self) -> Result<Status>;
    fn disconnect(&mut self) -> Result<()> { self.stop() }
}

pub const CONTENT_FEATURES: &str = "DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000";
