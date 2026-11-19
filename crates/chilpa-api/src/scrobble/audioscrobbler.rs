//! The Audioscrobbler 2.0 API spoken straight to a GNU FM server: Libre.fm,
//! or one somebody runs themselves.
//!
//! The requests are the ones Last.fm takes, and so is connecting: a request
//! token, the server's page where the person allows it, and the session
//! key the token is traded for. What differs is that nothing here needs the
//! proxy: a GNU FM server issues no application secrets -- it takes any key
//! of 32 characters and never checks a signature -- so the key and the
//! secret Libre.fm is called with are plain constants. A server that does
//! issue its own gets the ones the person typed. Every call is signed the
//! way the API says, for the servers that do check.

use super::queue::Service;
use super::{lastfm, server, Failure, Kind, Listen};
use md5::{Digest, Md5};
use serde_json::{Map, Value};

/// Scrobbles in one request: the API's limit.
const BATCH: usize = 50;

/// How long to wait before trying again when the other side does not say.
const LATER_SECONDS: u64 = 60;

/// How Chilpa names itself to Libre.fm. Not a secret: the server keeps no
/// list of keys, it only counts their length.
const LIBREFM: Server<'static> = Server {
    origin: "https://libre.fm",
    key: "6368696c7061636869c3a17020617070",
    secret: "6368696c70612e617070206f6e206c66",
};

pub(super) const LIBREFM_SERVICE: Service =
    Service { label: "Libre.fm", batch: BATCH, begin: Some(begin_librefm), check, submit };

pub(super) const GNUFM_SERVICE: Service =
    Service { label: "GNU FM", batch: BATCH, begin: Some(begin_gnufm), check, submit };

/// A server, and how Chilpa is known to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Server<'a> {
    origin: &'a str,
    key: &'a str,
    secret: &'a str,
}

impl Server<'_> {
    /// The GNU FM family wants the slash at the end.
    fn api(&self) -> String {
        format!("{}/2.0/", self.origin)
    }

    /// The page where the person allows the request token.
    fn allow_page(&self, token: &str) -> String {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("api_key", self.key)
            .append_pair("token", token)
            .finish();
        format!("{}/api/auth/?{query}", self.origin)
    }

    /// The server with one more part -- a request token, a session key -- as
    /// the one text the queue carries.
    fn with(&self, last: &str) -> String {
        server::pack(&[self.origin, self.key, self.secret, last])
    }
}

/// A server and the last part it was kept with.
fn read(account: &str) -> Option<(Server<'_>, &str)> {
    match server::unpack(account).as_slice() {
        [origin, key, secret, last] if !origin.is_empty() && !last.is_empty() => Some((Server { origin, key, secret }, last)),
        _ => None,
    }
}

fn begin_librefm(_: &str) -> Result<(String, String), Failure> {
    begin(&LIBREFM)
}

/// The address, the API key and the shared secret as typed, a line each.
fn begin_gnufm(typed: &str) -> Result<(String, String), Failure> {
    let parts = server::unpack(typed);
    let part = |i: usize| parts.get(i).map(|p| p.trim()).unwrap_or_default();
    let origin = origin(part(0)).ok_or_else(|| Failure::Refused(chilpa_i18n::t("Write the server's address first, like https://gnufm.example.com.")))?;
    if part(1).is_empty() || part(2).is_empty() {
        return Err(Failure::Refused(chilpa_i18n::t("Write the server's API key and its shared secret first.")));
    }
    begin(&Server { origin: &origin, key: part(1), secret: part(2) })
}

/// Where a GNU FM server is, as typed: with or without the API's own path.
fn origin(typed: &str) -> Option<String> {
    let origin = server::origin(typed)?;
    Some(origin.strip_suffix("/2.0").unwrap_or(&origin).to_string())
}

/// A request token and the page where the person allows it.
fn begin(server: &Server) -> Result<(String, String), Failure> {
    let reply = call(server, "auth.getToken", Map::new(), Asking::Account)?;
    let token = reply.get("token").and_then(Value::as_str).unwrap_or_default();
    if token.is_empty() {
        return Err(Failure::Later(LATER_SECONDS));
    }
    Ok((server.with(token), server.allow_page(token)))
}

/// Trades an allowed request token for the account: its user, and the
/// server with its session key.
fn check(connecting: &str) -> Result<(String, String), Failure> {
    let (server, token) = read(connecting).ok_or(Failure::Token)?;
    let mut params = Map::new();
    params.insert("token".into(), Value::String(token.to_string()));
    let reply = call(&server, "auth.getSession", params, Asking::Account)?;
    let session = reply.get("session");
    let user = session.and_then(|s| s.get("name")).and_then(Value::as_str).unwrap_or_default();
    let key = session.and_then(|s| s.get("key")).and_then(Value::as_str).unwrap_or_default();
    if user.is_empty() || key.is_empty() {
        return Err(Failure::Token);
    }
    Ok((user.to_string(), server.with(key)))
}

fn submit(account: &str, kind: Kind, listens: &[Listen]) -> Result<(), Failure> {
    let (server, session) = read(account).ok_or(Failure::Token)?;
    let (method, params) = lastfm::request(session, kind, listens);
    call(&server, method, params, Asking::Listens).map(|_| ())
}

/// What a call is for: the same error code reads differently while an
/// account is being allowed and once it sends.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asking {
    Account,
    Listens,
}

/// What an error the server answers with means.
fn failure(code: i64, message: &str, asking: Asking) -> Failure {
    match (code, asking) {
        // Not allowed on the page yet. GNU FM also answers an unknown token
        // this way, so it is asked until the time to allow it runs out.
        (14, Asking::Account) | (4, Asking::Account) => Failure::Later(3),
        (15, Asking::Account) => Failure::Token,
        // The session is not the server's any more.
        (9, _) | (14, Asking::Listens) => Failure::Token,
        // Its own trouble, or its limit: the listens wait.
        (4, Asking::Listens) | (8, _) | (11, _) | (16, _) => Failure::Later(LATER_SECONDS),
        (29, _) => Failure::Later(5 * 60),
        _ => Failure::Refused(message.to_string()),
    }
}

/// The parameters as they go out: with the method, the key and the
/// signature the API asks for, and the format last, which is not signed.
fn signed(server: &Server, method: &str, params: Map<String, Value>) -> Vec<(String, String)> {
    let mut all: Vec<(String, String)> = params
        .into_iter()
        .map(|(name, value)| (name, value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string())))
        .collect();
    all.push(("method".into(), method.to_string()));
    all.push(("api_key".into(), server.key.to_string()));
    // Sorted by name as bytes, each name followed by its value, the secret
    // at the end.
    all.sort();
    let mut text: String = all.iter().flat_map(|(name, value)| [name.as_str(), value.as_str()]).collect();
    text.push_str(server.secret);
    let digest = Md5::digest(text.as_bytes());
    all.push(("api_sig".into(), digest.iter().map(|byte| format!("{byte:02x}")).collect()));
    all.push(("format".into(), "json".into()));
    all
}

/// One method, as a form posted to the server.
fn call(server: &Server, method: &str, params: Map<String, Value>, asking: Asking) -> Result<Value, Failure> {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in signed(server, method, params) {
        form.append_pair(&name, &value);
    }
    let request = serde_json::json!({
        "url": server.api(),
        "method": "POST",
        "headers": { "Content-Type": "application/x-www-form-urlencoded", "Accept": "application/json" },
        "body": form.finish(),
    });
    let raw = chilpa_core::send_request(request.to_string().as_bytes(), None);
    let reply: Value = serde_json::from_slice(&raw).map_err(|_| Failure::Later(LATER_SECONDS))?;
    // No reply at all: the network, not the server.
    if reply.get("error").is_some() {
        return Err(Failure::Later(LATER_SECONDS));
    }
    let status = reply.get("status").and_then(Value::as_u64).unwrap_or(0);
    // A page instead of JSON is a server that is not there, or not this.
    let Some(body) = reply.get("body").and_then(Value::as_str).and_then(|b| serde_json::from_str::<Value>(b).ok()) else {
        return Err(Failure::Later(LATER_SECONDS));
    };
    if let Some(code) = body.get("error").and_then(Value::as_i64) {
        let message = body.get("message").and_then(Value::as_str).unwrap_or_default();
        return Err(failure(code, message, asking));
    }
    if status >= 400 || status == 0 {
        return Err(Failure::Later(LATER_SECONDS));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST: Server<'static> = Server { origin: "https://fm.example.org", key: "key", secret: "secret" };

    #[test]
    fn a_call_is_signed_over_its_sorted_parameters_and_the_secret() {
        let mut params = Map::new();
        params.insert("token".into(), json!("abc"));
        let sent = signed(&TEST, "auth.getSession", params);
        let value = |name: &str| sent.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
        // md5("api_keykeymethodauth.getSessiontokenabcsecret")
        let expected: String = Md5::digest(b"api_keykeymethodauth.getSessiontokenabcsecret").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(value("api_sig"), Some(expected.as_str()));
        assert_eq!(value("format"), Some("json"), "sent, and left out of the signature");
        assert_eq!(sent.last().map(|(n, _)| n.as_str()), Some("format"));
    }

    #[test]
    fn a_batch_is_signed_with_its_numbered_fields_in_byte_order() {
        let listens: Vec<Listen> = (0..11).map(|i| Listen { artist: "A".into(), track: format!("T{i}"), listened_at: i, ..Listen::default() }).collect();
        let (method, params) = lastfm::request("sk", Kind::Import, &listens);
        let sent = signed(&TEST, method, params);
        let names: Vec<&str> = sent.iter().map(|(n, _)| n.as_str()).filter(|n| n.starts_with("track[")).collect();
        assert_eq!(&names[..3], ["track[0]", "track[10]", "track[1]"], "as the server sorts them to check");
    }

    #[test]
    fn a_server_keeps_its_last_part_and_gives_it_back() {
        let kept = TEST.with("session");
        assert_eq!(read(&kept), Some((TEST, "session")));
        assert_eq!(read("https://fm.example.org\nkey\nsecret"), None);
        assert_eq!(read(""), None);
        assert_eq!(TEST.api(), "https://fm.example.org/2.0/");
        assert_eq!(TEST.allow_page("t k"), "https://fm.example.org/api/auth/?api_key=key&token=t+k");
    }

    #[test]
    fn a_server_of_ones_own_needs_its_address_its_key_and_its_secret() {
        // Refused before anything goes out, each with a reason to read.
        for typed in ["\nkey\nsecret", "fm.example.org\n\nsecret", "fm.example.org\nkey\n", ""] {
            assert!(matches!(begin_gnufm(typed), Err(Failure::Refused(why)) if !why.is_empty()), "{typed:?}");
        }
        assert_eq!(origin("fm.example.org/2.0/").as_deref(), Some("https://fm.example.org"));
    }

    #[test]
    fn not_allowed_yet_is_asked_again_and_a_dead_session_is_the_account() {
        assert_eq!(failure(14, "", Asking::Account), Failure::Later(3));
        assert_eq!(failure(4, "", Asking::Account), Failure::Later(3));
        assert_eq!(failure(9, "", Asking::Listens), Failure::Token);
        assert_eq!(failure(14, "", Asking::Listens), Failure::Token);
        assert_eq!(failure(4, "", Asking::Listens), Failure::Later(LATER_SECONDS), "kept, not dropped");
        assert_eq!(failure(11, "", Asking::Listens), Failure::Later(LATER_SECONDS));
        assert_eq!(failure(6, "Invalid parameters", Asking::Listens), Failure::Refused("Invalid parameters".into()));
    }

    #[test]
    fn the_client_key_is_as_long_as_the_server_asks() {
        assert_eq!(LIBREFM.key.len(), 32);
        assert_eq!(LIBREFM.secret.len(), 32);
    }

    #[test]
    fn an_account_is_allowed_and_a_listen_sent_as_the_api_says() {
        let (origin, served) = server::answering(vec![
            r#"{"token":"request-1"}"#,
            r#"{"error":14,"message":"This token has not yet been authorised"}"#,
            r#"{"session":{"name":"someone","key":"session-1","subscriber":0}}"#,
            r#"{"scrobbles":{"@attr":{"accepted":1,"ignored":0}}}"#,
            r#"{"error":9,"message":"Invalid session key - Please re-authenticate"}"#,
        ]);
        let typed = server::pack(&[&format!("{origin}/"), "key", "secret"]);
        let (connecting, link) = begin_gnufm(&typed).expect("a request token");
        assert_eq!(link, format!("{origin}/api/auth/?api_key=key&token=request-1"));
        assert_eq!(check(&connecting), Err(Failure::Later(3)), "not allowed yet");
        let (user, account) = check(&connecting).expect("allowed");
        assert_eq!(user, "someone");
        assert_eq!(read(&account), Some((Server { origin: &origin, key: "key", secret: "secret" }, "session-1")));
        let listen = Listen { artist: "Björk & Co".into(), track: "T".into(), duration_ms: 61_000, listened_at: 1_700_000_000, ..Listen::default() };
        assert_eq!(submit(&account, Kind::Single, std::slice::from_ref(&listen)), Ok(()));
        assert_eq!(submit(&account, Kind::Single, &[listen]), Err(Failure::Token), "a session the server dropped");
        let sent = served.join().expect("the server ran");
        assert!(sent.iter().all(|request| request.starts_with("POST /2.0/ HTTP/1.1")), "every call is a form posted to the API");
        let body = |i: usize| sent[i].split("\r\n\r\n").nth(1).unwrap_or_default().to_string();
        assert!(body(0).contains("method=auth.getToken") && body(0).contains("api_key=key") && body(0).ends_with("format=json"), "{}", body(0));
        assert!(body(2).contains("method=auth.getSession") && body(2).contains("token=request-1"));
        let scrobble = body(3);
        for part in ["method=track.scrobble", "sk=session-1", "artist%5B0%5D=Bj%C3%B6rk+%26+Co", "track%5B0%5D=T", "duration%5B0%5D=61", "timestamp%5B0%5D=1700000000", "api_sig="] {
            assert!(scrobble.contains(part), "{part} in {scrobble}");
        }
    }

    #[test]
    #[ignore = "goes out to Libre.fm; run it on purpose"]
    fn the_real_server_hands_out_a_token_and_waits_for_it_to_be_allowed() {
        let (connecting, link) = begin_librefm("").expect("Libre.fm answered");
        assert!(link.starts_with("https://libre.fm/api/auth/?api_key="), "{link}");
        assert_eq!(check(&connecting), Err(Failure::Later(3)));
    }
}
