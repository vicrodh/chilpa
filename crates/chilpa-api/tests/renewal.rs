//! An expired access renews itself, on Chilpa's side.
//!
//! The plugin asks to reach its domain with yesterday's token kept. Chilpa
//! sees it expired, swaps it for a new one against the service, keeps the
//! new one encrypted, and the plugin's request goes out signed with that
//! one. The plugin takes no part: it has no way to ask for the token and no
//! way to know a renewal happened (ADR-008).
//!
//! It is a single test because it points `XDG_DATA_HOME` at a temporary
//! directory, and that variable belongs to the whole process.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A service that renews tokens and keeps what each request was signed
/// with, so it can say what it saw.
struct Service {
    port: u16,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

fn service() -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mine = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            let text = loop {
                let read = stream.read(&mut buf).unwrap_or(0);
                if read == 0 {
                    break String::from_utf8_lossy(&raw).to_string();
                }
                raw.extend_from_slice(&buf[..read]);
                let text = String::from_utf8_lossy(&raw).to_string();
                let Some((head, body)) = text.split_once("\r\n\r\n") else { continue };
                let want: usize = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse().ok())
                    })
                    .unwrap_or(0);
                if body.len() >= want {
                    break text;
                }
            };

            let target = text.split_whitespace().nth(1).unwrap_or("").to_string();
            let authorization = text
                .lines()
                .find_map(|l| l.strip_prefix("authorization: ").or(l.strip_prefix("Authorization: ")))
                .unwrap_or("")
                .trim()
                .to_string();
            mine.lock().unwrap().push((target.clone(), authorization));

            // The first is born expired on purpose: it is an account
            // connected a while ago whose access no longer works. The
            // second, from the renewal, lasts an hour.
            let json = if target.starts_with("/token") {
                if text.contains("grant_type=refresh_token") {
                    r#"{"access_token":"new-token","token_type":"Bearer","expires_in":3600}"#
                } else {
                    r#"{"access_token":"old-token","refresh_token":"to-renew",
                        "token_type":"Bearer","expires_in":0}"#
                }
            } else {
                r#"{"ok":true}"#
            };
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

/// A plugin that only reaches the network, with whatever domain and port it
/// is given. The data's length is computed here.
fn install(dir: &Path, id: &str, port: u16) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(
        dir.join(id).join("manifest.json"),
        format!(
            r#"{{"id": "{id}", "name": "One", "version": "0.1.0", "api_version": 1,
                 "kind": "source",
                 "auth": {{"type": "oauth2",
                           "authorize_url": "http://127.0.0.1:{port}/authorize",
                           "token_url": "http://127.0.0.1:{port}/token",
                           "client_id": "chilpa"}},
                 "capabilities": [{{"type":"network","domains":["127.0.0.1"]}}]}}"#
        ),
    )
    .unwrap();

    let request = format!(r#"{{"url":"http://127.0.0.1:{port}/api/algo","method":"GET"}}"#);
    let esc = request.replace('"', "\\\"");
    let wat = format!(
        r#"(module
            (import "chilpa" "http" (func $http (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (global $next (mut i32) (i32.const 8192))
            (func (export "chilpa_alloc") (param i32) (result i32)
              (local i32)
              (local.set 1 (global.get $next))
              (global.set $next (i32.add (global.get $next) (local.get 0)))
              (local.get 1))
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (data (i32.const 16) "{esc}")
            (data (i32.const 4096) "{{\"ready\":true}}")
            ;; It reaches the network and answers something fixed: what is
            ;; watched is what it went out with, not what it answered.
            (func (export "go_out") (param i32 i32) (result i64)
              (drop (call $http (i32.const 16) (i32.const {len})
                                (i32.const 6000) (i32.const 1024)))
              (i64.or (i64.shl (i64.const 4096) (i64.const 32)) (i64.const 14))))"#,
        len = request.len(),
    );
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(&wat).expect("it compiles"))
        .unwrap();
}

#[test]
fn an_expired_access_is_renewed_by_chilpa_and_the_plugin_never_sees_a_token() {
    let dir = std::env::temp_dir().join("chilpa-api-renewal");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));

    let svc = service();
    let root = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&root).unwrap();
    install(&root, "com.test.account", svc.port);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.load_plugins();

    // What it asks for includes where it will send the user to grant permission.
    let listed = chilpa.plugins();
    assert_eq!(listed[0].state, "awaiting approval", "{:?}", listed[0].problem);
    let pending = listed[0].pending.clone();
    assert!(
        pending.iter().any(|p| p.contains("grant permission at 127.0.0.1")),
        "where it sends you is approved too: {pending:?}"
    );
    assert_eq!(chilpa.plugins()[0].account_host, "127.0.0.1");
    assert_eq!(chilpa.approve_plugin("com.test.account"), 1);

    // The account is connected down the real path: Chilpa gives the URL,
    // the browser comes back with the code, and Chilpa swaps it for tokens.
    let url = chilpa.connect_account("com.test.account").expect("there is somewhere to send you");
    assert!(url.contains("code_challenge="), "with PKCE: {url}");
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A"), "to a loopback: {url}");

    let redirect = between(&url, "redirect_uri=", "&");
    let state = between(&url, "state=", "&");
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        browser(&unescape(&redirect), &format!("code=the-code&state={state}"));
    });
    // The wait runs on another thread; what it answers arrives through
    // poll, which is how the window will see it.
    assert!(chilpa.connecting(), "the browser is being waited for");
    assert!(
        wait_for(&mut chilpa, |c| c.plugins()[0].connected),
        "the account ended up connected: {}",
        chilpa.now_playing().error
    );
    assert!(!chilpa.connecting(), "and nobody is being waited for any more");
    assert!(!chilpa.plugins()[0].account_expired, "it expired, but there is something to renew it with");

    // The plugin reaches the network. It asked for no token: it has no way to.
    chilpa.call_plugin("com.test.account", "go_out", b"{}").expect("it went out");

    let seen = svc.seen.lock().unwrap().clone();
    // Two: the code exchange on connecting, and this renewal.
    let at_token: Vec<_> = seen.iter().filter(|(t, _)| t.starts_with("/token")).collect();
    let calls: Vec<_> = seen.iter().filter(|(t, _)| t.starts_with("/api/")).collect();

    assert_eq!(at_token.len(), 2, "the exchange and the renewal: {seen:?}");
    assert_eq!(calls.len(), 1, "and the plugin's request went out: {seen:?}");
    assert_eq!(
        calls[0].1, "Bearer new-token",
        "signed with the renewed token, not the expired one"
    );

    // What is kept ended up current, so the next one does not renew again.
    chilpa.call_plugin("com.test.account", "go_out", b"{}").expect("it went out again");
    let seen = svc.seen.lock().unwrap().clone();
    assert_eq!(
        seen.iter().filter(|(t, _)| t.starts_with("/token")).count(),
        2,
        "the new token was kept: it does not renew on every request"
    );

    // And neither of the two is on disk in the clear.
    let sealed =
        std::fs::read(root.join("com.test.account").join("credential.sealed")).unwrap();
    for secret in ["old-token", "new-token", "to-renew"] {
        assert!(
            !sealed.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "{secret} was left in the clear"
        );
    }

    let _ = std::fs::remove_dir_all(dir);
}

/// Calls `poll` until the condition holds, or patience runs out.
fn wait_for(
    chilpa: &mut chilpa_api::Chilpa,
    mut cond: impl FnMut(&mut chilpa_api::Chilpa) -> bool,
) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        chilpa.poll();
        if cond(chilpa) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

/// What lies between two markers of a URL.
fn between(text: &str, from: &str, to: &str) -> String {
    let rest = text.split_once(from).map(|(_, r)| r).unwrap_or("");
    rest.split_once(to).map(|(v, _)| v).unwrap_or(rest).to_string()
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
            match u8::from_str_radix(hex, 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                }
                Err(_) => {
                    out.push(b[i]);
                    i += 1;
                }
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Does what the browser would do on coming back.
fn browser(redirect: &str, query: &str) {
    let rest = redirect.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let mut stream = std::net::TcpStream::connect(host).unwrap();
    let _ = write!(stream, "GET /{path}?{query} HTTP/1.1\r\nHost: {host}\r\n\r\n");
    let _ = stream.flush();
    let mut sink = Vec::new();
    let _ = stream.read_to_end(&mut sink);
}
