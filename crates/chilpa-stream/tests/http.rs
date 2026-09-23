//! Abrir un stream de verdad, contra un servidor local.
//!
//! Sin red de por medio: el servidor vive en la misma prueba, así que lo que
//! se comprueba es el plomería (encabezados, metadata ICY, tipo de
//! contenido), no la suerte que se tenga con una estación.

use chilpa_stream::{open, StreamError};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

/// Un servidor de un solo cliente que contesta lo que se le diga.
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
        b"datos de audio",
    ));
    let stream = open(&url).unwrap();
    assert_eq!(stream.info().name.as_deref(), Some("Radio Chilpa"));
    assert_eq!(stream.info().extension_hint().as_deref(), Some("mp3"));
}

#[test]
fn the_icy_metadata_comes_out_of_the_audio_and_leaves_the_title() {
    // 16 bytes de audio, el bloque con el título, y 16 más.
    let mut body = vec![7u8; 16];
    let text = b"StreamTitle='Radio Chilpa - Una';";
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
    let titulos = stream.now_playing();

    let mut audio = Vec::new();
    stream.read_to_end(&mut audio).unwrap();

    assert_eq!(audio, [[7u8; 16], [9u8; 16]].concat(), "el audio salió sin la metadata");
    assert_eq!(titulos.get().as_deref(), Some("Radio Chilpa - Una"));
}

#[test]
fn a_web_page_is_not_a_station() {
    let url = server(reply(&[("content-type", "text/html")], b"<html>"));
    match open(&url) {
        Err(StreamError::NotAudio(kind)) => assert!(kind.contains("html")),
        Err(other) => panic!("debía rechazarse por no ser audio: {other}"),
        Ok(_) => panic!("una página web no se abre como estación"),
    }
}

#[test]
fn a_dead_address_is_reported_without_crashing() {
    // Puerto cerrado: nadie contesta.
    match open("http://127.0.0.1:1/stream") {
        Err(StreamError::Open(_)) => {}
        Err(other) => panic!("el error debía ser de apertura: {other}"),
        Ok(_) => panic!("no hay nadie escuchando en ese puerto"),
    }
}
