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
fn servidor(respuesta: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let puerto = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut socket, _)) = listener.accept() {
            let mut peticion = [0u8; 1024];
            let _ = socket.read(&mut peticion);
            let _ = socket.write_all(&respuesta);
        }
    });
    format!("http://127.0.0.1:{puerto}/stream")
}

fn respuesta(encabezados: &[(&str, &str)], cuerpo: &[u8]) -> Vec<u8> {
    let mut salida = b"HTTP/1.1 200 OK\r\n".to_vec();
    for (k, v) in encabezados {
        salida.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    salida.extend_from_slice(format!("Content-Length: {}\r\n\r\n", cuerpo.len()).as_bytes());
    salida.extend_from_slice(cuerpo);
    salida
}

#[test]
fn se_lee_el_nombre_de_la_estacion_y_su_formato() {
    let url = servidor(respuesta(
        &[("icy-name", "Radio Chilpa"), ("content-type", "audio/mpeg")],
        b"datos de audio",
    ));
    let stream = open(&url).unwrap();
    assert_eq!(stream.info().name.as_deref(), Some("Radio Chilpa"));
    assert_eq!(stream.info().extension_hint().as_deref(), Some("mp3"));
}

#[test]
fn la_metadata_icy_sale_del_audio_y_deja_el_titulo() {
    // 16 bytes de audio, el bloque con el título, y 16 más.
    let mut cuerpo = vec![7u8; 16];
    let texto = b"StreamTitle='Radio Chilpa - Una';";
    let mut bloque = texto.to_vec();
    while bloque.len() % 16 != 0 {
        bloque.push(0);
    }
    cuerpo.push((bloque.len() / 16) as u8);
    cuerpo.extend_from_slice(&bloque);
    cuerpo.extend_from_slice(&[9u8; 16]);

    let url = servidor(respuesta(
        &[("content-type", "audio/mpeg"), ("icy-metaint", "16")],
        &cuerpo,
    ));
    let mut stream = open(&url).unwrap();
    let titulos = stream.now_playing();

    let mut audio = Vec::new();
    stream.read_to_end(&mut audio).unwrap();

    assert_eq!(audio, [[7u8; 16], [9u8; 16]].concat(), "el audio salió sin la metadata");
    assert_eq!(titulos.get().as_deref(), Some("Radio Chilpa - Una"));
}

#[test]
fn una_pagina_web_no_es_una_estacion() {
    let url = servidor(respuesta(&[("content-type", "text/html")], b"<html>"));
    match open(&url) {
        Err(StreamError::NotAudio(tipo)) => assert!(tipo.contains("html")),
        Err(otro) => panic!("debía rechazarse por no ser audio: {otro}"),
        Ok(_) => panic!("una página web no se abre como estación"),
    }
}

#[test]
fn una_direccion_muerta_se_reporta_sin_tronar() {
    // Puerto cerrado: nadie contesta.
    match open("http://127.0.0.1:1/stream") {
        Err(StreamError::Open(_)) => {}
        Err(otro) => panic!("el error debía ser de apertura: {otro}"),
        Ok(_) => panic!("no hay nadie escuchando en ese puerto"),
    }
}
