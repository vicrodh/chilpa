//! What the services somebody runs themselves share: reading the address
//! they typed, and keeping an account of several parts as one credential.

/// Where a server is, from what was typed: trimmed, with `https://` when
/// no scheme was written, and without trailing slashes. `None` for nothing,
/// or for what cannot be an address: a space in it, another scheme.
pub(super) fn origin(typed: &str) -> Option<String> {
    let typed = typed.trim().trim_end_matches('/');
    if typed.is_empty() || typed.contains(char::is_whitespace) {
        return None;
    }
    let with_scheme = if typed.starts_with("https://") || typed.starts_with("http://") {
        typed.to_string()
    } else if typed.contains("://") {
        return None;
    } else {
        format!("https://{typed}")
    };
    let parsed = url::Url::parse(&with_scheme).ok()?;
    parsed.host_str().filter(|host| !host.is_empty())?;
    Some(with_scheme)
}

/// An account's parts as the one text that is sealed: a line each. No part
/// holds a line break: an address, a key and a session never do.
pub(super) fn pack(parts: &[&str]) -> String {
    parts.join("\n")
}

pub(super) fn unpack(account: &str) -> Vec<&str> {
    account.split('\n').collect()
}

/// A server on loopback for the wires' tests: it answers each request with
/// the next reply, as JSON, and hands back what it was sent, whole.
#[cfg(test)]
pub(super) fn answering(replies: Vec<&'static str>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let origin = format!("http://127.0.0.1:{}", listener.local_addr().expect("its address").port());
    let served = std::thread::spawn(move || {
        replies
            .into_iter()
            .map(|reply| {
                let (mut peer, _) = listener.accept().expect("a request");
                let mut sent = Vec::new();
                let mut chunk = [0u8; 4096];
                // The head, then as much body as it says it carries.
                let whole = |sent: &[u8]| {
                    let text = String::from_utf8_lossy(sent);
                    let Some(head) = text.find("\r\n\r\n") else { return false };
                    let length = text[..head]
                        .lines()
                        .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").and_then(|n| n.trim().parse::<usize>().ok()))
                        .unwrap_or(0);
                    sent.len() >= head + 4 + length
                };
                while !whole(&sent) {
                    let n = peer.read(&mut chunk).expect("the request is readable");
                    if n == 0 {
                        break;
                    }
                    sent.extend_from_slice(&chunk[..n]);
                }
                let answer = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
                peer.write_all(answer.as_bytes()).expect("the reply is written");
                String::from_utf8_lossy(&sent).into_owned()
            })
            .collect()
    });
    (origin, served)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_gets_a_scheme_and_loses_its_last_slash() {
        assert_eq!(origin("example.org").as_deref(), Some("https://example.org"));
        assert_eq!(origin("http://example.org:8080//").as_deref(), Some("http://example.org:8080"));
        assert_eq!(origin("https://example.org/fm").as_deref(), Some("https://example.org/fm"));
        assert_eq!(origin("  "), None);
        assert_eq!(origin("ftp://example.org"), None);
        assert_eq!(origin("not an address"), None);
    }

    #[test]
    fn an_account_comes_back_as_the_parts_it_was_made_of() {
        assert_eq!(unpack(&pack(&["https://example.org", "key", "session"])), ["https://example.org", "key", "session"]);
    }
}
