//! I stream an explicitly requested public audio file into a caller-owned temporary file.
use std::{
    io::{Read, Write},
    time::Duration,
};

pub fn allowed(address: &str) -> bool {
    let Ok(url) = url::Url::parse(address) else {
        return false;
    };
    (url.scheme() == "https" || url.scheme() == "http" && url.host_str() == Some("127.0.0.1"))
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some()
}

/// I retain one 64 KiB buffer, never the episode body. Errors omit publisher URLs.
pub fn download(
    address: &str,
    output: &mut impl Write,
    limit: u64,
    cancelled: impl Fn() -> bool,
) -> Result<u64, &'static str> {
    if !allowed(address) {
        return Err(chilpa_i18n::mark(
            "This episode does not have a supported public audio file.",
        ));
    }
    if cancelled() {
        return Err(chilpa_i18n::mark("Episode download was cancelled."));
    }
    let local = address.starts_with("http://127.0.0.1");
    let agent = crate::network::agent(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(180)))
            .timeout_resolve(Some(Duration::from_secs(10)))
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(10)))
            .timeout_recv_body(Some(Duration::from_secs(5)))
            .https_only(!local)
            .max_redirects(5)
            .http_status_as_error(false)
            .middleware(crate::network::guard)
            .build(),
    );
    let mut response = agent
        .get(address)
        .header("Accept", "audio/*")
        .header("User-Agent", "Chilpa/1")
        .call()
        .map_err(|_| chilpa_i18n::mark("The episode audio could not be downloaded."))?;
    if response.status() != 200 {
        return Err(chilpa_i18n::mark(
            "The publisher did not return the episode audio.",
        ));
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.starts_with("audio/") && !content_type.starts_with("application/octet-stream")
    {
        return Err(chilpa_i18n::mark(
            "The publisher did not return an audio file.",
        ));
    }
    let expected = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if expected.is_some_and(|n| n > limit) {
        return Err(chilpa_i18n::mark("The episode exceeds the download limit."));
    }
    let mut reader = response.body_mut().as_reader();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        if cancelled() {
            return Err(chilpa_i18n::mark("Episode download was cancelled."));
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|_| chilpa_i18n::mark("The episode download was interrupted."))?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > limit {
            return Err(chilpa_i18n::mark("The episode exceeds the download limit."));
        }
        output
            .write_all(&buffer[..count])
            .map_err(|_| chilpa_i18n::mark("The episode could not be saved temporarily."))?;
    }
    if total == 0 || expected.is_some_and(|n| n != total) {
        return Err(chilpa_i18n::mark("The episode download is incomplete."));
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(headers: &str, body: &[u8]) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/episode.mp3", listener.local_addr().unwrap());
        let reply = format!("HTTP/1.1 200 OK\r\n{headers}Connection: close\r\n\r\n").into_bytes();
        let body = body.to_vec();
        let task = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = peer.read(&mut request);
            let _ = peer.write_all(&reply);
            let _ = peer.write_all(&body);
        });
        (address, task)
    }
    #[test]
    fn download_streams_audio_and_refuses_oversize_html_and_truncated_replies() {
        for (headers, body, limit, success) in [
            (
                "Content-Type: audio/mpeg\r\nContent-Length: 4\r\n",
                &b"test"[..],
                4,
                true,
            ),
            ("Content-Type: audio/mpeg\r\n", &b"oversize"[..], 4, false),
            ("Content-Type: text/html\r\n", &b"html"[..], 20, false),
            (
                "Content-Type: audio/mpeg\r\nContent-Length: 8\r\n",
                &b"test"[..],
                20,
                false,
            ),
        ] {
            let (address, task) = response(headers, body);
            let mut output = Vec::new();
            assert_eq!(
                download(&address, &mut output, limit, || false).is_ok(),
                success
            );
            task.join().unwrap();
            if success {
                assert_eq!(output, body);
            }
        }
        assert!(!allowed("https://user:secret@example.invalid/a.mp3"));
        assert!(!allowed("http://example.invalid/a.mp3"));
        assert!(download("https://example.invalid/a.mp3", &mut Vec::new(), 4, || true).is_err());
    }
}
