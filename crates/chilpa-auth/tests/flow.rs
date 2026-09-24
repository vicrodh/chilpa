//! The whole flow against a fake service, with no browser.
//!
//! The test plays the browser: it fetches the redirect URL just as one
//! would. What matters is that the code arrives tied to its `state`, that
//! the verifier travels only when exchanging it for tokens, and that
//! el refresh.

use chilpa_auth::{authorize_url, exchange, refresh, Auth, Loopback, Pkce, Tokens};
use std::io::{Read, Write};
use std::net::TcpListener;

/// A service that hands out tokens and records what it was asked for.
struct Service {
    port: u16,
    seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

fn service() -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mine = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            // Read until the whole body is there: the client sends headers and
            // body separately, and keeping only the first read would make the
            // test pass without looking at anything.
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            let body = loop {
                let read = stream.read(&mut buf).unwrap_or(0);
                if read == 0 {
                    break String::new();
                }
                raw.extend_from_slice(&buf[..read]);
                let text = String::from_utf8_lossy(&raw).to_string();
                let Some((head, body)) = text.split_once("\r\n\r\n") else { continue };
                let want: usize = head
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse().ok()))
                    .unwrap_or(0);
                if body.len() >= want {
                    break body.to_string();
                }
            };
            mine.lock().unwrap().push(body.clone());

            // On renewal it sends no new refresh, like many services.
            let json = if body.contains("grant_type=refresh_token") {
                r#"{"access_token":"segundo","token_type":"Bearer","expires_in":3600}"#
            } else {
                r#"{"access_token":"primero","refresh_token":"para-renovar",
                    "token_type":"Bearer","expires_in":3600}"#
            };
            // `Connection: close` because this server serves one request per
            // connection and the client reuses them: without it the second call
            // waits for nobody.
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                json.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(json.as_bytes());
            let _ = stream.flush();
        }
    });
    Service { port, seen }
}

fn auth(port: u16) -> Auth {
    Auth::OAuth2 {
        authorize_url: format!("http://127.0.0.1:{port}/authorize"),
        token_url: format!("http://127.0.0.1:{port}/token"),
        client_id: "chilpa".into(),
        scopes: vec!["leer".into()],
    }
}

/// Does what the browser would: fetch the redirect URL.
fn browser(redirect: &str, query: &str) {
    let rest = redirect.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let mut stream = std::net::TcpStream::connect(host).unwrap();
    let _ = write!(stream, "GET /{path}?{query} HTTP/1.1\r\nHost: {host}\r\n\r\n");
    let _ = stream.flush();
    let mut sink = Vec::new();
    let _ = stream.read_to_end(&mut sink);
}

fn now() -> i64 {
    1_000_000
}

/// The real request, which is what Chilpa injects. The crate does not know
/// the network; here it is lent one.
fn post(url: &str, body: &str) -> Result<(u16, String), String> {
    let request = serde_json::json!({
        "url": url,
        "method": "POST",
        "headers": {
            "Content-Type": "application/x-www-form-urlencoded",
            "Accept": "application/json",
        },
        "body": body,
    });
    let raw = chilpa_stream::request::send(&serde_json::to_vec(&request).unwrap(), None);
    let reply: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if let Some(e) = reply.get("error").and_then(|e| e.as_str()) {
        return Err(e.to_string());
    }
    Ok((
        reply.get("status").and_then(|s| s.as_u64()).unwrap_or(0) as u16,
        reply.get("body").and_then(|b| b.as_str()).unwrap_or_default().to_string(),
    ))
}

#[test]
fn a_whole_connection_from_the_browser_coming_back_to_a_renewed_token() {
    let svc = service();
    let auth = auth(svc.port);
    assert_eq!(auth.problem(), None, "loopback por http se acepta");

    let back = Loopback::bind().unwrap();
    let pkce = Pkce::new();
    let url = authorize_url(&auth, &pkce, &back).unwrap();
    assert!(url.contains("code_challenge="), "{url}");
    assert!(!url.contains(pkce.verifier()), "el verificador no va en la URL");

    // The browser comes back with the code and the same state.
    let redirect = back.redirect_uri();
    let state = back.state().to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        browser(&redirect, &format!("code=el-codigo&state={state}"));
    });

    let code = chilpa_auth::wait_for_code(&back).expect("llegó el código");
    assert_eq!(code, "el-codigo");

    // Exchanged for tokens. Here, and only here, the verifier travels.
    let tokens = exchange(&auth, &code, &pkce, &back, now(), &post).expect("cambió");
    assert_eq!(tokens.access_token, "primero");
    assert_eq!(tokens.refresh_token, "para-renovar");
    assert_eq!(tokens.expires_at, Some(now() + 3600));
    assert!(!tokens.stale(now()), "recién sacado");

    let asked = svc.seen.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].contains("grant_type=authorization_code"));
    assert!(asked[0].contains(&format!("code_verifier={}", pkce.verifier())), "{}", asked[0]);
    assert!(asked[0].contains("client_id=chilpa"));

    // And when it expires, it renews without bothering anyone.
    let old = Tokens { expires_at: Some(now() - 1), ..tokens.clone() };
    assert!(old.stale(now()));
    assert!(old.renewable());
    let fresh = refresh(&auth, &old, now(), &post).expect("renovó");
    assert_eq!(fresh.access_token, "segundo");
    assert_eq!(
        fresh.refresh_token, "para-renovar",
        "el servicio no mandó uno nuevo, así que se conserva el que había"
    );
    assert_eq!(fresh.expires_at, Some(now() + 3600));
}

#[test]
fn a_reply_with_someone_else_state_is_refused_without_using_the_code() {
    let back = Loopback::bind().unwrap();
    let redirect = back.redirect_uri();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        browser(&redirect, "code=robado&state=de-alguien-mas");
    });
    let e = chilpa_auth::wait_for_code(&back).expect_err("no se acepta");
    assert!(matches!(e, chilpa_auth::AuthError::WrongState), "{e}");
}

#[test]
fn a_user_who_says_no_is_reported_and_not_retried() {
    let back = Loopback::bind().unwrap();
    let redirect = back.redirect_uri();
    let state = back.state().to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        browser(&redirect, &format!("error=access_denied&state={state}"));
    });
    let e = chilpa_auth::wait_for_code(&back).expect_err("dijo que no");
    assert!(matches!(e, chilpa_auth::AuthError::NoCode(ref s) if s == "access_denied"), "{e}");
}
