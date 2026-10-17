//! A file on a server, read by windows (ADR-017).
//!
//! The file is the original, byte for byte, and it is read the way a file
//! on disk is: from anywhere, in any order. Each window is its own request
//! with `Range`, so:
//!
//! - **a seek is a new window**, not a download of everything before it;
//! - **the length is known** from the first answer, and with it the
//!   duration and the end;
//! - **no request has a deadline for the whole file.** QBZ gave the whole
//!   body 300 s, and a long hi-res file on a slow link was cut off halfway.
//!   Here each window has its own, and a window is a few seconds of music.
//!
//! While one window plays, the next is already on its way: the device holds
//! half a second, and a window that was only asked for when it was needed
//! would arrive late on anything slower than the LAN.
//!
//! What comes back has to be audio. A Subsonic server that refuses a stream
//! answers HTTP 200 with a JSON or XML error, and a server that converts on
//! the fly answers without a length. Both are refused here, before a
//! decoder is handed a page of text as music.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::time::Duration;

use crate::server;

mod diagnostics;
use diagnostics::MemoryTrace;

/// The most a request asks for: a few seconds of hi-res.
pub const WINDOW: u64 = 4 * 1024 * 1024;

/// What the first request after opening or a seek asks for. Playing starts
/// once it is in hand, so it is small; each window after it is twice the
/// one before, up to [`WINDOW`], and is on its way while the one in hand
/// plays.
pub const FIRST: u64 = 512 * 1024;

/// The largest file taken whole from a server that cannot send part of one.
const WHOLE_MAX: u64 = 512 * 1024 * 1024;

/// Where the file is and what to send with each request.
#[derive(Clone, Debug)]
pub struct RangedRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug)]
pub enum RangedError {
    /// Nothing came back, or an error status. Text the window can show.
    Refused(String),
    /// Something came back that is not the file.
    NotAudio(String),
}

impl std::fmt::Display for RangedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RangedError::Refused(why) | RangedError::NotAudio(why) => write!(f, "{why}"),
        }
    }
}

type Window = Result<(u64, Vec<u8>), String>;

pub struct Ranged {
    agent: ureq::Agent,
    request: RangedRequest,
    len: u64,
    content_type: Option<String>,
    position: u64,
    /// The window in hand: where it starts and its bytes.
    start: u64,
    bytes: Vec<u8>,
    /// The next window, already asked for: where it starts, and where it
    /// will arrive.
    ahead: Option<(u64, Mutex<Receiver<Window>>)>,
    trace: MemoryTrace,
}

/// How long a window may take. It is a few seconds of music; ten times that
/// is a server or a link that cannot keep up, and saying so beats a player
/// that waits forever.
const PER_WINDOW: Duration = Duration::from_secs(45);

impl Ranged {
    /// Asks for the first window, and with it the length and the type.
    pub fn open(request: RangedRequest) -> Result<Self, RangedError> {
        let agent = server::agent(Duration::from_secs(20), PER_WINDOW);
        let trace = MemoryTrace::open();
        let reply = get(&agent, &request, 0, FIRST, trace).map_err(RangedError::Refused)?;
        let Reply { status, total, content_type, body, chunked } = reply;
        if !(200..300).contains(&status) {
            return Err(RangedError::Refused(chilpa_i18n::t_args("the server answered {}", &[&status.to_string()])));
        }
        if let Some(kind) = content_type.as_deref() {
            if !is_audio_type(kind) {
                return Err(RangedError::NotAudio(chilpa_i18n::t("the server answered with a message instead of the file")));
            }
        }
        let Some(len) = total else {
            let why = if chunked {
                chilpa_i18n::t("the server is converting the file instead of sending it as it is")
            } else {
                chilpa_i18n::t("the server did not say how long the file is")
            };
            return Err(RangedError::NotAudio(why));
        };
        if looks_like_a_message(&body) {
            return Err(RangedError::NotAudio(chilpa_i18n::t("the server answered with a message instead of the file")));
        }
        let mut ranged = Self { agent, request, len, content_type, position: 0, start: 0, bytes: body, ahead: None, trace };
        ranged.ask_ahead();
        Ok(ranged)
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// Starts fetching the window after the one in hand, unless it is
    /// already coming or there is nothing after it.
    fn ask_ahead(&mut self) {
        let next = self.start + self.bytes.len() as u64;
        if next >= self.len || self.ahead.as_ref().is_some_and(|(at, _)| *at == next) {
            return;
        }
        let size = (self.bytes.len() as u64 * 2).clamp(FIRST, WINDOW);
        let (tx, rx) = channel();
        let (agent, request) = (self.agent.clone(), self.request.clone());
        let trace = self.trace;
        let _ = std::thread::Builder::new().name("chilpa-window".into()).spawn(move || {
            let _ = tx.send(window(&agent, &request, next, size, trace));
        });
        self.ahead = Some((next, Mutex::new(rx)));
    }

    /// Puts the window that holds `position` in hand.
    fn load(&mut self, position: u64) -> io::Result<()> {
        let arrived = match self.ahead.take() {
            Some((at, rx)) if at == position => rx
                .into_inner()
                .unwrap_or_else(|e| e.into_inner())
                .recv()
                .map_err(|_| chilpa_i18n::t("the connection failed: interrupted"))
                .and_then(|w| w),
            _ => window(&self.agent, &self.request, position, FIRST, self.trace),
        };
        let (start, bytes) = arrived.map_err(io::Error::other)?;
        if bytes.is_empty() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, chilpa_i18n::t("the server stopped sending the file")));
        }
        self.start = start;
        self.bytes = bytes;
        self.ask_ahead();
        Ok(())
    }
}

impl Drop for Ranged {
    fn drop(&mut self) {
        self.trace.close(self.bytes.len(), self.bytes.capacity(), self.ahead.is_some());
    }
}

impl Read for Ranged {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let end = self.start + self.bytes.len() as u64;
        if self.position < self.start || self.position >= end {
            self.load(self.position)?;
        }
        let offset = (self.position - self.start) as usize;
        let n = buf.len().min(self.bytes.len() - offset);
        buf[..n].copy_from_slice(&self.bytes[offset..offset + n]);
        self.position += n as u64;
        Ok(n)
    }
}

impl Seek for Ranged {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(n) => self.len as i128 + n as i128,
            SeekFrom::Current(n) => self.position as i128 + n as i128,
        };
        if target < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"));
        }
        // Past the end is allowed, like a file: reading there gives nothing.
        self.position = target as u64;
        Ok(self.position)
    }
}

impl symphonia::core::io::MediaSource for Ranged {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

struct Reply {
    status: u16,
    total: Option<u64>,
    content_type: Option<String>,
    body: Vec<u8>,
    chunked: bool,
}

fn window(agent: &ureq::Agent, request: &RangedRequest, start: u64, size: u64, trace: MemoryTrace) -> Window {
    let reply = get(agent, request, start, size, trace)?;
    if reply.status == 206 {
        return Ok((start, reply.body));
    }
    if reply.status == 200 {
        // The server sends the whole file instead of the window. It is the
        // file, so it can still be read: the window is cut out of it.
        let from = (start as usize).min(reply.body.len());
        let to = (start + size).min(reply.body.len() as u64) as usize;
        return Ok((start, reply.body[from..to].to_vec()));
    }
    Err(chilpa_i18n::t_args("the server answered {}", &[&reply.status.to_string()]))
}

fn get(agent: &ureq::Agent, request: &RangedRequest, start: u64, size: u64, trace: MemoryTrace) -> Result<Reply, String> {
    crate::network::check(&request.url)?;
    let mut builder = agent.get(&request.url);
    for (name, value) in &request.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    let reply = builder
        .header("Range", format!("bytes={}-{}", start, start + size - 1))
        .header("Accept-Encoding", "identity")
        .call()
        .map_err(|e| server::describe(&e))?;
    let status = reply.status().as_u16();
    let header = |name: &str| reply.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let content_type = header("content-type");
    let chunked = header("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked"));
    let total = match status {
        // "bytes 0-99/531579"
        206 => header("content-range").and_then(|v| v.rsplit('/').next().and_then(|n| n.trim().parse().ok())),
        200 => header("content-length").and_then(|v| v.trim().parse().ok()),
        _ => None,
    };
    let limit = match (status, total) {
        (206, _) => size,
        // A server that cannot send part of a file sends all of it. It is
        // held whole, which is fine for a track and not for anything the
        // size of a disc image.
        (200, Some(total)) if total > WHOLE_MAX => {
            return Err(chilpa_i18n::t("the server cannot send part of a file, and this one is too big to take whole"));
        }
        (200, Some(total)) => total,
        // No length: nothing worth reading. The caller refuses it.
        (200, None) => return Ok(Reply { status, total, content_type, body: Vec::new(), chunked }),
        // An error page, read only to know it is one.
        _ => 64 * 1024,
    };
    let mut body = reply.into_body();
    // One past: ureq's limit fails the read that finds the end right after
    // the last allowed byte.
    let body = body.with_config().limit(limit + 1).read_to_vec().map_err(|e| server::describe(&e))?;
    trace.response(status, start, size, total, body.len(), body.capacity());
    Ok(Reply { status, total, content_type, body, chunked })
}

/// What a server sends when it sends a message instead of the file.
fn is_audio_type(kind: &str) -> bool {
    let kind = kind.to_ascii_lowercase();
    !(kind.starts_with("text/") || kind.contains("json") || kind.contains("xml") || kind.starts_with("image/"))
}

/// A Subsonic error with an audio type, or none: the first bytes say it.
fn looks_like_a_message(first: &[u8]) -> bool {
    let start: Vec<u8> = first.iter().take(64).copied().skip_while(|b| b.is_ascii_whitespace()).collect();
    start.starts_with(b"{\"subsonic-response\"") || start.starts_with(b"<?xml") || start.starts_with(b"<subsonic-response")
}

#[cfg(test)]
mod tests;
