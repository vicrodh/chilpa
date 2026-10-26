//! A bounded request that hands back bytes.
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

#[derive(Clone)]
pub struct Fetch<'a> {
    pub url: &'a str,
    pub user_agent: &'a str,
    /// What is asked for: "application/json", "image/*".
    pub accept: &'a str,
    pub timeout: Duration,
    /// A reply larger than this is refused.
    pub max_bytes: usize,
    /// I send this only to an explicitly allowed origin and never across a redirect.
    pub authorization: Option<&'a str>,
}

impl std::fmt::Debug for Fetch<'_> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Fetch").field("timeout",&self.timeout).field("max_bytes",&self.max_bytes).finish_non_exhaustive()
    }
}

/// User credentials belong to the direct API. The one-use handshake secret
/// belongs only to the worker's finish route and has its own scheme.
pub fn permits_authorization(address: &str, authorization: &str) -> bool {
    let Ok(url) = url::Url::parse(address) else { return false };
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some()
        || url.port_or_known_default() != Some(443) || authorization.contains(['\r','\n']) { return false; }
    match url.host_str() {
        Some("api.discogs.com") => authorization.starts_with("Discogs token=") || authorization.starts_with("OAuth "),
        Some("api.chilpa.app") => url.path() == "/discogs/oauth/finish" && authorization.starts_with("Chilpa "),
        _ => false,
    }
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
pub fn fetch(request: &Fetch) -> Result<Fetched, FetchError> { ask(request,false) }

/// I restrict bodyless authenticated POSTs to the two direct OAuth endpoints.
/// The application credential never crosses a redirect or reaches another host.
pub fn fetch_oauth(request:&Fetch)->Result<Fetched,FetchError> {
    let url=url::Url::parse(request.url).map_err(|_|FetchError::Scheme)?;
    if url.host_str()!=Some("api.discogs.com") || !matches!(url.path(),"/oauth/request_token"|"/oauth/access_token")
        || url.query().is_some() || url.fragment().is_some()
        || !request.authorization.is_some_and(|header|header.starts_with("OAuth ") && permits_authorization(request.url,header)) {
        return Err(FetchError::Scheme);
    }
    ask(request,true)
}

fn ask(request:&Fetch,post:bool)->Result<Fetched,FetchError> {
    let local = local(request.url);
    if !request.url.starts_with("https://") && !local {
        return Err(FetchError::Scheme);
    }
    let agent = crate::network::agent(ureq::Agent::config_builder()
        .timeout_global(Some(request.timeout))
        .http_status_as_error(false)
        // A redirect to plain http is refused too.
        .https_only(!local)
        .max_redirects(if request.authorization.is_some() || post { 0 } else { 5 })
        .middleware(crate::network::guard)
        .build());
    let reply = if post {
        let mut asked=agent.post(request.url).header("User-Agent",request.user_agent)
            .header("Accept",request.accept).header("Content-Type","application/x-www-form-urlencoded");
        if let Some(header)=request.authorization.filter(|value|permits_authorization(request.url,value)) {asked=asked.header("Authorization",header);}
        asked.send_empty()
    } else {
        let mut asked=agent.get(request.url).header("User-Agent",request.user_agent).header("Accept",request.accept);
        if let Some(header)=request.authorization.filter(|value|permits_authorization(request.url,value)) {asked=asked.header("Authorization",header);}
        asked.call()
    }
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
            authorization: None,
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
    #[test]
    fn oauth_posts_refuse_other_origins_paths_credentials_and_query_strings() {
        for address in ["https://evil.test/oauth/request_token","https://api.discogs.com/releases/1",
            "http://api.discogs.com/oauth/request_token","https://api.discogs.com/oauth/request_token?secret=marker",
            "https://api.discogs.com/oauth/access_token#fragment","https://api.discogs.com:8443/oauth/request_token"] {
            let request=Fetch {url:address,user_agent:"fixture",accept:"*/*",timeout:Duration::from_secs(1),max_bytes:64,authorization:Some("OAuth marker")};
            assert!(matches!(fetch_oauth(&request),Err(FetchError::Scheme)),"{address}");
        }
        let request=Fetch {url:"https://api.discogs.com/oauth/request_token",user_agent:"fixture",accept:"*/*",timeout:Duration::from_secs(1),max_bytes:64,authorization:None};
        assert!(matches!(fetch_oauth(&request),Err(FetchError::Scheme)));
    }
    #[test]
    fn the_oauth_transport_sends_a_bodyless_post_and_preserves_redirects() {
        let (url,asked)=answering(reply("302 Found","Location: https://api.discogs.com/oauth/request_token\r\n",b""));
        let request=Fetch {url:&url,user_agent:"fixture",accept:"application/x-www-form-urlencoded",timeout:Duration::from_secs(1),max_bytes:64,authorization:None};
        assert_eq!(super::ask(&request,true).unwrap().status,302);
        let wire=asked.join().unwrap();assert!(wire.starts_with("POST /x HTTP/1.1"));
        assert!(wire.to_ascii_lowercase().contains("content-type: application/x-www-form-urlencoded"));
    }
    #[test]
    fn credentials_have_exact_origins_and_separate_handshake_scope() {
        let token = "Discogs token=MARKER_TOKEN";
        assert!(permits_authorization("https://api.discogs.com/oauth/identity",token));
        assert!(permits_authorization("https://api.discogs.com/releases/1","OAuth marker"));
        assert!(permits_authorization("https://api.chilpa.app/discogs/oauth/finish?oauth_token=public","Chilpa request-secret"));
        for address in ["https://i.discogs.com/image", "https://api.discogs.com.evil.test/x", "https://api.discogs.com@evil.test/x",
            "http://api.discogs.com/x", "https://api.discogs.com:8443/x", "https://api.chilpa.app/discogs/oauth/finish", "https://api.chilpa.app/discogs/search"] {
            assert!(!permits_authorization(address,token),"{address}");
        }
        assert!(!permits_authorization("https://api.chilpa.app/discogs/search","Chilpa request-secret"));
    }
    #[test]
    fn an_unrelated_host_receives_no_credential_and_auth_requests_follow_no_redirects() {
        let (url, asked) = answering(reply("302 Found","Location: https://api.discogs.com/oauth/identity\r\n",b""));
        let request = Fetch {url:&url,user_agent:"fixture",accept:"*/*",timeout:Duration::from_secs(2),max_bytes:64,
            authorization:Some("Discogs token=MARKER_TOKEN")};
        let got = fetch(&request).unwrap();
        assert_eq!(got.status,302);
        let wire = asked.join().unwrap();
        assert!(!wire.to_ascii_lowercase().contains("authorization:"));
        assert!(!wire.contains("MARKER_TOKEN"));
        assert!(!format!("{request:?}").contains("MARKER_TOKEN"));
    }
}
