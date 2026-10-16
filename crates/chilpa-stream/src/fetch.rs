//! A GET that hands back bytes.
//!
//! `request::send` is for plugins: JSON in, JSON out, text only. A cover is
//! not text, a catalogue wants to be told who is asking, and a lookup that
//! takes twenty seconds has already lost its listener. This is the request
//! Chilpa makes for itself.
//!
//! It does not retry and it does not pace. Whoever calls knows what the
//! other side allows; this only asks once.

use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Fetch<'a> {
    pub url: &'a str,
    pub user_agent: &'a str,
    /// What is asked for: "application/json", "image/*".
    pub accept: &'a str,
    pub timeout: Duration,
    /// A reply larger than this is refused.
    pub max_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct Fetched {
    pub status: u16,
    /// Names in lower case.
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum FetchError {
    /// Not https, and not this machine.
    Scheme,
    /// Nothing came back: the network, a timeout.
    Transport(String),
    TooLarge,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Scheme => write!(f, "{}", chilpa_i18n::t("the address has to be https")),
            FetchError::Transport(why) => write!(f, "{why}"),
            FetchError::TooLarge => write!(f, "{}", chilpa_i18n::t("the reply does not fit")),
        }
    }
}

/// This machine, which is where the tests listen.
fn local(url: &str) -> bool {
    url.starts_with("http://127.0.0.1:") || url.starts_with("http://127.0.0.1/")
}

/// Asks once. A status that is not 200 is an answer, not a failure.
pub fn fetch(request: &Fetch) -> Result<Fetched, FetchError> {
    let local = local(request.url);
    if !request.url.starts_with("https://") && !local {
        return Err(FetchError::Scheme);
    }
    let agent = crate::network::agent(ureq::Agent::config_builder()
        .timeout_global(Some(request.timeout))
        .http_status_as_error(false)
        // A redirect to plain http is refused too.
        .https_only(!local)
        .max_redirects(5)
        .middleware(crate::network::guard)
        .build());
    let reply = agent
        .get(request.url)
        .header("User-Agent", request.user_agent)
        .header("Accept", request.accept)
        .call()
        .map_err(|e| match e {
            ureq::Error::RequireHttpsOnly(_) => FetchError::Scheme,
            other => FetchError::Transport(other.to_string()),
        })?;

    let status = reply.status().as_u16();
    let headers: BTreeMap<String, String> = reply
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.as_str().to_ascii_lowercase(), v.to_str().ok()?.to_string())))
        .collect();
    let mut body = reply.into_body();
    let body = body.with_config().limit(request.max_bytes as u64).read_to_vec().map_err(|e| match e {
        ureq::Error::BodyExceedsLimit(_) => FetchError::TooLarge,
        other => FetchError::Transport(other.to_string()),
    })?;
    Ok(Fetched { status, headers, body })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// Answers one request with those bytes and hands back what was asked.
    fn answering(reply: Vec<u8>) -> (String, std::thread::JoinHandle<String>) {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let asked = std::thread::spawn(move || {
            let (mut peer, _) = server.accept().unwrap();
            let mut asked = [0u8; 2048];
            let n = peer.read(&mut asked).unwrap_or(0);
            peer.write_all(&reply).unwrap();
            String::from_utf8_lossy(&asked[..n]).into_owned()
        });
        (format!("http://127.0.0.1:{port}/x"), asked)
    }

    fn reply(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
        out.extend_from_slice(body);
        out
    }

    fn ask(url: &str, max_bytes: usize) -> Result<Fetched, FetchError> {
        fetch(&Fetch {
            url,
            user_agent: "Chilpa/0.0.0 ( test )",
            accept: "image/*",
            timeout: Duration::from_secs(5),
            max_bytes,
        })
    }

    #[test]
    fn bytes_that_are_not_text_come_back_whole() {
        let picture: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0xFF, 0xFE, 0x80];
        let (url, asked) = answering(reply("200 OK", "Content-Type: image/png\r\n", &picture));
        let got = ask(&url, 1024).unwrap();
        asked.join().unwrap();
        assert_eq!(got.status, 200);
        assert_eq!(got.body, picture);
        assert_eq!(got.headers["content-type"], "image/png");
    }

    #[test]
    fn a_404_is_an_answer_and_not_a_failure() {
        let (url, asked) = answering(reply("404 Not Found", "", b"nothing here"));
        let got = ask(&url, 1024).unwrap();
        asked.join().unwrap();
        assert_eq!(got.status, 404);
        assert_eq!(got.body, b"nothing here");
    }

    #[test]
    fn a_reply_past_the_limit_is_refused() {
        let (url, asked) = answering(reply("200 OK", "", &[7u8; 4096]));
        let got = ask(&url, 1024);
        asked.join().unwrap();
        assert!(matches!(got, Err(FetchError::TooLarge)), "{got:?}");
    }

    #[test]
    fn plain_http_to_anywhere_else_is_refused_without_going_out() {
        for url in ["http://example.org/x", "ftp://example.org/x", "file:///etc/passwd", "http://127.0.0.1.example.org/x"] {
            assert!(matches!(ask(url, 1024), Err(FetchError::Scheme)), "{url}");
        }
    }

    #[test]
    fn the_user_agent_given_is_the_one_sent() {
        let (url, asked) = answering(reply("200 OK", "", b"ok"));
        ask(&url, 1024).unwrap();
        let asked = asked.join().unwrap().to_lowercase();
        assert!(asked.contains("user-agent: chilpa/0.0.0 ( test )"), "{asked}");
        assert_eq!(asked.matches("user-agent:").count(), 1, "and it is the only one");
        assert!(asked.contains("accept: image/*"));
    }

    #[test]
    fn retry_after_comes_back_among_the_headers() {
        let (url, asked) = answering(reply("503 Service Unavailable", "Retry-After: 4\r\n", b""));
        let got = ask(&url, 1024).unwrap();
        asked.join().unwrap();
        assert_eq!(got.status, 503);
        assert_eq!(got.headers["retry-after"], "4");
    }
}
