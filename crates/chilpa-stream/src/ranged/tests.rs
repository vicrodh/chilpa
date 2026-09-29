use super::*;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// How a test server answers.
#[derive(Clone, Copy, PartialEq)]
enum Serves {
    /// `Range`, as Navidrome, Jellyfin's static stream and Plex do.
    Ranges,
    /// The whole file every time, with its length.
    Whole,
    /// No length: a conversion on the fly.
    Chunked,
}

struct Server {
    url: String,
    asked: Arc<Mutex<Vec<String>>>,
    requests: Arc<AtomicUsize>,
}

fn serve(file: Vec<u8>, content_type: &'static str, how: Serves) -> Server {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/rest/stream.view?id=1&t=secret", listener.local_addr().unwrap().port());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(AtomicUsize::new(0));
    let (log, count) = (Arc::clone(&asked), Arc::clone(&requests));
    std::thread::spawn(move || {
        for peer in listener.incoming() {
            let Ok(mut peer) = peer else { break };
            let mut range = None;
            let mut reader = BufReader::new(peer.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" { break; }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                    let (a, b) = value.trim().split_once('-').unwrap();
                    range = Some((a.parse::<usize>().unwrap(), b.parse::<usize>().unwrap()));
                    log.lock().unwrap().push(value.trim().to_string());
                }
            }
            count.fetch_add(1, Ordering::Relaxed);
            let reply = match (how, range) {
                (Serves::Ranges, Some((a, b))) => {
                    let b = b.min(file.len() - 1);
                    let mut head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Type: {content_type}\r\nContent-Range: bytes {a}-{b}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        file.len(), b + 1 - a
                    ).into_bytes();
                    head.extend_from_slice(&file[a..=b]);
                    head
                }
                (Serves::Chunked, _) => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\nabcd\r\n0\r\n\r\n"
                ).into_bytes(),
                _ => {
                    let mut head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        file.len()
                    ).into_bytes();
                    head.extend_from_slice(&file);
                    head
                }
            };
            let _ = peer.write_all(&reply);
        }
    });
    Server { url, asked, requests }
}

fn file(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

fn open(server: &Server) -> Result<Ranged, RangedError> {
    Ranged::open(RangedRequest { url: server.url.clone(), headers: vec![("X-Test".into(), "1".into())] })
}

#[test]
fn a_file_is_read_whole_across_windows_byte_for_byte() {
    let original = file(2 * WINDOW as usize + 12_345);
    let server = serve(original.clone(), "audio/flac", Serves::Ranges);
    let mut ranged = open(&server).unwrap();
    assert_eq!(ranged.len(), original.len() as u64);
    assert_eq!(ranged.content_type(), Some("audio/flac"));

    let mut read = Vec::new();
    ranged.read_to_end(&mut read).unwrap();

    assert!(read == original, "the original bytes, all of them");
    assert_eq!(server.requests.load(Ordering::Relaxed), 3, "one request per window, none twice");
}

#[test]
fn a_seek_asks_only_for_the_window_it_lands_in() {
    let original = file(3 * WINDOW as usize);
    let server = serve(original.clone(), "audio/flac", Serves::Ranges);
    let mut ranged = open(&server).unwrap();

    let at = 2 * WINDOW + 1000;
    assert_eq!(ranged.seek(SeekFrom::Start(at)).unwrap(), at);
    let mut some = [0u8; 100];
    ranged.read_exact(&mut some).unwrap();
    assert_eq!(&some[..], &original[at as usize..at as usize + 100]);
    assert!(server.asked.lock().unwrap().iter().any(|r| r.starts_with(&format!("{at}-"))), "{:?}", server.asked.lock().unwrap());

    assert_eq!(ranged.seek(SeekFrom::End(-10)).unwrap(), original.len() as u64 - 10);
    let mut tail = Vec::new();
    ranged.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, original[original.len() - 10..]);
}

#[test]
fn a_server_without_ranges_is_read_whole_once() {
    let original = file(WINDOW as usize + 500);
    let server = serve(original.clone(), "audio/mpeg", Serves::Whole);
    let mut ranged = open(&server).unwrap();
    let mut read = Vec::new();
    ranged.read_to_end(&mut read).unwrap();
    ranged.seek(SeekFrom::Start(10)).unwrap();
    let mut again = [0u8; 10];
    ranged.read_exact(&mut again).unwrap();
    assert!(read == original);
    assert_eq!(&again[..], &original[10..20]);
    assert_eq!(server.requests.load(Ordering::Relaxed), 1, "held whole: nothing is asked twice");
}

#[test]
fn a_message_in_place_of_the_file_is_refused_before_it_reaches_a_decoder() {
    for (body, kind) in [
        (&br#"{"subsonic-response":{"status":"failed","error":{"code":70}}}"#[..], "audio/flac"),
        (&b"<?xml version=\"1.0\"?><subsonic-response status=\"failed\"/>"[..], "audio/mpeg"),
        (&b"not found"[..], "text/plain"),
        (&br#"{"error":1}"#[..], "application/json"),
    ] {
        let server = serve(body.to_vec(), kind, Serves::Whole);
        assert!(matches!(open(&server), Err(RangedError::NotAudio(_))), "{kind}");
    }
}

#[test]
fn a_conversion_on_the_fly_is_refused() {
    let server = serve(Vec::new(), "audio/mpeg", Serves::Chunked);
    let Err(RangedError::NotAudio(why)) = open(&server) else { panic!("a stream without a length is not the file") };
    assert!(why.contains("convert"), "{why}");
}

#[test]
fn the_decoder_is_told_it_can_seek_and_how_long_the_file_is() {
    use symphonia::core::io::MediaSource;
    let server = serve(file(1000), "audio/flac", Serves::Ranges);
    let ranged = open(&server).unwrap();
    assert!(ranged.is_seekable());
    assert_eq!(ranged.byte_len(), Some(1000));
}
