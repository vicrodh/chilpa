//! Last.fm: an account allowed on its own site, and scrobbles.
//!
//! Every request to Last.fm is signed with the application's secret, which
//! cannot ship inside a program anyone can read. The requests go through a
//! proxy that adds the key and the signature and hands the reply back as it
//! came. During development it is QBZ's; Chilpa's own comes later.
//!
//! Connecting is three steps: a request token and the page where the person
//! allows Chilpa; the person allows it; the token is traded for a session
//! key, which is the account from then on. Until they allow it, Last.fm
//! answers that the token is not authorized, which here means "later".

use super::queue::Service;
use super::{Failure, Kind, Listen};
use serde_json::{json, Map, Value};

const PROXY: &str = "https://qbz-api-proxy.blitzkriegfc.workers.dev/lastfm";

/// Scrobbles in one request: Last.fm's own limit.
const BATCH: usize = 50;

/// How long to wait before trying again when the other side does not say.
const LATER_SECONDS: u64 = 60;

/// Last.fm as the scrobbler drives it.
pub(super) const SERVICE: Service = Service { label: "Last.fm", batch: BATCH, begin: Some(begin), check, submit };

/// A request token and the page where the person allows Chilpa.
fn begin() -> Result<(String, String), Failure> {
    let reply = call("auth.getToken", Map::new())?;
    let token = reply.get("token").and_then(Value::as_str).unwrap_or_default();
    let link = reply.get("authUrl").and_then(Value::as_str).unwrap_or_default();
    if token.is_empty() || !link.starts_with("https://") {
        return Err(Failure::Later(LATER_SECONDS));
    }
    Ok((token.to_string(), link.to_string()))
}

/// Trades an allowed request token for the account: its user and session key.
fn check(token: &str) -> Result<(String, String), Failure> {
    let mut params = Map::new();
    params.insert("token".into(), json!(token));
    let reply = call("auth.getSession", params)?;
    let session = reply.get("session");
    let user = session.and_then(|s| s.get("name")).and_then(Value::as_str).unwrap_or_default();
    let key = session.and_then(|s| s.get("key")).and_then(Value::as_str).unwrap_or_default();
    if user.is_empty() || key.is_empty() {
        return Err(Failure::Token);
    }
    Ok((user.to_string(), key.to_string()))
}

fn submit(session: &str, kind: Kind, listens: &[Listen]) -> Result<(), Failure> {
    let (method, params) = request(session, kind, listens);
    call(method, params).map(|_| ())
}

/// The method and its parameters for a submission. A scrobble of several
/// numbers each field: `artist[0]`, `artist[1]`...
fn request(session: &str, kind: Kind, listens: &[Listen]) -> (&'static str, Map<String, Value>) {
    let mut params = Map::new();
    params.insert("sk".into(), json!(session));
    let playing = kind == Kind::PlayingNow;
    for (i, l) in listens.iter().enumerate() {
        let key = |name: &str| if playing { name.to_string() } else { format!("{name}[{i}]") };
        params.insert(key("artist"), json!(l.artist));
        params.insert(key("track"), json!(l.track));
        if !l.release.is_empty() {
            params.insert(key("album"), json!(l.release));
        }
        if l.duration_ms > 0 {
            params.insert(key("duration"), json!((l.duration_ms / 1000).to_string()));
        }
        if let Some(n) = l.track_number {
            params.insert(key("trackNumber"), json!(n.to_string()));
        }
        if !l.recording_mbid.is_empty() {
            params.insert(key("mbid"), json!(l.recording_mbid));
        }
        if !playing {
            params.insert(key("timestamp"), json!(l.listened_at.to_string()));
        }
    }
    (if playing { "track.updateNowPlaying" } else { "track.scrobble" }, params)
}

/// What an error Last.fm answers with means.
fn failure(code: i64, message: &str) -> Failure {
    match code {
        // Not authorized yet: the person has not allowed it on the page.
        14 => Failure::Later(3),
        // An expired or unknown token, a session that is no longer valid.
        4 | 9 | 15 => Failure::Token,
        // The service is down or busy, or the rate limit.
        11 | 16 => Failure::Later(LATER_SECONDS),
        29 => Failure::Later(5 * 60),
        _ => Failure::Refused(message.to_string()),
    }
}

/// One method through the proxy, with its parameters as JSON.
fn call(method: &str, params: Map<String, Value>) -> Result<Value, Failure> {
    let request = json!({
        "url": format!("{PROXY}/{method}"),
        "method": "POST",
        "headers": { "Content-Type": "application/json", "Accept": "application/json" },
        "body": Value::Object(params).to_string(),
    });
    let raw = chilpa_core::send_request(request.to_string().as_bytes(), None);
    let reply: Value = serde_json::from_slice(&raw).map_err(|_| Failure::Later(LATER_SECONDS))?;
    // No reply at all: the network, not Last.fm.
    if reply.get("error").is_some() {
        return Err(Failure::Later(LATER_SECONDS));
    }
    let status = reply.get("status").and_then(Value::as_u64).unwrap_or(0);
    let body: Value = reply.get("body").and_then(Value::as_str).and_then(|b| serde_json::from_str(b).ok()).unwrap_or_default();
    if let Some(code) = body.get("error").and_then(Value::as_i64) {
        let message = body.get("message").and_then(Value::as_str).unwrap_or_default();
        return Err(failure(code, message));
    }
    // The proxy's own refusal carries its reason as text.
    if let Some(why) = body.get("error").and_then(Value::as_str) {
        return Err(if status == 429 { Failure::Later(LATER_SECONDS) } else { Failure::Refused(why.to_string()) });
    }
    if status >= 500 || status == 0 {
        return Err(Failure::Later(LATER_SECONDS));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen(track: &str, at: i64) -> Listen {
        Listen {
            artist: "Aimer".into(),
            track: track.into(),
            release: "Sun Dance".into(),
            duration_ms: 263_400,
            track_number: Some(3),
            listened_at: at,
            ..Listen::default()
        }
    }

    #[test]
    fn a_scrobble_of_several_numbers_every_field() {
        let (method, p) = request("key", Kind::Import, &[listen("A", 10), listen("B", 20)]);
        assert_eq!(method, "track.scrobble");
        assert_eq!(p["sk"], "key");
        assert_eq!(p["track[1]"], "B");
        assert_eq!(p["timestamp[0]"], "10");
        assert_eq!(p["duration[0]"], "263", "in seconds");
        assert!(p.get("mbid[0]").is_none(), "an empty one is left out");
    }

    #[test]
    fn what_is_playing_is_named_plainly_and_has_no_moment() {
        let (method, p) = request("key", Kind::PlayingNow, &[listen("A", 10)]);
        assert_eq!(method, "track.updateNowPlaying");
        assert_eq!(p["track"], "A");
        assert!(p.get("timestamp").is_none());
    }

    #[test]
    #[ignore = "goes out to the proxy and Last.fm; the development proxy only lets QBZ in for now"]
    fn the_proxy_hands_out_a_request_token_and_the_page_to_allow_it() {
        let (token, link) = begin().expect("the proxy answered");
        assert!(!token.is_empty());
        assert!(link.starts_with("https://www.last.fm/api/auth/"), "{link}");
        // Asking for the account before it is allowed is "not yet".
        assert_eq!(check(&token), Err(Failure::Later(3)));
    }

    #[test]
    fn not_allowed_yet_is_later_and_a_dead_session_is_the_account() {
        assert_eq!(failure(14, ""), Failure::Later(3));
        assert_eq!(failure(9, ""), Failure::Token);
        assert_eq!(failure(29, ""), Failure::Later(300));
        assert_eq!(failure(6, "Invalid parameters"), Failure::Refused("Invalid parameters".into()));
    }
}
