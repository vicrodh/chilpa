//! Talking to a server of mine: Subsonic, Jellyfin, Plex (ADR-017).
//!
//! Three things set it apart from `fetch`, which is for the catalogues on
//! the internet:
//!
//! - **Plain http is allowed.** A server on the LAN rarely has a
//!   certificate, and refusing it would refuse most of them.
//! - **No redirect is followed.** A Subsonic credential travels in the
//!   query, and following a redirect would hand it to wherever the redirect
//!   points. A 3xx comes back as an answer; the window says where it points
//!   so the address can be corrected.
//! - **Certificates are checked against the system's.** A home CA installed
//!   in the system works. There is no switch to accept a certificate that
//!   does not check out.
//!
//! No error text carries the address: a Subsonic address carries a
//! credential in its query.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::fetch::{FetchError, Fetched};

/// One request to a server.
#[derive(Clone, Debug)]
pub struct Call<'a> {
    /// `GET` or `POST`.
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [(String, String)],
    /// The body and its type, for a `POST`.
    pub body: Option<(&'a str, &'a [u8])>,
    pub timeout: Duration,
    /// A reply larger than this is refused.
    pub max_bytes: usize,
}

pub(crate) fn agent(response: Duration, body: Duration) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    crate::network::agent(ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(response))
        .timeout_recv_body(Some(body))
        .tls_config(tls)
        .middleware(crate::network::guard)
        .build())
}

/// Why nothing came back, in words that name no address.
pub(crate) fn describe(error: &ureq::Error) -> String {
    match error {
        ureq::Error::Timeout(_) => chilpa_i18n::t("the server took too long to answer"),
        ureq::Error::HostNotFound => chilpa_i18n::t("that server name does not resolve"),
        ureq::Error::ConnectionFailed => chilpa_i18n::t("the server could not be reached"),
        ureq::Error::BadUri(_) => chilpa_i18n::t("that is not a server address"),
        ureq::Error::BodyExceedsLimit(_) => chilpa_i18n::t("the server sent more than was asked for"),
        ureq::Error::Io(e) => chilpa_i18n::t_args("the connection failed: {}", &[&e.kind().to_string()]),
        other => {
            let text = other.to_string();
            // A certificate that does not check out is worth naming as such:
            // it is the one failure the user can do something specific about.
            if text.to_ascii_lowercase().contains("certificate") {
                chilpa_i18n::t("the server's certificate is not trusted by this system")
            } else {
                chilpa_i18n::t("the server answered with something that is not HTTP")
            }
        }
    }
}

/// Asks once. Any status is an answer.
pub fn call(request: &Call) -> Result<Fetched, FetchError> {
    if !request.url.starts_with("http://") && !request.url.starts_with("https://") {
        return Err(FetchError::Scheme);
    }
    let agent = agent(request.timeout, request.timeout);
    let reply = match (request.method, request.body) {
        ("POST", body) => {
            let mut builder = agent.post(request.url);
            for (name, value) in request.headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            match body {
                Some((kind, bytes)) => builder.header("Content-Type", kind).send(bytes),
                None => builder.send_empty(),
            }
        }
        _ => {
            let mut builder = agent.get(request.url);
            for (name, value) in request.headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            builder.call()
        }
    }
    .map_err(|e| FetchError::Transport(describe(&e)))?;

    let status = reply.status().as_u16();
    let headers: BTreeMap<String, String> = reply
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.as_str().to_ascii_lowercase(), v.to_str().ok()?.to_string())))
        .collect();
    let mut body = reply.into_body();
    // ureq's limit fails the read that finds the end right after the last
    // allowed byte, so it is set one past what is allowed.
    let body = body.with_config().limit(request.max_bytes as u64 + 1).read_to_vec().map_err(|e| match e {
        ureq::Error::BodyExceedsLimit(_) => FetchError::TooLarge,
        other => FetchError::Transport(describe(&other)),
    })?;
    Ok(Fetched { status, headers, body })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn answering(reply: Vec<u8>) -> (String, std::thread::JoinHandle<String>) {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let asked = std::thread::spawn(move || {
            let (mut peer, _) = server.accept().unwrap();
            peer.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
            // The headers and the body can come in separate reads.
            let mut asked = Vec::new();
            let mut chunk = [0u8; 4096];
            while let Ok(n) = peer.read(&mut chunk) {
                if n == 0 { break; }
                asked.extend_from_slice(&chunk[..n]);
            }
            peer.write_all(&reply).unwrap();
            String::from_utf8_lossy(&asked).into_owned()
        });
        (format!("http://127.0.0.1:{port}/rest/ping.view?u=me&t=secret&s=salt"), asked)
    }

    fn ask(url: &str, method: &str, body: Option<(&str, &[u8])>) -> Result<Fetched, FetchError> {
        let headers = vec![("X-Plex-Token".to_string(), "tok".to_string())];
        call(&Call { method, url, headers: &headers, body, timeout: Duration::from_secs(5), max_bytes: 1024 })
    }

    #[test]
    fn plain_http_on_the_lan_is_allowed_and_the_headers_go_out() {
        let (url, asked) = answering(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_vec());
        let reply = ask(&url, "GET", None).unwrap();
        assert_eq!((reply.status, reply.body.as_slice()), (200, &b"ok"[..]));
        assert!(asked.join().unwrap().to_ascii_lowercase().contains("x-plex-token: tok"));
    }

    #[test]
    fn a_redirect_comes_back_as_an_answer_and_is_not_followed() {
        let (url, asked) = answering(b"HTTP/1.1 301 Moved\r\nLocation: http://elsewhere.invalid/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec());
        let reply = ask(&url, "GET", None).unwrap();
        assert_eq!(reply.status, 301);
        assert_eq!(reply.headers.get("location").map(String::as_str), Some("http://elsewhere.invalid/"));
        asked.join().unwrap();
    }

    #[test]
    fn a_post_carries_its_body_and_its_type() {
        let (url, asked) = answering(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec());
        ask(&url, "POST", Some(("application/json", br#"{"Username":"me"}"#))).unwrap();
        let asked = asked.join().unwrap();
        assert!(asked.starts_with("POST "));
        assert!(asked.to_ascii_lowercase().contains("content-type: application/json"));
        assert!(asked.contains(r#"{"Username":"me"}"#));
    }

    #[test]
    fn a_failure_never_repeats_the_address_or_its_credential() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        drop(server);
        let url = format!("http://127.0.0.1:{port}/rest/ping.view?u=me&t=secret&s=salt");
        let Err(FetchError::Transport(why)) = ask(&url, "GET", None) else { panic!("nothing listens there") };
        assert!(!why.contains("secret") && !why.contains("127.0.0.1"), "{why}");
    }

    #[test]
    fn only_http_and_https_are_asked() {
        assert!(matches!(ask("ftp://host/x", "GET", None), Err(FetchError::Scheme)));
    }
}
