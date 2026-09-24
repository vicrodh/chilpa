//! The flow: where the user is sent, what the browser answers, and how that
//! becomes tokens.

use crate::pkce::{nonce, Pkce};
use crate::{secure, Auth, AuthError, Tokens};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

/// How long to wait for the user to finish in the browser.
const PATIENCE: Duration = Duration::from_secs(300);

/// The address the browser comes back to.
///
/// Loopback on an ephemeral port: RFC 8252 prefers it over a custom scheme
/// (`chilpa://`) because any other installed application can register a
/// scheme, and then the authorization code lands in someone else's hands.
pub struct Loopback {
    listener: TcpListener,
    state: String,
}

impl Loopback {
    pub fn bind() -> Result<Self, AuthError> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(false)?;
        Ok(Self { listener, state: nonce() })
    }

    /// Where we tell the service to send the browser back to.
    pub fn redirect_uri(&self) -> String {
        let port = self.listener.local_addr().map(|a| a.port()).unwrap_or(0);
        format!("http://127.0.0.1:{port}/callback")
    }

    pub fn state(&self) -> &str {
        &self.state
    }
}

/// The URL the user has to be sent to.
///
/// It does not open it: the caller decides how a browser opens, which is the
/// system's business and not this crate's.
pub fn authorize_url(auth: &Auth, pkce: &Pkce, back: &Loopback) -> Option<String> {
    let Auth::OAuth2 { authorize_url, client_id, scopes, .. } = auth else {
        return None;
    };
    let joiner = if authorize_url.contains('?') { '&' } else { '?' };
    let mut url = format!(
        "{authorize_url}{joiner}response_type=code&client_id={}&redirect_uri={}&state={}\
         &code_challenge={}&code_challenge_method={}",
        escape(client_id),
        escape(&back.redirect_uri()),
        escape(back.state()),
        escape(&pkce.challenge()),
        Pkce::method(),
    );
    if !scopes.is_empty() {
        url.push_str(&format!("&scope={}", escape(&scopes.join(" "))));
    }
    Some(url)
}

/// Waits for the browser to come back and answers the code.
///
/// Blocks. The caller runs it on its own thread, because the user can take
/// as long as they like to decide.
pub fn wait_for_code(back: &Loopback) -> Result<String, AuthError> {
    back.listener.set_nonblocking(false)?;
    let deadline = std::time::Instant::now() + PATIENCE;

    while std::time::Instant::now() < deadline {
        let (mut stream, _) = back.listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;

        let head = request_line(&mut stream);
        let Some(target) = head.split_whitespace().nth(1) else {
            let _ = answer(&mut stream, &chilpa_i18n::t("I did not understand that request."));
            continue;
        };
        // The browser asks for the favicon as well as the callback; that is
        // not the answer being waited for.
        let Some(query) = target.split_once('?').map(|(_, q)| q.to_string()) else {
            let _ = answer(&mut stream, &chilpa_i18n::t("The service's answer is missing."));
            continue;
        };

        let mut code = None;
        let mut state = None;
        let mut failed = None;
        for (k, v) in query.split('&').filter_map(|p| p.split_once('=')) {
            match k {
                "code" => code = Some(unescape(v)),
                "state" => state = Some(unescape(v)),
                "error" => failed = Some(unescape(v)),
                _ => {}
            }
        }

        if let Some(e) = failed {
            let _ = answer(&mut stream, &chilpa_i18n::t("The account did not connect. You can close this tab."));
            return Err(AuthError::NoCode(e));
        }
        // Compared before the code is used: a `state` that is not ours means
        // somebody else caused this reply.
        if state.as_deref() != Some(back.state()) {
            let _ = answer(&mut stream, &chilpa_i18n::t("That answer does not belong to this window."));
            return Err(AuthError::WrongState);
        }
        let Some(code) = code else {
            let _ = answer(&mut stream, &chilpa_i18n::t("The service sent no code."));
            continue;
        };
        let _ = answer(&mut stream, &chilpa_i18n::t("Done. You can close this tab and go back to Chilpa."));
        return Ok(code);
    }
    Err(AuthError::NoCode(chilpa_i18n::t("the wait ran out").into()))
}

/// How a request is sent to the service.
///
/// Injected instead of carrying an HTTP client inside. That way this crate
/// does not depend on the network, and `chilpa-plugin` can use its [`Auth`]
/// type to read a manifest without the sandbox ending up linking an HTTP
/// client that has no business being there.
///
/// Takes the URL and the body already built; answers the status code and the
/// reply body.
pub type Post<'a> = &'a dyn Fn(&str, &str) -> Result<(u16, String), String>;

/// Exchanges the code for the tokens.
pub fn exchange(
    auth: &Auth,
    code: &str,
    pkce: &Pkce,
    back: &Loopback,
    now_unix: i64,
    post_with: Post<'_>,
) -> Result<Tokens, AuthError> {
    let Auth::OAuth2 { token_url, client_id, .. } = auth else {
        return Err(AuthError::Unreadable(chilpa_i18n::t("that source does not use OAuth").into()));
    };
    let form = form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &back.redirect_uri()),
        ("client_id", client_id),
        ("code_verifier", pkce.verifier()),
    ]);
    post(token_url, &form, now_unix, "", post_with)
}

/// Asks for fresh access with the refresh token, without bothering the user.
pub fn refresh(
    auth: &Auth,
    tokens: &Tokens,
    now_unix: i64,
    post_with: Post<'_>,
) -> Result<Tokens, AuthError> {
    let Auth::OAuth2 { token_url, client_id, .. } = auth else {
        return Err(AuthError::Unreadable(chilpa_i18n::t("that source does not use OAuth").into()));
    };
    let form = form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &tokens.refresh_token),
        ("client_id", client_id),
    ]);
    // Many services return no new refresh on renewal; the one already held
    // still works, and losing it would force reconnecting the account.
    post(token_url, &form, now_unix, &tokens.refresh_token, post_with)
}

fn post(
    url: &str,
    body: &str,
    now_unix: i64,
    keep_refresh: &str,
    post_with: Post<'_>,
) -> Result<Tokens, AuthError> {
    if !secure(url) {
        return Err(AuthError::Unreadable("el token_url no es https".into()));
    }
    let (status, text) = post_with(url, body).map_err(AuthError::Network)?;
    let text = text.as_str();
    if !(200..300).contains(&status) {
        // Truncated: an error body can carry another account's token, or half
        // a page of HTML.
        let short: String = text.chars().take(200).collect();
        return Err(AuthError::Refused(status, short));
    }

    let body: serde_json::Value =
        serde_json::from_str(text).map_err(|e| AuthError::Unreadable(e.to_string()))?;
    let access = body.get("access_token").and_then(|t| t.as_str()).unwrap_or_default();
    if access.is_empty() {
        return Err(AuthError::NoToken);
    }
    let refresh = body
        .get("refresh_token")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .unwrap_or(keep_refresh);
    let expires_at = body.get("expires_in").and_then(|e| e.as_i64()).map(|s| now_unix + s);

    Ok(Tokens {
        access_token: access.to_string(),
        refresh_token: refresh.to_string(),
        expires_at,
    })
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{k}={}", escape(v))).collect::<Vec<_>>().join("&")
}

/// Escapes for a URL. Only the characters RFC 3986 calls "unreserved" pass
/// through; everything else goes percent-encoded.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The request's first line, reading until it is there.
///
/// One read does not bring a request: TCP delivers what it likes when it
/// likes. Measured on loopback, the first one brought `GET /` and nothing
/// else, and keeping only that threw the browser's reply away and the wait
/// never ended.
fn request_line(stream: &mut std::net::TcpStream) -> String {
    // A request whose first line does not fit here is not a browser coming
    // back from granting permission.
    const MAX: usize = 8 * 1024;
    let mut raw = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while raw.len() < MAX {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                if byte[0] != b'\r' {
                    raw.push(byte[0]);
                }
            }
        }
    }
    String::from_utf8_lossy(&raw).to_string()
}

fn answer(stream: &mut std::net::TcpStream, message: &str) -> std::io::Result<()> {
    let page = format!(
        "<!doctype html><html lang=\"es\"><meta charset=\"utf-8\">\
         <title>Chilpa</title><body style=\"font-family:sans-serif;padding:3rem\">\
         <p>{message}</p></body></html>"
    );
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    )?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> Auth {
        Auth::OAuth2 {
            authorize_url: "https://accounts.example.com/authorize".into(),
            token_url: "https://accounts.example.com/token".into(),
            client_id: "chilpa app".into(),
            scopes: vec!["read".into(), "write".into()],
        }
    }

    #[test]
    fn the_url_the_user_is_sent_to_carries_the_challenge_and_never_the_verifier() {
        let back = Loopback::bind().unwrap();
        let pkce = Pkce::new();
        let url = authorize_url(&auth(), &pkce, &back).unwrap();

        assert!(url.contains("response_type=code"));
        assert!(url.contains(&format!("code_challenge={}", escape(&pkce.challenge()))));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(!url.contains(pkce.verifier()), "the verifier does not leave this process");
        assert!(url.contains("scope=read%20write"), "{url}");
        assert!(url.contains("client_id=chilpa%20app"), "what carries spaces is escaped");
        assert!(url.contains("127.0.0.1"), "it comes back to a loopback: {url}");
    }

    #[test]
    fn the_redirect_is_a_loopback_port_that_the_system_gave_us() {
        let back = Loopback::bind().unwrap();
        let uri = back.redirect_uri();
        assert!(uri.starts_with("http://127.0.0.1:"), "{uri}");
        assert!(!uri.ends_with(":0"), "a real port: {uri}");
    }

    #[test]
    fn two_connections_do_not_share_a_state() {
        assert_ne!(Loopback::bind().unwrap().state(), Loopback::bind().unwrap().state());
    }

    #[test]
    fn what_goes_in_a_url_comes_back_the_same() {
        for s in ["hi", "with space", "a+b/c=d", "jalapeño", "%41"] {
            assert_eq!(unescape(&escape(s)), s, "{s}");
        }
    }

    #[test]
    fn a_static_token_source_has_no_url_to_send_anyone_to() {
        let back = Loopback::bind().unwrap();
        assert!(authorize_url(&Auth::Token, &Pkce::new(), &back).is_none());
    }
}
