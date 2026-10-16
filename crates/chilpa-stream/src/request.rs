//! An HTTP request made on someone else's behalf.
//!
//! It is what Chilpa does when a plugin asks to go out to the network
//! (ADR-007). The plugin says where and with what; **Chilpa puts in the
//! credential**, on this side, and it was never in the plugin's memory.
//!
//! JSON in and out, because it is what any language can read and what can be
//! looked at by eye when something fails.
//!
//! What this module does not do, on purpose: it does not decide whether the
//! domain is allowed. That was checked before getting here, with the
//! manifest in hand, and repeating it in two places is having two rules that
//! can drift apart.

use std::collections::BTreeMap;
use std::time::Duration;

/// How long to wait for the other side to answer.
const TIMEOUT: Duration = Duration::from_secs(20);

/// How much of a reply is accepted. More than this is someone sending a
/// record through an API, and it has no business fitting in memory.
const MAX: usize = 8 * 1024 * 1024;

/// What was asked for, already read out of the JSON.
#[derive(Debug, Default)]
struct Request {
    url: String,
    method: String,
    headers: BTreeMap<String, String>,
    body: String,
}

/// Makes the request and answers in JSON.
///
/// Never returns `Err`: a network problem is a reply carrying `error`,
/// because on the other side is a plugin that only understands JSON.
pub fn send(request_json: &[u8], credential: Option<&str>) -> Vec<u8> {
    let p = match read(request_json) {
        Ok(p) => p,
        Err(e) => return error(&e),
    };
    if !p.url.starts_with("https://") && !p.url.starts_with("http://") {
        return error(&chilpa_i18n::t("the URL has to be http or https"));
    }

    // A status the other side answers with is part of the reply, not a
    // failure of the request: a 401 says the token is dead, a 429 says when
    // to come back, and the body says why. ureq turns them into errors unless
    // told not to, and that hid all of it behind "http status: 401".
    let agent = crate::network::agent(ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .middleware(crate::network::guard)
        .build());

    // The headers that really go out. The plugin cannot set its own
    // authorization: Chilpa sets that, on this side.
    let mut headers: Vec<(String, String)> = p
        .headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("authorization"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if let Some(c) = credential {
        headers.push(("Authorization".into(), c.to_string()));
    }
    headers.push(("User-Agent".into(), "Chilpa".into()));

    // ureq distinguishes by type the methods that carry a body from those
    // that do not, so the two branches are written separately.
    let reply = if matches!(p.method.as_str(), "POST" | "PUT" | "PATCH") {
        let mut req = match p.method.as_str() {
            "PUT" => agent.put(&p.url),
            "PATCH" => agent.patch(&p.url),
            _ => agent.post(&p.url),
        };
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        req.send(&p.body)
    } else {
        let mut req = match p.method.as_str() {
            "DELETE" => agent.delete(&p.url),
            "HEAD" => agent.head(&p.url),
            _ => agent.get(&p.url),
        };
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        req.call()
    };

    match reply {
        Ok(r) => read_reply(r),
        Err(e) => error(&chilpa_i18n::t_args("could not: {}", &[&e.to_string()])),
    }
}

fn read_reply(r: ureq::http::Response<ureq::Body>) -> Vec<u8> {
    let status = r.status().as_u16();
    let headers: BTreeMap<String, String> = r
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
        .collect();
    let body = match r.into_body().read_to_string() {
        Ok(s) if s.len() <= MAX => s,
        Ok(_) => return error(&chilpa_i18n::t("the reply does not fit")),
        Err(e) => return error(&chilpa_i18n::t_args("the reply could not be read: {}", &[&e.to_string()])),
    };
    serde_json::to_vec(&serde_json::json!({
        "status": status,
        "headers": headers,
        "body": body,
    }))
    .unwrap_or_else(|_| error(&chilpa_i18n::t("the reply could not be built")))
}

fn read(json: &[u8]) -> Result<Request, String> {
    let v: serde_json::Value =
        serde_json::from_slice(json).map_err(|e| chilpa_i18n::t_args("the request is not JSON: {}", &[&e.to_string()]))?;
    let url = v
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or(&chilpa_i18n::t("the request carries no url"))?
        .to_string();
    let method = v
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let headers = v
        .get("headers")
        .and_then(|h| h.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let body = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();
    Ok(Request { url, method, headers, body })
}

fn error(reason: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "error": reason })).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_json(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).expect("JSON is always answered")
    }

    #[test]
    fn a_request_that_is_not_json_answers_an_error_in_json() {
        let r = as_json(&send(b"junk", None));
        assert!(r["error"].as_str().unwrap().contains("not JSON"));
    }

    #[test]
    fn no_url_means_no_request() {
        let r = as_json(&send(b"{}", None));
        assert!(r["error"].as_str().unwrap().contains("no url"));
    }

    #[test]
    fn a_scheme_that_is_not_http_is_refused_without_going_out() {
        for url in ["file:///etc/passwd", "ftp://something/x", "chilpa://internal"] {
            let request = format!(r#"{{"url":"{url}"}}"#);
            let r = as_json(&send(request.as_bytes(), None));
            assert!(
                r["error"].as_str().unwrap().contains("http"),
                "\"{url}\" should not be attempted: {r}"
            );
        }
    }

    #[test]
    fn a_status_that_is_not_200_comes_back_with_its_body_and_headers() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let answer = std::thread::spawn(move || {
            let (mut peer, _) = server.accept().unwrap();
            let mut asked = [0u8; 1024];
            let _ = peer.read(&mut asked);
            let body = r#"{"error":"token expired"}"#;
            let reply = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nX-RateLimit-Reset-In: 7\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            peer.write_all(reply.as_bytes()).unwrap();
        });
        let request = format!(r#"{{"url":"http://127.0.0.1:{port}/x"}}"#);
        let r = as_json(&send(request.as_bytes(), None));
        answer.join().unwrap();
        assert!(r.get("error").is_none(), "a 401 is an answer, not a failure: {r}");
        assert_eq!(r["status"], 401);
        assert_eq!(r["body"], r#"{"error":"token expired"}"#);
        assert_eq!(r["headers"]["x-ratelimit-reset-in"], "7");
    }

    #[test]
    fn what_the_plugin_asked_for_is_read() {
        let p = read(br#"{"url":"https://a.b/c","method":"post","headers":{"X":"1"},"body":"hi"}"#)
            .unwrap();
        assert_eq!(p.url, "https://a.b/c");
        assert_eq!(p.method, "POST", "the method is normalized");
        assert_eq!(p.headers.get("X").map(String::as_str), Some("1"));
        assert_eq!(p.body, "hi");
    }

    #[test]
    fn no_method_means_a_get() {
        let p = read(br#"{"url":"https://a.b"}"#).unwrap();
        assert_eq!(p.method, "GET");
        assert!(p.headers.is_empty() && p.body.is_empty());
    }
}
