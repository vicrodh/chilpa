//! Streams one published release file into a caller-owned writer, bounded in bytes and time.
use std::{
    io::{Read, Write},
    time::Duration,
};

/// A release file comes from GitHub over HTTPS; loopback HTTP serves the tests.
pub fn allowed(address: &str) -> bool {
    let Ok(url) = url::Url::parse(address) else {
        return false;
    };
    let local = url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    let github = url.scheme() == "https" && url.host_str() == Some("github.com");
    (github || local) && url.username().is_empty() && url.password().is_none()
}

/// The reply must be exactly `size` bytes: a longer or shorter body is an error.
/// `progress` receives the bytes written so far.
pub fn download(
    address: &str,
    output: &mut impl Write,
    size: u64,
    cancelled: impl Fn() -> bool,
    mut progress: impl FnMut(u64),
) -> Result<u64, &'static str> {
    if !allowed(address) {
        return Err("The release address is not allowed.");
    }
    if cancelled() {
        return Err("The release download was cancelled.");
    }
    let local = address.starts_with("http://127.0.0.1");
    let agent = crate::network::agent(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(1800)))
            .timeout_resolve(Some(Duration::from_secs(10)))
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(15)))
            .timeout_recv_body(Some(Duration::from_secs(30)))
            .https_only(!local)
            .max_redirects(5)
            .http_status_as_error(false)
            .middleware(crate::network::guard)
            .build(),
    );
    let mut response = agent
        .get(address)
        .header("User-Agent", "Chilpa/1")
        .call()
        .map_err(|_| "The release download failed.")?;
    if response.status() != 200 {
        return Err("The release download failed.");
    }
    let announced = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if announced.is_some_and(|n| n != size) {
        return Err("The release file differs in size from its listing.");
    }
    let mut reader = response.body_mut().as_reader();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        if cancelled() {
            return Err("The release download was cancelled.");
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|_| "The release download was interrupted.")?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > size {
            return Err("The release file is larger than its listing.");
        }
        output
            .write_all(&buffer[..count])
            .map_err(|_| "The release file could not be saved.")?;
        progress(total);
    }
    if total != size {
        return Err("The release download is incomplete.");
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serve(headers: &str, body: &[u8]) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/Chilpa.zip", listener.local_addr().unwrap());
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
    fn only_github_over_https_or_loopback_is_allowed() {
        for address in [
            "https://github.com/vicrodh/chilpa/releases/download/v1.0.0/Chilpa.zip",
            "http://127.0.0.1:8080/Chilpa.zip",
        ] {
            assert!(allowed(address), "{address}");
        }
        for address in [
            "http://github.com/x",
            "https://github.com.example/x",
            "https://user@github.com/x",
            "https://example.org/x",
            "ftp://github.com/x",
            "not a url",
        ] {
            assert!(!allowed(address), "{address}");
        }
    }

    #[test]
    fn a_reply_of_the_listed_size_is_written_and_reported() {
        let (address, task) = serve("Content-Length: 4\r\n", b"test");
        let mut written = Vec::new();
        let mut seen = 0;
        let total = download(&address, &mut written, 4, || false, |bytes| seen = bytes).unwrap();
        task.join().unwrap();
        assert_eq!((total, seen, written.as_slice()), (4, 4, b"test".as_slice()));
    }

    #[test]
    fn replies_that_differ_from_the_listed_size_are_refused() {
        for (headers, body, size) in [
            ("Content-Length: 8\r\n", &b"oversize"[..], 4),
            ("", &b"oversize"[..], 4),
            ("", &b"ab"[..], 4),
        ] {
            let (address, task) = serve(headers, body);
            assert!(download(&address, &mut Vec::new(), size, || false, |_| ()).is_err(), "{headers:?} {body:?}");
            task.join().unwrap();
        }
    }

    #[test]
    fn a_cancelled_download_does_not_connect() {
        assert!(download("http://127.0.0.1:1/x", &mut Vec::new(), 1, || true, |_| ()).is_err());
    }
}
