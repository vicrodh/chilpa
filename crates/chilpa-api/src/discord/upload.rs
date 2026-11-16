//! Fooyin's other way to a cover Discord can show: upload the album's own
//! cover to Litterbox, a public file host that keeps a file for a chosen
//! time (`discordartworkuploader.cpp`). Discord fetches the address that
//! comes back.
//!
//! It is the listener's cover, from this computer, on a public host: it
//! only runs when the listener opts in. The cover is decoded and encoded
//! again as JPEG at quality 85, as Fooyin does, which also drops whatever
//! the file carried besides the picture. Nothing else goes: no name, no
//! path, no e-mail; the file is named by its own hash.

use super::artwork::Outcome;
use sha2::{Digest, Sha256};
use std::time::Duration;

pub(crate) const ENDPOINT: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
/// How long the host may be asked to keep a file, in hours (Fooyin's).
pub(crate) const HOURS: [u32; 4] = [1, 12, 24, 72];
/// What an upload may take (Fooyin's `UploadTimeoutMs`).
const WAIT: Duration = Duration::from_secs(15);
/// An answer is an address: past this it is not one (`MaxResponseBytes`).
const ANSWER_LARGEST: usize = 4096;
/// An address is taken as gone this long before the host drops it, so the
/// cover is sent again in time (`ExpiryMarginSecs`).
const MARGIN: Duration = Duration::from_secs(5 * 60);
/// The longer side of what is sent: Discord draws it far smaller, and it is
/// the largest size the Cover Art Archive gives.
const SIDE: u32 = 1200;
const QUALITY: u8 = 85;

/// An album's cover to upload.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Job {
    /// The cover on disk.
    pub cover: String,
    pub hours: u32,
}

/// Whoever sends the form. Tests send it nowhere.
pub(crate) trait Post {
    /// `Err` is nothing came back. Any status is an answer.
    fn post(&self, url: &str, content_type: &str, body: &[u8], timeout: Duration, largest: usize) -> Result<(u16, Vec<u8>), String>;
}

/// The cover as it is sent: JPEG, at most 1200 pixels on its longer side.
pub(crate) fn jpeg(cover: &str) -> Option<Vec<u8>> {
    crate::covers::with_cover(std::path::Path::new(cover), |picture| {
        let picture = if picture.width() > SIDE || picture.height() > SIDE { picture.resize(SIDE, SIDE, image::imageops::FilterType::Triangle) } else { picture };
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, QUALITY).encode_image(&picture.to_rgb8()).ok()?;
        Some(out)
    })
    .flatten()
}

/// Fooyin's form: what to do, for how long, and the file, named by its
/// hash. `None` for a time the host does not offer.
pub(crate) fn form(jpeg: &[u8], hours: u32) -> Option<(String, Vec<u8>)> {
    if !HOURS.contains(&hours) {
        return None;
    }
    let name: String = Sha256::digest(jpeg).iter().map(|byte| format!("{byte:02x}")).collect();
    let boundary = format!("chilpa-{}", &name[..32]);
    let mut body = Vec::with_capacity(jpeg.len() + 512);
    for (field, value) in [("reqtype", "fileupload".to_string()), ("time", format!("{hours}h"))] {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    body.extend_from_slice(format!("--{boundary}\r\nContent-Type: image/jpeg\r\nContent-Disposition: form-data; name=\"fileToUpload\"; filename=\"{name}.jpg\"\r\n\r\n").as_bytes());
    body.extend_from_slice(jpeg);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Some((format!("multipart/form-data; boundary={boundary}"), body))
}

/// The address the host answered, when it is one: http or https, with a
/// host and nobody's name in it (Fooyin's `isValidEndpoint`).
pub(crate) fn address(answer: &[u8]) -> Option<String> {
    if answer.len() > ANSWER_LARGEST {
        return None;
    }
    let text = std::str::from_utf8(answer).ok()?.trim();
    let url = url::Url::parse(text).ok()?;
    let fine = matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty() && url.password().is_none();
    fine.then(|| url.to_string())
}

/// Uploads the cover, unless its album stopped playing first. A cover that
/// cannot be read or sent is none, as in Fooyin.
pub(crate) fn send(job: &Job, endpoint: &str, http: &dyn Post, wanted: &dyn Fn() -> bool) -> Outcome {
    let Some((content_type, body)) = jpeg(&job.cover).and_then(|jpeg| form(&jpeg, job.hours)) else { return Outcome::Missing };
    if !wanted() {
        return Outcome::Abandoned;
    }
    match http.post(endpoint, &content_type, &body, WAIT, ANSWER_LARGEST) {
        Ok((200..=299, answer)) => match address(&answer) {
            Some(url) => Outcome::Uploaded(url, Duration::from_secs(u64::from(job.hours) * 3600).saturating_sub(MARGIN)),
            None => {
                eprintln!("chilpa-discord: the cover host answered no image address");
                Outcome::Missing
            }
        },
        Ok((status, _)) => {
            eprintln!("chilpa-discord: the cover upload failed, HTTP {status}");
            Outcome::Missing
        }
        Err(why) => {
            eprintln!("chilpa-discord: the cover upload failed: {why}");
            Outcome::Missing
        }
    }
}

/// The network, with Chilpa's User-Agent and nothing else of the listener's.
struct Network;

impl Post for Network {
    fn post(&self, url: &str, content_type: &str, body: &[u8], timeout: Duration, largest: usize) -> Result<(u16, Vec<u8>), String> {
        // Offline mode, and the listener's other network switches.
        chilpa_core::network::check(url)?;
        // This machine, where the tests listen, is the one place plain
        // http is taken.
        let local = url.starts_with("http://127.0.0.1:");
        if !url.starts_with("https://") && !local {
            return Err("the address has to be https".into());
        }
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .https_only(!local)
            .max_redirects(0)
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let reply = agent.post(url)
            .header("User-Agent", chilpa_meta::user_agent(env!("CARGO_PKG_VERSION")))
            .header("Content-Type", content_type)
            .send(body)
            .map_err(|error| error.to_string())?;
        let status = reply.status().as_u16();
        let answer = reply.into_body().with_config().limit(largest as u64).read_to_vec().map_err(|error| error.to_string())?;
        Ok((status, answer))
    }
}

/// [`send`] to Litterbox.
pub(crate) fn send_online(job: &Job, wanted: &dyn Fn() -> bool) -> Outcome {
    send(job, ENDPOINT, &Network, wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::Mutex;

    fn cover(folder: &std::path::Path, width: u32, height: u32) -> String {
        let path = folder.join("cover.png");
        image::RgbImage::from_pixel(width, height, image::Rgb([200, 30, 90])).save(&path).unwrap();
        path.display().to_string()
    }

    /// Answers as it was told, and keeps what it was sent.
    struct Host {
        answer: Result<(u16, Vec<u8>), String>,
        sent: Mutex<Vec<(String, String, Vec<u8>)>>,
    }

    impl Post for Host {
        fn post(&self, url: &str, content_type: &str, body: &[u8], _: Duration, _: usize) -> Result<(u16, Vec<u8>), String> {
            self.sent.lock().unwrap().push((url.into(), content_type.into(), body.to_vec()));
            self.answer.clone()
        }
    }

    fn host(answer: Result<(u16, &str), &str>) -> Host {
        Host { answer: answer.map(|(status, text)| (status, text.as_bytes().to_vec())).map_err(str::to_string), sent: Mutex::default() }
    }

    #[test]
    fn a_cover_is_sent_as_a_jpeg_of_at_most_1200_pixels() {
        let folder = tempfile::tempdir().unwrap();
        let bytes = jpeg(&cover(folder.path(), 2400, 1200)).unwrap();
        let sent = image::load_from_memory(&bytes).unwrap();
        assert_eq!(image::guess_format(&bytes).unwrap(), image::ImageFormat::Jpeg);
        assert_eq!((sent.width(), sent.height()), (1200, 600));
        let small = jpeg(&cover(folder.path(), 300, 300)).map(|bytes| image::load_from_memory(&bytes).unwrap());
        assert_eq!(small.map(|small| small.width()), Some(300), "a small cover keeps its size");
        assert_eq!(jpeg(&folder.path().join("none.jpg").display().to_string()), None);
    }

    #[test]
    fn the_form_names_the_file_by_its_hash_only() {
        let (content_type, body) = form(b"jpeg bytes", 24).unwrap();
        let boundary = content_type.strip_prefix("multipart/form-data; boundary=").unwrap();
        let body = String::from_utf8(body).unwrap();
        let name: String = Sha256::digest(b"jpeg bytes").iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(body, format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"reqtype\"\r\n\r\nfileupload\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"time\"\r\n\r\n24h\r\n\
             --{boundary}\r\nContent-Type: image/jpeg\r\nContent-Disposition: form-data; name=\"fileToUpload\"; filename=\"{name}.jpg\"\r\n\r\n\
             jpeg bytes\r\n--{boundary}--\r\n"));
        for hours in HOURS {
            assert!(form(b"x", hours).is_some());
        }
        assert_eq!(form(b"x", 48), None, "only the times the host offers");
    }

    #[test]
    fn only_an_address_is_taken_as_the_answer() {
        assert_eq!(address(b"https://litter.catbox.moe/abc123.jpg\n").as_deref(), Some("https://litter.catbox.moe/abc123.jpg"));
        for refused in [&b"Error: file too large"[..], b"ftp://host/x.jpg", b"https://someone:secret@host/x.jpg", b"https://", b"\xff\xfe"] {
            assert_eq!(address(refused), None, "{}", String::from_utf8_lossy(refused));
        }
        assert_eq!(address(format!("https://litter.catbox.moe/{}", "a".repeat(ANSWER_LARGEST)).as_bytes()), None);
    }

    #[test]
    fn an_uploaded_cover_is_kept_until_five_minutes_before_it_goes() {
        let folder = tempfile::tempdir().unwrap();
        let job = Job { cover: cover(folder.path(), 64, 64), hours: 1 };
        let site = host(Ok((200, "https://litter.catbox.moe/abc123.jpg")));
        assert_eq!(send(&job, ENDPOINT, &site, &|| true), Outcome::Uploaded("https://litter.catbox.moe/abc123.jpg".into(), Duration::from_secs(55 * 60)));
        let sent = site.sent.lock().unwrap();
        assert_eq!(sent[0].0, ENDPOINT);
        assert!(!String::from_utf8_lossy(&sent[0].2).contains(&job.cover), "the cover's path does not go");
    }

    #[test]
    fn a_refused_or_silent_upload_is_no_cover_and_a_left_album_sends_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let job = Job { cover: cover(folder.path(), 64, 64), hours: 24 };
        for answer in [Ok((413, "too large")), Ok((200, "no address")), Err("timed out")] {
            assert_eq!(send(&job, ENDPOINT, &host(answer), &|| true), Outcome::Missing);
        }
        let site = host(Ok((200, "https://litter.catbox.moe/abc123.jpg")));
        assert_eq!(send(&job, ENDPOINT, &site, &|| false), Outcome::Abandoned);
        assert!(site.sent.lock().unwrap().is_empty());
        let missing = Job { cover: folder.path().join("gone.jpg").display().to_string(), hours: 24 };
        assert_eq!(send(&missing, ENDPOINT, &site, &|| true), Outcome::Missing);
    }

    #[test]
    fn the_form_goes_over_http_with_the_user_agent_only() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/resources/internals/api.php", server.local_addr().unwrap().port());
        let heard = std::thread::spawn(move || {
            let (mut peer, _) = server.accept().unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0u8; 4096];
            let length = loop {
                let n = peer.read(&mut chunk).unwrap();
                seen.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&seen).to_lowercase();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length: usize = text.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().trim().parse().unwrap();
                    break end + 4 + length;
                }
            };
            while seen.len() < length {
                let n = peer.read(&mut chunk).unwrap();
                seen.extend_from_slice(&chunk[..n]);
            }
            let answer = "https://litter.catbox.moe/abc123.jpg";
            write!(peer, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len()).unwrap();
            String::from_utf8_lossy(&seen).into_owned()
        });
        let (status, answer) = Network.post(&url, "multipart/form-data; boundary=b", b"--b--\r\n", Duration::from_secs(5), ANSWER_LARGEST).unwrap();
        assert_eq!((status, address(&answer).as_deref()), (200, Some("https://litter.catbox.moe/abc123.jpg")));
        let request = heard.join().unwrap();
        let lower = request.to_lowercase();
        assert!(request.starts_with("POST /resources/internals/api.php HTTP/1.1"));
        assert!(lower.contains(&format!("user-agent: chilpa/{} ( https://chilpa.app )", env!("CARGO_PKG_VERSION"))), "{request}");
        assert!(lower.contains("content-type: multipart/form-data; boundary=b"));
        assert!(!lower.contains("cookie:") && !lower.contains("authorization:"));
        assert!(Network.post("http://example.org/x", "a/b", b"", Duration::from_secs(1), 16).is_err(), "plain http goes nowhere else");
    }
}
