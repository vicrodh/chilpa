//! One permission-checked HTTP hop for a guest which handles its own account.

use base64::Engine;
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};

const MAX_BODY: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerNetworkError { Denied, Invalid, Timeout, Transport, TooLarge }

pub fn worker_url_allowed(url: &str, domains: &[String]) -> bool {
    let Ok(uri) = url.parse::<ureq::http::Uri>() else { return false };
    let Some(authority) = uri.authority() else { return false };
    let Some(host) = uri.host() else { return false };
    let host = host.to_ascii_lowercase();
    matches!(uri.scheme_str(), Some("https" | "http")) && !authority.as_str().contains('@')
        && domains.iter().any(|domain| domain == "*" || host == domain.to_ascii_lowercase() || host.ends_with(&format!(".{}", domain.to_ascii_lowercase())))
}

/// Authentication headers belong to this guest. They are never replaced with
/// native account credentials. Redirects return to the worker as responses;
/// every subsequent hop must pass the same domain check.
/// One agent for every worker request, so its connections are kept and
/// reused: a plugin asks the same hosts over and over, and a new agent per
/// request paid DNS, TCP and TLS each time. No cookies are kept in it; the
/// guest sends its own as headers.
fn shared_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| crate::network::agent(ureq::Agent::config_builder().max_redirects(0).http_status_as_error(false).middleware(crate::network::guard).build()))
}

pub fn send_worker_request(
    request: &Value,
    domains: &[String],
    timeout: Duration,
) -> Result<Value, WorkerNetworkError> {
    if timeout.is_zero() { return Err(WorkerNetworkError::Timeout); }
    let url = request.get("url").and_then(Value::as_str).ok_or(WorkerNetworkError::Invalid)?;
    if crate::network::check(url).is_err() || !worker_url_allowed(url, domains) { return Err(WorkerNetworkError::Denied); }
    let method = request.get("method").and_then(Value::as_str).unwrap_or("GET");
    if !matches!(method, "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE") {
        return Err(WorkerNetworkError::Invalid);
    }
    let mut headers = Vec::new();
    if let Some(values) = request.get("headers").and_then(Value::as_object) {
        if values.len() > 64 { return Err(WorkerNetworkError::TooLarge); }
        for (name, value) in values {
            let value = value.as_str().ok_or(WorkerNetworkError::Invalid)?;
            if name.len() + value.len() > 16384 { return Err(WorkerNetworkError::TooLarge); }
            if ["host", "content-length", "transfer-encoding", "connection", "proxy-authorization"]
                .iter().any(|reserved| name.eq_ignore_ascii_case(reserved)) { continue; }
            headers.push((name.as_str(), value));
        }
    }
    let encoded = request.get("body_base64").and_then(Value::as_str).unwrap_or("");
    if encoded.len() > 2 * MAX_BODY { return Err(WorkerNetworkError::TooLarge); }
    let body = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| WorkerNetworkError::Invalid)?;
    if body.len() > MAX_BODY { return Err(WorkerNetworkError::TooLarge); }
    let agent = shared_agent();
    let timeout = Some(timeout.min(Duration::from_secs(30)));
    let reply = if matches!(method, "POST" | "PUT" | "PATCH") {
        let mut builder = match method { "PUT" => agent.put(url), "PATCH" => agent.patch(url), _ => agent.post(url) };
        for (key, value) in &headers { builder = builder.header(*key, *value); }
        builder.config().timeout_global(timeout).build().send(body.as_slice())
    } else {
        if !body.is_empty() { return Err(WorkerNetworkError::Invalid); }
        let mut builder = match method { "HEAD" => agent.head(url), "DELETE" => agent.delete(url), _ => agent.get(url) };
        for (key, value) in &headers { builder = builder.header(*key, *value); }
        builder.config().timeout_global(timeout).build().call()
    }.map_err(|_| WorkerNetworkError::Transport)?;
    let status = reply.status().as_u16();
    let mut headers: BTreeMap<String, Value> = BTreeMap::new();
    let mut header_bytes = 0;
    for (index, (key, value)) in reply.headers().iter().enumerate() {
        header_bytes += key.as_str().len() + value.as_bytes().len();
        if index >= 128 || header_bytes > 65536 { return Err(WorkerNetworkError::TooLarge); }
        let value = Value::String(value.to_str().map_err(|_| WorkerNetworkError::Invalid)?.to_string());
        match headers.get_mut(key.as_str()) {
            Some(Value::Array(values)) => values.push(value),
            Some(previous) => *previous = Value::Array(vec![previous.clone(), value]),
            None => { headers.insert(key.as_str().to_string(), value); }
        }
    }
    let mut response = reply.into_body();
    let bytes = response.with_config().limit(MAX_BODY as u64 + 1).read_to_vec()
        .map_err(|_| WorkerNetworkError::Transport)?;
    if bytes.len() > MAX_BODY { return Err(WorkerNetworkError::TooLarge); }
    Ok(json!({"status": status, "headers": headers,
        "body_base64": base64::engine::general_purpose::STANDARD.encode(bytes)}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn guest_authorization_is_preserved_but_redirects_are_not_followed() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/token", listener.local_addr().unwrap().port());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut bytes = Vec::new();
            let mut byte = [0];
            while !bytes.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
            }
            socket.write_all(b"HTTP/1.1 302 Found\r\nLocation: https://unapproved.example/token\r\nSet-Cookie: account=one; Path=/\r\nSet-Cookie: session=two; Path=/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            String::from_utf8(bytes).unwrap()
        });
        let reply = send_worker_request(&json!({"url":url,"method":"GET", "headers":{
            "Authorization":"Bearer fixture", "Host":"forged.example"}, "body_base64":""}),
            &["127.0.0.1".into()], Duration::from_secs(3)).unwrap();
        assert_eq!(reply["status"], 302);
        assert_eq!(reply["headers"]["set-cookie"], json!(["account=one; Path=/", "session=two; Path=/"]));
        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("authorization: bearer fixture"));
        assert!(!request.contains("forged.example"));
    }

    #[test]
    fn an_explicit_network_grant_includes_dynamic_audio_servers() {
        let domains = vec!["*".into()];
        for url in ["https://audio.example.test/file", "https://new-cdn.example.test/file"] {
            assert!(worker_url_allowed(url, &domains));
        }
        for url in ["file:///etc/passwd", "https://user:password@audio.example.test/file"] {
            assert!(!worker_url_allowed(url, &domains));
        }
    }

    #[test]
    fn unapproved_hosts_and_userinfo_are_refused_before_network_io() {
        for url in ["https://evil.example/", "https://good.example.evil.example/", "https://user@good.example/"] {
            assert_eq!(send_worker_request(&json!({"url":url}), &["good.example".into()], Duration::from_secs(1)),
                Err(WorkerNetworkError::Denied));
        }
        assert_eq!(send_worker_request(&json!({"url":"https://good.example/"}), &["good.example".into()], Duration::ZERO),
            Err(WorkerNetworkError::Timeout));
    }
}
