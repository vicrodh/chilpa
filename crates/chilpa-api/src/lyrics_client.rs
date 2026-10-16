//! The lyrics service, over HTTP.
//!
//! This is all Chilpa knows about where lyrics come from when the track has
//! none of its own: one address and one question, described in
//! `chilpa_lyrics::service`. Which providers are behind it, how they are
//! reached and what runs there is the service's business; it can change
//! without a new Chilpa.
//!
//! The pipeline sees it as its [`Remote`]: whoever answers for a track with
//! no lyrics of its own; and as its [`Translator`]: whoever translates the
//! lyrics shown when they carry no translation in the reader's language.

use chilpa_lyrics::pipeline::{Answer, Remote, Translation, Translator};
use chilpa_lyrics::{service, Timing, Track};
use serde_json::{json, Value};
use std::time::Duration;

/// Another address for the service, for trying a build against one that
/// runs somewhere else. The contract is the same wherever it runs.
const ADDRESS_VARIABLE: &str = "CHILPA_LYRICS_SERVICE";

pub(crate) struct LyricsServiceClient {
    origin: String,
}

impl LyricsServiceClient {
    pub(crate) fn new() -> Self {
        let stated = std::env::var(ADDRESS_VARIABLE).ok().map(|v| v.trim().trim_end_matches('/').to_string()).filter(|v| !v.is_empty());
        Self::at(stated.as_deref().unwrap_or(service::ORIGIN))
    }

    pub(crate) fn at(origin: &str) -> Self {
        LyricsServiceClient { origin: origin.to_string() }
    }
}

impl LyricsServiceClient {
    /// Posts a question at a path and splits the reply: the status, the
    /// `Retry-After` and the body. `Err` is no reply at all: the network,
    /// not the service.
    fn post(&self, path: &str, body: &Value) -> Result<(u16, Option<String>, String), String> {
        if !chilpa_core::network::current().lyrics_online() { return Err(chilpa_core::network::blocked()); }
        let request = json!({
            "url": format!("{}{path}", self.origin),
            "method": "POST",
            "headers": { "Content-Type": "application/json", "Accept": "application/json" },
            "body": body.to_string(),
        });
        let raw = chilpa_core::send_request(request.to_string().as_bytes(), None);
        let reply = serde_json::from_slice::<Value>(&raw).map_err(|_| "the lyrics service could not be reached".to_string())?;
        if let Some(error) = reply.get("error").and_then(Value::as_str) {
            return Err(error.to_string());
        }
        let status = reply.get("status").and_then(Value::as_u64).unwrap_or(0) as u16;
        let retry_after = reply.get("headers").and_then(|h| h.get("retry-after")).and_then(Value::as_str).map(str::to_string);
        Ok((status, retry_after, reply.get("body").and_then(Value::as_str).unwrap_or_default().to_string()))
    }
}

impl Remote for LyricsServiceClient {
    fn id(&self) -> &str {
        "lyrics-service"
    }

    /// Shown only for a document that does not say who gave it.
    fn name(&self) -> &str {
        "Chilpa"
    }

    fn resolve(&self, track: &Track, language: Option<&str>, providers: Option<&[String]>, _timeout: Duration) -> Answer {
        let allowed = crate::lyrics_service::REMOTE_PROVIDERS;
        let selected: Vec<String> = providers.map_or_else(|| allowed.iter().map(|id| (*id).to_string()).collect(),
            |ids| ids.iter().filter(|id| allowed.contains(&id.as_str())).cloned().collect());
        match self.post(service::RESOLVE_PATH, &service::request_with_providers(track, Timing::Unsynced, language, Some(&selected))) {
            Ok((status, retry_after, body)) => service::read(status, retry_after.as_deref(), &body),
            Err(message) => Answer::Failed { retry_after: None, message },
        }
    }
}

impl Translator for LyricsServiceClient {
    fn id(&self) -> &str {
        "lyrics-service"
    }

    fn translate(&self, track: &Track, lines: &[String], language: &str, from: Option<&str>, _timeout: Duration) -> Translation {
        match self.post(service::TRANSLATE_PATH, &service::translate_request(track, lines, language, from)) {
            Ok((status, retry_after, body)) => service::read_translation(status, retry_after.as_deref(), &body),
            Err(message) => Translation::Failed { retry_after: None, message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const WORDS: &str = include_str!("../../chilpa-lyrics/tests/service/found-word-richsync.json");
    const NONE: &str = include_str!("../../chilpa-lyrics/tests/service/none.json");
    const FAILED: &str = include_str!("../../chilpa-lyrics/tests/service/failed.json");

    fn track() -> Track {
        Track {
            title: "Small Hours".into(),
            artist: "Example Artist".into(),
            album: Some("Example Album".into()),
            duration_ms: Some(7_200),
            isrc: Some("QZES72500001".into()),
            mbid: None,
        }
    }

    /// A service that answers one request with a recorded answer, and hands
    /// back what it was asked: the request line, the User-Agent and the body.
    fn serve(golden: &str) -> (String, mpsc::Receiver<(String, String, Value)>) {
        let file: Value = serde_json::from_str(golden).unwrap();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", server.server_addr().to_ip().unwrap());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok(mut request) = server.recv() else { return };
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let agent = request.headers().iter().find(|h| h.field.equiv("User-Agent")).map(|h| h.value.to_string()).unwrap_or_default();
            let _ = tx.send((format!("{} {}", request.method(), request.url()), agent, serde_json::from_str(&body).unwrap_or_default()));
            let mut response = tiny_http::Response::from_string(file["response"].to_string()).with_status_code(file["status"].as_u64().unwrap() as u16);
            if let Some(wait) = file.get("retry_after").and_then(Value::as_str) {
                response.add_header(tiny_http::Header::from_bytes("Retry-After", wait).unwrap());
            }
            let _ = request.respond(response);
        });
        (origin, rx)
    }

    #[test]
    fn it_asks_the_contracts_question_and_reads_the_lyrics() {
        let (origin, asked) = serve(WORDS);
        let answer = LyricsServiceClient::at(&origin).resolve(&track(), None, None, Duration::from_secs(5));
        let (line, agent, body) = asked.recv().unwrap();
        assert_eq!(line, "POST /v1/lyrics/resolve");
        assert!(agent.starts_with("Chilpa"), "the service only serves Chilpa: {agent}");
        let golden: Value = serde_json::from_str(WORDS).unwrap();
        let mut expected = golden["request"].clone();
        expected["preferences"]["providers"] = json!(["lrclib", "netease", "qq", "kugou"]);
        assert_eq!(body, expected, "the contract question carries an explicit provider selection");
        let Answer::Found(found) = answer else { panic!("{answer:?}") };
        assert_eq!(found[0].document.timing, Timing::Word);
        assert_eq!(json!(found[0].document.source.as_ref().unwrap().name), golden["response"]["document"]["source"]["name"]);
    }

    #[test]
    fn none_is_none() {
        let (origin, _asked) = serve(NONE);
        assert_eq!(LyricsServiceClient::at(&origin).resolve(&track(), None, None, Duration::from_secs(5)), Answer::None);
    }

    #[test]
    fn a_service_that_fails_is_a_failure_and_never_none() {
        let (origin, _asked) = serve(FAILED);
        let answer = LyricsServiceClient::at(&origin).resolve(&track(), None, None, Duration::from_secs(5));
        assert!(matches!(answer, Answer::Failed { .. }), "{answer:?}");
    }

    const TRANSLATE_FOUND: &str = include_str!("../../chilpa-lyrics/tests/service/translate-found.json");
    const TRANSLATE_LATER: &str = include_str!("../../chilpa-lyrics/tests/service/translate-later.json");

    #[test]
    fn it_asks_for_a_translation_as_the_contract_says_and_reads_the_layer() {
        let (origin, asked) = serve(TRANSLATE_FOUND);
        let golden: Value = serde_json::from_str(TRANSLATE_FOUND).unwrap();
        let lines: Vec<String> = golden["request"]["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect();
        let small = Track { title: "Small Hours".into(), artist: "Example Artist".into(), album: None, duration_ms: None, isrc: None, mbid: None };
        let answer = LyricsServiceClient::at(&origin).translate(&small, &lines, "es", Some("en"), Duration::from_secs(5));
        let (line, agent, body) = asked.recv().unwrap();
        assert_eq!(line, "POST /v1/lyrics/translate");
        assert!(agent.starts_with("Chilpa"), "the service only serves Chilpa: {agent}");
        assert_eq!(body, golden["request"], "the question is the contract's");
        let Translation::Found(layer) = answer else { panic!("{answer:?}") };
        assert_eq!(layer.lines.len(), 3);
    }

    #[test]
    fn a_translation_still_being_made_is_a_failure_with_its_wait() {
        let (origin, _asked) = serve(TRANSLATE_LATER);
        let answer = LyricsServiceClient::at(&origin).translate(&track(), &["Line".to_string()], "es", None, Duration::from_secs(5));
        assert!(matches!(answer, Translation::Failed { retry_after: Some(wait), .. } if wait == Duration::from_secs(5)), "{answer:?}");
    }

    #[test]
    fn a_service_that_is_not_there_is_a_failure() {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        let answer = LyricsServiceClient::at(&format!("http://{closed}")).resolve(&track(), None, None, Duration::from_secs(5));
        assert!(matches!(answer, Answer::Failed { retry_after: None, .. }), "{answer:?}");
    }
}
