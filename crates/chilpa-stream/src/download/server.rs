//! Authenticated original files use the same TLS and redirect policy as playback.

use std::{
    io::{Read, Write},
    time::Duration,
};

pub struct Received {
    pub bytes: u64,
    pub content_type: String,
    pub validator: String,
}

/// One transfer retains a 64 KiB buffer. Credentials never leave the
/// requested origin, and error messages never contain the request URL.
pub fn download(
    request: &crate::RangedRequest,
    output: &mut impl Write,
    limit: u64,
    cancelled: impl Fn() -> bool,
    progress: impl Fn(u64),
) -> Result<Received, String> {
    crate::network::check(&request.url)?;
    let url = url::Url::parse(&request.url)
        .map_err(|_| chilpa_i18n::t("that is not a server address"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(chilpa_i18n::t("that is not a server address"));
    }
    let stopped = || cancelled() || crate::network::current().offline;
    if stopped() {
        return Err(chilpa_i18n::t("Download cancelled."));
    }
    let agent = crate::server::agent(Duration::from_secs(20), Duration::from_secs(5));
    let mut builder = agent.get(&request.url);
    for (key, value) in &request.headers {
        builder = builder.header(key, value);
    }
    let mut response = builder
        .header("Accept-Encoding", "identity")
        .call()
        .map_err(|error| crate::server::describe(&error))?;
    if response.status() != 200 {
        return Err(chilpa_i18n::t_args(
            "the server answered {}",
            &[&response.status().as_u16().to_string()],
        ));
    }
    let headers = response.headers();
    let content_type = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let kind = content_type.split(';').next().unwrap_or("").trim();
    if !(kind.starts_with("audio/")
        || matches!(
            kind,
            "application/octet-stream" | "binary/octet-stream" | "application/ogg" | "video/mp4"
        ))
    {
        return Err(chilpa_i18n::t(
            "the server answered with a message instead of the file",
        ));
    }
    let expected = headers
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|bytes| *bytes > 0)
        .ok_or_else(|| chilpa_i18n::t("the server did not say how long the file is"))?;
    if expected > limit {
        return Err(chilpa_i18n::t(
            "The offline storage limit has been reached.",
        ));
    }
    let validator = headers
        .get("etag")
        .or_else(|| headers.get("last-modified"))
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 1024)
        .unwrap_or("")
        .to_string();
    let mut reader = response.body_mut().as_reader();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        if stopped() {
            return Err(chilpa_i18n::t("Download cancelled."));
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|_| chilpa_i18n::t("The download was interrupted. Try again."))?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > limit || total > expected {
            return Err(chilpa_i18n::t(
                "The offline storage limit has been reached.",
            ));
        }
        output
            .write_all(&buffer[..count])
            .map_err(|_| chilpa_i18n::t("The offline file could not be written."))?;
        progress(total);
    }
    if total != expected {
        return Err(chilpa_i18n::t("The download is incomplete. Try again."));
    }
    Ok(Received {
        bytes: total,
        content_type,
        validator,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply(response: &str) -> (crate::RangedRequest, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/audio?credential=fixture",
            listener.local_addr().unwrap()
        );
        let response = response.to_string();
        let worker = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let count = peer.read(&mut buffer).unwrap();
            peer.write_all(response.as_bytes()).unwrap();
            String::from_utf8_lossy(&buffer[..count]).to_string()
        });
        (
            crate::RangedRequest {
                url,
                headers: vec![("Authorization".into(), "Bearer fixture".into())],
            },
            worker,
        )
    }
    #[test]
    fn authenticated_downloads_are_bounded_complete_and_never_follow_redirects() {
        for (response,success) in [
            ("HTTP/1.1 200 OK\r\nContent-Type: audio/flac\r\nContent-Length: 4\r\nConnection: close\r\n\r\nfLaC",true),
            ("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",false),
            ("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 4\r\nConnection: close\r\n\r\n{}xx",false),
            ("HTTP/1.1 200 OK\r\nContent-Type: audio/flac\r\nContent-Length: 8\r\nConnection: close\r\n\r\nfLaC",false),
            ("HTTP/1.1 200 OK\r\nContent-Type: audio/flac\r\nContent-Length: 99\r\nConnection: close\r\n\r\nfLaC",false),
        ] {
            let (request,worker)=reply(response);
            let mut bytes=Vec::new();
            let result=download(&request,&mut bytes,16,|| false,|_| {});
            assert_eq!(result.is_ok(),success);
            assert!(worker.join().unwrap().to_ascii_lowercase().contains("authorization: bearer fixture"));
            if success { assert_eq!(bytes,b"fLaC"); }
            else { assert!(!result.err().unwrap().contains("credential")); }
        }
    }
}
