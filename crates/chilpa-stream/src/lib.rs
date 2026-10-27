//! HTTP streams: radio, and anything else that arrives over the network.
//!
//! A stream is not a file: it has no known end, cannot be seeked, and may
//! cut off mid-song. This crate wraps it so the rest of the engine does not
//! have to know, and keeps the two things that do change the experience:
//!
//! - **ICY metadata** (`StreamTitle`), which is the only thing that says
//!   what is playing on a radio.
//! - **The format the server announces**, to pick the decoder without
//!   guessing from the extension of a URL that often has none.

pub mod network;
pub mod icy;
pub mod patience;
pub mod request;
pub mod fetch;
pub mod download;
pub mod ranged;
pub mod server;

pub use icy::IcyReader;
pub use patience::Patience;
pub use request::send as send_request;
mod worker;
pub use worker::{send_worker_request, worker_url_allowed, WorkerNetworkError};
pub use fetch::{fetch, fetch_oauth, permits_authorization, Fetch, FetchError, Fetched};
pub use ranged::{Ranged, RangedError, RangedRequest};
pub use server::{call as call_server, Call};

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum StreamError {
    /// The URL could not be opened.
    Open(String),
    /// The server answered with something that is not audio.
    NotAudio(String),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Open(s) => {
                write!(f, "{}", chilpa_i18n::t_args("could not open the stream: {}", &[s]))
            }
            StreamError::NotAudio(s) => {
                write!(f, "{}", chilpa_i18n::t_args("the server sent no audio: {}", &[s]))
            }
        }
    }
}

/// What is known about a stream when it opens.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct StreamInfo {
    /// The station's name, when it sends one.
    pub name: Option<String>,
    /// The content type: `audio/mpeg`, `audio/flac`, …
    pub content_type: Option<String>,
    /// The extension that goes with it, to help the decoder.
    pub hint: Option<String>,
    /// How many bytes go between metadata blocks, if the server offers them.
    pub icy_metaint: Option<usize>,
}

impl StreamInfo {
    /// The extension handed to the format prober.
    pub fn extension_hint(&self) -> Option<String> {
        self.hint.clone().or_else(|| {
            let kind = self.content_type.as_deref()?;
            Some(
                match kind.split(';').next()?.trim() {
                    "audio/mpeg" | "audio/mp3" => "mp3",
                    "audio/aac" | "audio/aacp" => "aac",
                    "audio/flac" | "audio/x-flac" => "flac",
                    "audio/ogg" | "application/ogg" => "ogg",
                    "audio/wav" | "audio/x-wav" => "wav",
                    _ => return None,
                }
                .to_string(),
            )
        })
    }
}

/// What changes while it plays: today, the title the station sends.
#[derive(Clone, Debug, Default)]
pub struct NowPlaying(Arc<Mutex<Option<String>>>);

impl NowPlaying {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, title: Option<String>) {
        if let Ok(mut g) = self.0.lock() {
            *g = title;
        }
    }

    pub fn get(&self) -> Option<String> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
}

/// An open stream, ready for the decoder to read.
///
/// Implements `Seek` because the decoder demands it, but only answers the
/// position: a stream cannot rewind, and pretending it can produces corrupt
/// audio instead of a clear error.
pub struct HttpStream {
    inner: Box<dyn Read + Send + Sync>,
    info: StreamInfo,
    now_playing: NowPlaying,
    position: u64,
}

impl HttpStream {
    /// Builds the stream from a reader that is already open.
    pub fn new(
        inner: Box<dyn Read + Send + Sync>,
        info: StreamInfo,
        now_playing: NowPlaying,
    ) -> Self {
        Self { inner, info, now_playing, position: 0 }
    }

    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// The title the station sends, if it sends one.
    pub fn now_playing(&self) -> NowPlaying {
        self.now_playing.clone()
    }
}

impl Read for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if network::current().offline { return Err(io::Error::new(io::ErrorKind::PermissionDenied, network::blocked())); }
        let n = self.inner.read(buf)?;
        self.position += n as u64;
        Ok(n)
    }
}

impl Seek for HttpStream {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match pos {
            // Staying where you are is the only seek there is.
            SeekFrom::Current(0) => Ok(self.position),
            SeekFrom::Start(n) if n == self.position => Ok(self.position),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                chilpa_i18n::t("a stream cannot be rewound"),
            )),
        }
    }
}

impl symphonia::core::io::MediaSource for HttpStream {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// Reads several URLs one after another, as if they were one file.
///
/// It is the normal case in hi-res: a source signs one URL per part, and the
/// parts are the same file cut up, not segments of a manifest. Chained means
/// chained at the byte level: the decoder asks for bytes, and when a part
/// runs out the next one opens without it noticing.
///
/// **This is the reading of the manifest, not a measurement.** It was
/// verified against a local server that serves a file in pieces, which is
/// what the shape of the data says a real source does; it has not been
/// verified against a real service. If a source ever hands over parts that
/// are whole files with their own headers, this will play the first one and
/// then produce noise, so a part that does not open at all fails loudly
/// rather than being skipped: half a track and silence is a symptom someone
/// can report, and a corrupt one is not.
struct Chained {
    /// The parts not yet opened, in order, reversed so the next one pops.
    rest: Vec<String>,
    current: Box<dyn Read + Send + Sync>,
}

impl Read for Chained {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let n = self.current.read(buf)?;
            if n > 0 {
                return Ok(n);
            }
            // A part that ends is not the end of the track: the next one
            // opens here, and only running out of parts is the end.
            let Some(next) = self.rest.pop() else { return Ok(0) };
            self.current = body_of(&next).map_err(|e| {
                io::Error::other(chilpa_i18n::t_args(
                    "the next part of the track could not be opened: {}",
                    &[&e.to_string()],
                ))
            })?;
        }
    }
}

/// Opens a URL and hands back only its body.
fn body_of(url: &str) -> Result<Box<dyn Read + Send + Sync>, StreamError> {
    let patience = Patience::default();
    let reply = patience
        .agent()
        .get(url)
        .header("User-Agent", "Chilpa")
        .call()
        .map_err(|e| StreamError::Open(e.to_string()))?;
    Ok(Box::new(patience::Watched::new(reply.into_body().into_reader(), patience.silence)))
}

/// Opens a track that comes in several parts and reads them in order.
///
/// The first part is opened here so what comes back already knows its
/// content type; the rest open as the decoder reaches them. An empty list is
/// the caller's mistake and says so.
pub fn open_parts(urls: &[String]) -> Result<HttpStream, StreamError> {
    let Some((first, rest)) = urls.split_first() else {
        return Err(StreamError::Open(chilpa_i18n::t("there is no part to open")));
    };
    let mut stream = open(first)?;
    if rest.is_empty() {
        return Ok(stream);
    }
    let info = stream.info.clone();
    let now_playing = stream.now_playing.clone();
    let mut pending: Vec<String> = rest.to_vec();
    pending.reverse();
    let first_body = std::mem::replace(&mut stream.inner, Box::new(io::empty()));
    Ok(HttpStream::new(
        Box::new(Chained { rest: pending, current: first_body }),
        info,
        now_playing,
    ))
}

/// Opens a URL and leaves the audio ready for the decoder.
///
/// Asks for ICY metadata: if the server offers it, the reader takes it out
/// of the stream and publishes the title; if not, the audio passes through.
pub fn open(url: &str) -> Result<HttpStream, StreamError> {
    open_with(url, Patience::default())
}

/// The same, saying how long each wait may last.
pub fn open_with(url: &str, patience: Patience) -> Result<HttpStream, StreamError> {
    let reply = patience
        .agent()
        .get(url)
        .header("Icy-MetaData", "1")
        .header("User-Agent", "Chilpa")
        .call()
        .map_err(|e| StreamError::Open(e.to_string()))?;

    let header = |name: &str| reply.headers().get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let info = StreamInfo {
        name: header("icy-name"),
        content_type: header("content-type"),
        hint: None,
        icy_metaint: header("icy-metaint").and_then(|v| v.parse().ok()),
    };
    if let Some(kind) = &info.content_type {
        if kind.starts_with("text/") || kind.contains("html") {
            return Err(StreamError::NotAudio(kind.clone()));
        }
    }

    let now_playing = NowPlaying::new();
    let body = patience::Watched::new(reply.into_body().into_reader(), patience.silence);
    let reader: Box<dyn Read + Send + Sync> = match info.icy_metaint {
        Some(metaint) if metaint > 0 => {
            Box::new(IcyReader::new(body, metaint, now_playing.clone()))
        }
        _ => Box::new(body),
    };
    Ok(HttpStream::new(reader, info, now_playing))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_content_type_suggests_the_extension() {
        let mut info = StreamInfo { content_type: Some("audio/mpeg".into()), ..Default::default() };
        assert_eq!(info.extension_hint().as_deref(), Some("mp3"));
        info.content_type = Some("audio/flac; charset=utf-8".into());
        assert_eq!(info.extension_hint().as_deref(), Some("flac"));
        info.content_type = Some("text/html".into());
        assert_eq!(info.extension_hint(), None);
    }

    #[test]
    fn a_stream_does_not_rewind() {
        let data: Vec<u8> = (0..100u8).collect();
        let mut s = HttpStream::new(
            Box::new(std::io::Cursor::new(data)),
            StreamInfo::default(),
            NowPlaying::new(),
        );
        let mut buf = [0u8; 10];
        assert_eq!(s.read(&mut buf).unwrap(), 10);
        assert_eq!(s.seek(SeekFrom::Current(0)).unwrap(), 10, "it does say where it is");
        assert!(s.seek(SeekFrom::Start(0)).is_err(), "but it does not go back");
    }

    #[test]
    fn the_station_title_is_shared_across_threads() {
        let np = NowPlaying::new();
        let copy = np.clone();
        std::thread::spawn(move || copy.set(Some("A song".into()))).join().unwrap();
        assert_eq!(np.get().as_deref(), Some("A song"));
    }
}
