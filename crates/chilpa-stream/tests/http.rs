//! Opening a real stream, against a local server.
//!
//! With no network in between: the server lives in the same test, so what is
//! checked is the plumbing (headers, ICY metadata, content type), not the
//! luck one has with a station.

use chilpa_stream::{open, StreamError};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

/// A single client server that answers whatever it is told to.
fn server(reply: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut socket, _)) = listener.accept() {
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request);
            let _ = socket.write_all(&reply);
        }
    });
    format!("http://127.0.0.1:{port}/stream")
}

fn reply(headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut output = b"HTTP/1.1 200 OK\r\n".to_vec();
    for (k, v) in headers {
        output.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    output.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    output.extend_from_slice(body);
    output
}

#[test]
fn the_station_name_and_its_format_are_read() {
    let url = server(reply(
        &[("icy-name", "Radio Chilpa"), ("content-type", "audio/mpeg")],
        b"audio data",
    ));
    let stream = open(&url).unwrap();
    assert_eq!(stream.info().name.as_deref(), Some("Radio Chilpa"));
    assert_eq!(stream.info().extension_hint().as_deref(), Some("mp3"));
}

#[test]
fn the_icy_metadata_comes_out_of_the_audio_and_leaves_the_title() {
    // 16 bytes of audio, the block with the title, and 16 more.
    let mut body = vec![7u8; 16];
    let text = b"StreamTitle='Radio Chilpa - One';";
    let mut block = text.to_vec();
    while block.len() % 16 != 0 {
        block.push(0);
    }
    body.push((block.len() / 16) as u8);
    body.extend_from_slice(&block);
    body.extend_from_slice(&[9u8; 16]);

    let url = server(reply(
        &[("content-type", "audio/mpeg"), ("icy-metaint", "16")],
        &body,
    ));
    let mut stream = open(&url).unwrap();
    let titles = stream.now_playing();

    let mut audio = Vec::new();
    stream.read_to_end(&mut audio).unwrap();

    assert_eq!(audio, [[7u8; 16], [9u8; 16]].concat(), "the audio came out without the metadata");
    assert_eq!(titles.get().as_deref(), Some("Radio Chilpa - One"));
}

#[test]
fn a_web_page_is_not_a_station() {
    let url = server(reply(&[("content-type", "text/html")], b"<html>"));
    match open(&url) {
        Err(StreamError::NotAudio(kind)) => assert!(kind.contains("html")),
        Err(other) => panic!("it should have been refused for not being audio: {other}"),
        Ok(_) => panic!("a web page does not open as a station"),
    }
}

#[test]
fn a_dead_address_is_reported_without_crashing() {
    // Closed port: nobody answers.
    match open("http://127.0.0.1:1/stream") {
        Err(StreamError::Open(_)) => {}
        Err(other) => panic!("the error should have been an open one: {other}"),
        Ok(_) => panic!("nobody is listening on that port"),
    }
}

/// A server that answers several requests, each with its own body, in order.
///
/// One connection per part, because that is what opening a URL does: the
/// chained reader opens the next part only when the current one runs out.
fn server_in_parts(parts: Vec<Vec<u8>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for part in parts {
            let Ok((mut socket, _)) = listener.accept() else { return };
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request);
            let _ = socket.write_all(&reply(&[("content-type", "audio/flac")], &part));
        }
    });
    format!("http://127.0.0.1:{port}/part")
}

#[test]
fn a_track_in_several_parts_reads_as_one() {
    // Three parts of one file. What the decoder must see is the bytes in
    // order, with no seam it can notice.
    let parts: Vec<Vec<u8>> =
        vec![(0u8..80).collect(), (80u8..160).collect(), (160u8..240).collect()];
    let url = server_in_parts(parts);
    let urls = vec![url.clone(), url.clone(), url];

    let mut stream = chilpa_stream::open_parts(&urls).unwrap();
    assert_eq!(stream.info().extension_hint().as_deref(), Some("flac"));

    let mut read = Vec::new();
    stream.read_to_end(&mut read).unwrap();
    let expected: Vec<u8> = (0u8..240).collect();
    assert_eq!(read, expected, "the parts came out in order and whole");
}

#[test]
fn one_part_is_the_same_as_opening_it() {
    let url = server(reply(&[("content-type", "audio/flac")], b"only one"));
    let mut stream = chilpa_stream::open_parts(&[url]).unwrap();
    let mut read = Vec::new();
    stream.read_to_end(&mut read).unwrap();
    assert_eq!(read, b"only one");
}

#[test]
fn no_parts_is_the_callers_mistake_and_says_so() {
    let Err(e) = chilpa_stream::open_parts(&[]) else { panic!("it should have refused") };
    assert!(matches!(e, StreamError::Open(_)), "{e}");
}

#[test]
fn a_part_that_does_not_open_fails_instead_of_being_skipped() {
    // Half a track and silence is a symptom someone can report; a track
    // that quietly loses its second half is not.
    let good = server(reply(&[("content-type", "audio/flac")], b"first half"));
    let dead = "http://127.0.0.1:1/gone".to_string();
    let mut stream = chilpa_stream::open_parts(&[good, dead]).unwrap();
    let mut read = Vec::new();
    let problem = stream.read_to_end(&mut read).unwrap_err();
    assert_eq!(read, b"first half", "what did arrive is not thrown away");
    assert!(problem.to_string().contains("part"), "{problem}");
}
