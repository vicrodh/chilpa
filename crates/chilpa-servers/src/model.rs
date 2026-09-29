//! The words the three protocols are translated into.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::net::Request;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Subsonic,
    Jellyfin,
    Plex,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Subsonic, Kind::Jellyfin, Kind::Plex];

    /// The provider of its `SourceId`.
    pub fn provider(self) -> &'static str {
        match self {
            Kind::Subsonic => "subsonic",
            Kind::Jellyfin => "jellyfin",
            Kind::Plex => "plex",
        }
    }

    pub fn from_provider(provider: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.provider() == provider)
    }

    /// What the window calls the kind before a server of it says its name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Subsonic => "OpenSubsonic",
            Kind::Jellyfin => "Jellyfin",
            Kind::Plex => "Plex",
        }
    }

    /// Its glyph in the theme (`ui/design/01-tokens`).
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Subsonic => "server",
            Kind::Jellyfin => "server-2",
            Kind::Plex => "server-bolt",
        }
    }
}

/// How Chilpa introduces itself. Plex shows it among the user's
/// authorized devices; Jellyfin keeps one session per device id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Client {
    pub version: String,
    /// "Linux", "macOS", "Windows".
    pub platform: String,
    /// This machine's name.
    pub device: String,
    /// Opaque, one per server, kept: Jellyfin replaces the session of a
    /// device id that signs in again, and Plex lists each id as a device.
    pub id: String,
}

impl Client {
    pub const PRODUCT: &'static str = "Chilpa";
}

/// What the protocol needs to remember beyond the address, kept by the
/// library as JSON (`servers.details`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Details {
    /// Subsonic: the OpenSubsonic extensions it announced.
    pub extensions: Vec<String>,
    /// Jellyfin: the user id the token belongs to.
    pub user_id: String,
    /// Plex: every address the server can be reached at, best first. The
    /// one in use is `Server::address`; the others are tried when it stops
    /// answering.
    pub connections: Vec<String>,
}

/// A server as a request is built for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    pub kind: Kind,
    /// Scheme and host, no trailing slash: `http://music.lan:4533`.
    pub address: String,
    pub client: Client,
    /// The libraries of it that come in. Empty is all of them.
    pub libraries: Vec<String>,
    pub details: Details,
}

/// What is sealed and kept, one per server. Never logged, never in a URL
/// that is written anywhere.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Credential {
    /// Subsonic's own: every request carries md5(password + salt).
    Password { user: String, password: String },
    /// OpenSubsonic's `apiKeyAuthentication`.
    ApiKey { key: String },
    /// Jellyfin's access token. The password never reaches here.
    Token { token: String },
    /// Plex: the account's token, to find the server again, and the
    /// server's own, to talk to it.
    Plex { account: String, server: String },
}

impl std::fmt::Debug for Credential {
    /// Says which kind it is and nothing of what it holds: QBZ derived
    /// `Debug` on its credentials, and any `{:?}` printed them.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Credential::Password { .. } => "password",
            Credential::ApiKey { .. } => "api key",
            Credential::Token { .. } => "token",
            Credential::Plex { .. } => "plex tokens",
        };
        write!(f, "Credential({kind})")
    }
}

/// A library the server offers: a music folder, a view, a section.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
}

/// What a server says of itself before anyone signs in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Probe {
    /// What it is and its version: "Navidrome 0.64.2", "Jellyfin 12.1.0".
    pub product: String,
    /// Its own name, when it has one ("blitzmini").
    pub name: String,
    /// Its own id, to refuse adding it twice. Empty when it has none.
    pub identity: String,
    /// Subsonic: it takes an API key instead of a password.
    pub api_key: bool,
    /// Jellyfin: Quick Connect is on.
    pub quick_connect: bool,
    pub extensions: Vec<String>,
}

/// A track as a server describes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub album_id: String,
    pub cover: Option<String>,
    pub disc: Option<u16>,
    pub discs: Option<u16>,
    pub number: Option<u16>,
    pub year: Option<u32>,
    pub genres: Vec<String>,
    pub duration_ms: u64,
    /// The audio codec in lower case ("flac", "alac", "mp3"). Its file's
    /// extension follows from it.
    pub codec: Option<String>,
    pub bits: Option<u8>,
    pub rate: Option<u32>,
    pub channels: Option<u16>,
    pub isrc: Option<String>,
    pub recording_mbid: Option<String>,
    /// What changes when the server's copy of it does.
    pub revision: Option<String>,
    pub added_at: Option<i64>,
    /// Whether this description carries the quality. A listing that does not
    /// is completed by `details`.
    pub complete: bool,
}

/// A page of the catalogue.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub tracks: Vec<Track>,
    /// Where the next page starts. `None`: this was the last.
    pub next: Option<String>,
    /// How many tracks the library being read (`scope`) has, when the
    /// server says. The caller adds up one per scope and compares it with
    /// what it saw: paging without a tie-breaker can repeat or skip a track,
    /// and a sync that missed some must not take them out.
    pub total: Option<u64>,
    /// Which of the server's libraries this page is from.
    pub scope: String,
    /// Something the page listed could not be read: a record without its
    /// songs, an item that is not a track as it should be, a count that was
    /// not said. The sync then takes nothing out.
    pub partial: bool,
}

/// Where a track's bytes are read from.
#[derive(Clone, PartialEq, Eq)]
pub struct Stream {
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// The file's extension, for the decoder: the address has none.
    pub extension: Option<String>,
}

impl std::fmt::Debug for Stream {
    /// Without the query or the headers: both carry credentials.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = self.url.split('?').next().unwrap_or("");
        write!(f, "Stream({path}, {:?})", self.extension)
    }
}

/// Why something a server was asked did not work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Nothing came back: it is off, or not where it was.
    Unreachable(String),
    /// It answered that the credential is not good.
    Refused(String),
    /// It moved: the address it points to.
    Moved(String),
    /// It is busy with its own library and asks to wait.
    Busy(String),
    /// It answered, with something this does not understand.
    Unexpected(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Unreachable(why) | Failure::Refused(why) | Failure::Busy(why) | Failure::Unexpected(why) => write!(f, "{why}"),
            Failure::Moved(to) => write!(f, "{}", chilpa_i18n::t_args("the server moved to {}; use that address", &[to])),
        }
    }
}

impl Failure {
    /// Where a redirect points, without what could carry a credential: a
    /// Subsonic request has it in the query, and a proxy may repeat it, or
    /// put a user in front of the host. The error is shown and logged.
    pub fn moved(location: &str) -> Failure {
        let bare = location.split(['?', '#']).next().unwrap_or("");
        let bare = match bare.split_once("://") {
            Some((scheme, rest)) => {
                let (authority, path) = match rest.split_once('/') {
                    Some((authority, path)) => (authority, format!("/{path}")),
                    None => (rest, String::new()),
                };
                let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
                format!("{scheme}://{host}{path}")
            }
            None => bare.to_string(),
        };
        Failure::Moved(bare)
    }
}

impl Failure {
    /// Whether this says the server stopped answering, which leaves its
    /// records listed and unplayable.
    pub fn is_unreachable(&self) -> bool {
        matches!(self, Failure::Unreachable(_))
    }
}

/// The extension a codec's file usually has, for the decoder's hint.
pub(crate) fn extension_of(codec: &str) -> Option<&'static str> {
    Some(match codec {
        "flac" => "flac",
        "alac" | "aac" | "m4a" | "mp4" => "m4a",
        "mp3" => "mp3",
        "vorbis" | "ogg" => "ogg",
        "opus" => "opus",
        "wav" | "pcm" => "wav",
        "aiff" | "aif" => "aiff",
        "wavpack" | "wv" => "wv",
        "ape" => "ape",
        _ => return None,
    })
}

/// A request the facade sends as it is, for a cover.
pub(crate) fn image(url: String, headers: Vec<(String, String)>) -> Request {
    let mut request = Request::get(url, headers);
    request.timeout = Duration::from_secs(30);
    request
}
