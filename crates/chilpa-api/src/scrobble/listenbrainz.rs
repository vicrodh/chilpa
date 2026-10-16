//! ListenBrainz's API: checking a user token and sending listens.
//!
//! Straight to the API, with no proxy in between: a user token pasted from
//! the account's settings page is all it asks for. The requests go out
//! through the engine's one HTTP client, which puts the token in the
//! `Authorization` header on this side.
//!
//! Nothing here keeps state or retries. What to do with a failure -- keep
//! the listen for later, stop until the token is replaced, drop it -- is the
//! queue's decision, and `Failure` says which one applies.

use super::queue::Service;
use super::{Failure, Kind, Listen};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

const API: &str = "https://api.listenbrainz.org/1";

/// Listens sent in one import. ListenBrainz takes up to a thousand; a
/// hundred keeps a request small enough that a slow link still finishes it.
const BATCH: usize = 100;

/// How long to wait before trying again when the other side does not say.
const LATER_SECONDS: u64 = 60;

/// ListenBrainz as the scrobbler drives it: a pasted token is the account.
pub(super) const SERVICE: Service =
    Service { label: "ListenBrainz", batch: BATCH, begin: None, check, submit };

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::PlayingNow => "playing_now",
        Kind::Single => "single",
        Kind::Import => "import",
    }
}

/// A pasted token: its user, and the token itself to send with.
fn check(token: &str) -> Result<(String, String), Failure> {
    validate(token).map(|user| (user, token.to_string()))
}

/// Checks a token. Answers with the user name it belongs to.
fn validate(token: &str) -> Result<String, Failure> {
    let (status, _, body) = request("GET", &format!("{API}/validate-token"), token, "")?;
    let reply: Value = serde_json::from_str(&body).unwrap_or_default();
    let valid = reply.get("valid").and_then(Value::as_bool).unwrap_or(false);
    match reply.get("user_name").and_then(Value::as_str) {
        Some(user) if status == 200 && valid && !user.is_empty() => Ok(user.to_string()),
        _ if status == 200 || status == 401 => Err(Failure::Token),
        _ => Err(failure(status, &BTreeMap::new(), &body)),
    }
}

/// Sends listens: one for `PlayingNow` and `Single`, up to `BATCH` for an
/// `Import`.
fn submit(token: &str, kind: Kind, listens: &[Listen]) -> Result<(), Failure> {
    let body = payload(kind, listens).to_string();
    let (status, headers, reply) = request("POST", &format!("{API}/submit-listens"), token, &body)?;
    if status == 200 {
        Ok(())
    } else {
        Err(failure(status, &headers, &reply))
    }
}

/// The JSON ListenBrainz expects for a submission.
fn payload(kind: Kind, listens: &[Listen]) -> Value {
    let items: Vec<Value> = listens
        .iter()
        .map(|l| {
            let mut info = Map::new();
            info.insert("media_player".into(), json!("Chilpa"));
            info.insert("submission_client".into(), json!("Chilpa"));
            info.insert("submission_client_version".into(), json!(env!("CARGO_PKG_VERSION")));
            if l.duration_ms > 0 {
                info.insert("duration_ms".into(), json!(l.duration_ms));
            }
            if let Some(n) = l.track_number {
                info.insert("tracknumber".into(), json!(n));
            }
            if !l.recording_mbid.is_empty() {
                info.insert("recording_mbid".into(), json!(l.recording_mbid));
            }
            if !l.isrc.is_empty() {
                info.insert("isrc".into(), json!(l.isrc));
            }
            let mut meta = Map::new();
            meta.insert("artist_name".into(), json!(l.artist));
            meta.insert("track_name".into(), json!(l.track));
            if !l.release.is_empty() {
                meta.insert("release_name".into(), json!(l.release));
            }
            meta.insert("additional_info".into(), Value::Object(info));
            let mut item = Map::new();
            // What is playing now has no moment yet: it has not been listened
            // to, and ListenBrainz refuses one that carries it.
            if kind != Kind::PlayingNow {
                item.insert("listened_at".into(), json!(l.listened_at));
            }
            item.insert("track_metadata".into(), Value::Object(meta));
            Value::Object(item)
        })
        .collect();
    json!({ "listen_type": kind_name(kind), "payload": items })
}

/// What a status that is not 200 means.
fn failure(status: u16, headers: &BTreeMap<String, String>, body: &str) -> Failure {
    let header = |name: &str| {
        headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).and_then(|(_, v)| v.trim().parse::<u64>().ok())
    };
    match status {
        401 => Failure::Token,
        // The rate limit says how long until it resets.
        429 => Failure::Later(header("X-RateLimit-Reset-In").unwrap_or(LATER_SECONDS).max(1)),
        400 => {
            let reply: Value = serde_json::from_str(body).unwrap_or_default();
            Failure::Refused(reply.get("error").and_then(Value::as_str).unwrap_or(body).to_string())
        }
        _ => Failure::Later(LATER_SECONDS),
    }
}

/// One request through the engine's client: status, headers and body.
fn request(method: &str, url: &str, token: &str, body: &str) -> Result<(u16, BTreeMap<String, String>, String), Failure> {
    let request = json!({
        "url": url,
        "method": method,
        "headers": { "Content-Type": "application/json", "Accept": "application/json" },
        "body": body,
    });
    let raw = chilpa_core::send_request(request.to_string().as_bytes(), Some(&format!("Token {token}")));
    let reply: Value = serde_json::from_slice(&raw).map_err(|_| Failure::Later(LATER_SECONDS))?;
    // No reply at all: the network, not ListenBrainz.
    if reply.get("error").is_some() {
        return Err(Failure::Later(LATER_SECONDS));
    }
    let status = reply.get("status").and_then(Value::as_u64).unwrap_or(0) as u16;
    let headers = reply
        .get("headers")
        .and_then(Value::as_object)
        .map(|o| o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    let body = reply.get("body").and_then(Value::as_str).unwrap_or_default().to_string();
    Ok((status, headers, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen() -> Listen {
        Listen {
            artist: "Aimer".into(),
            track: "Ref:rain".into(),
            release: "Sun Dance".into(),
            duration_ms: 263_000,
            track_number: Some(3),
            recording_mbid: String::new(),
            isrc: "JPU901800123".into(),
            listened_at: 1_700_000_000,
        }
    }

    #[test]
    fn a_listen_carries_its_moment_and_what_is_known_of_it() {
        let v = payload(Kind::Single, &[listen()]);
        assert_eq!(v["listen_type"], "single");
        let item = &v["payload"][0];
        assert_eq!(item["listened_at"], 1_700_000_000);
        assert_eq!(item["track_metadata"]["release_name"], "Sun Dance");
        let info = &item["track_metadata"]["additional_info"];
        assert_eq!(info["duration_ms"], 263_000);
        assert_eq!(info["tracknumber"], 3);
        assert_eq!(info["isrc"], "JPU901800123");
        assert!(info.get("recording_mbid").is_none(), "an empty one is left out, not sent blank");
        assert_eq!(info["media_player"], "Chilpa");
    }

    #[test]
    fn what_is_playing_now_has_no_moment() {
        let v = payload(Kind::PlayingNow, &[listen()]);
        assert_eq!(v["listen_type"], "playing_now");
        assert!(v["payload"][0].get("listened_at").is_none());
    }

    #[test]
    #[ignore = "goes out to ListenBrainz; run it on purpose"]
    fn a_made_up_token_is_refused_by_the_real_api() {
        assert_eq!(validate("not-a-real-token"), Err(Failure::Token));
    }

    #[test]
    fn the_rate_limit_says_when_to_try_again() {
        let mut headers = BTreeMap::new();
        headers.insert("x-ratelimit-reset-in".to_string(), "7".to_string());
        assert_eq!(failure(429, &headers, ""), Failure::Later(7));
        assert_eq!(failure(429, &BTreeMap::new(), ""), Failure::Later(LATER_SECONDS));
        assert_eq!(failure(401, &BTreeMap::new(), ""), Failure::Token);
        assert_eq!(failure(503, &BTreeMap::new(), ""), Failure::Later(LATER_SECONDS));
        assert_eq!(
            failure(400, &BTreeMap::new(), r#"{"code":400,"error":"bad listen"}"#),
            Failure::Refused("bad listen".into())
        );
    }
}
