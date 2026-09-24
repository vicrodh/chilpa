//! De un resultado de un source a música saliendo por el dispositivo.
//!
//! Es el camino completo: el plugin contesta la búsqueda, el usuario hace
//! clic, Chilpa le pide `chilpa_resolver`, el plugin da una URL, y esa URL
//! se abre y se reproduce como cualquier otro stream.
//!
//! El servidor es local y de juguete: lo que se prueba es la costura, no
//! HTTP. Y el plugin **no pide permiso de red**, a propósito: resolver no
//! necesita que el guest salga a internet, porque quien abre la URL es
//! Chilpa.
//!
//! Es una sola prueba porque apunta `XDG_DATA_HOME` a un directorio
//! temporal, y esa variable es del proceso entero.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;

const OUTPUT: &str = "capture";

/// Un WAV corto con un tono, en memoria.
fn wave(frames: u32) -> Vec<u8> {
    let data = frames * 4;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&44_100u32.to_le_bytes());
    w.extend_from_slice(&176_400u32.to_le_bytes());
    w.extend_from_slice(&4u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        let v = (i % 3_000) as i16;
        w.extend_from_slice(&v.to_le_bytes());
        w.extend_from_slice(&v.to_le_bytes());
    }
    w
}

/// Sirve el mismo WAV a quien se lo pida, una vez por conexión.
fn serve(audio: Vec<u8>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\n\r\n",
                audio.len()
            );
            if stream.write_all(head.as_bytes()).is_ok() {
                let _ = stream.write_all(&audio);
            }
            let _ = stream.flush();
        }
    });
    port
}

/// Un source que encuentra una pista y sabe decir con qué se reproduce.
fn install(dir: &Path, id: &str, found: &str, stream: &str) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(
        dir.join(id).join("manifest.json"),
        format!(
            r#"{{"id": "{id}", "name": "Source de prueba", "version": "0.1.0",
                 "api_version": 1, "kind": "source", "capabilities": []}}"#
        ),
    )
    .unwrap();

    // Dos datos en memoria, cada uno con su dirección y su largo sacados
    // aquí. Contarlos a mano es cómo una prueba pasa por la razón
    // equivocada.
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let wat = format!(
        r#"(module
            (memory (export "memory") 1)
            (global $next (mut i32) (i32.const 8192))
            (func (export "chilpa_alloc") (param i32) (result i32)
              (local i32)
              (local.set 1 (global.get $next))
              (global.set $next (i32.add (global.get $next) (local.get 0)))
              (local.get 1))
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (data (i32.const 16) "{found_esc}")
            (data (i32.const 2048) "{stream_esc}")
            (func (export "chilpa_search") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 16) (i64.const 32)) (i64.const {found_len})))
            ;; No mira el id que le mandan: solo tiene una pista.
            (func (export "chilpa_resolver") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 2048) (i64.const 32)) (i64.const {stream_len}))))"#,
        found_esc = esc(found),
        found_len = found.len(),
        stream_esc = esc(stream),
        stream_len = stream.len(),
    );
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(&wat).expect("compila"))
        .unwrap();
}

#[test]
fn a_track_from_a_source_resolves_and_plays() {
    let dir = std::env::temp_dir().join("chilpa-api-resolve");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));

    let port = serve(wave(44_100));
    let root = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&root).unwrap();
    install(
        &root,
        "com.prueba.fuente",
        r#"{"tracks":[{"external_id":"a1","title":"Una de un source","artist":"Alguien"}]}"#,
        &format!(r#"{{"url":"http://127.0.0.1:{port}/track.wav","codec":"wav"}}"#),
    );

    // Y uno que contesta una dirección que no es http, para ver que no se
    // intenta abrir.
    install(
        &root,
        "com.prueba.mentiroso",
        r#"{"tracks":[{"external_id":"b1","title":"Una que miente","artist":"Alguien"}]}"#,
        r#"{"url":"file:///etc/passwd"}"#,
    );

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.load_plugins();
    chilpa.approve_plugin("com.prueba.fuente");
    assert_eq!(chilpa.approve_plugin("com.prueba.mentiroso"), 2, "los dos corriendo");

    // Buscar: lo local está vacío, así que el único resultado es del source.
    chilpa.search_all("una", 20);
    assert!(wait_for(&mut chilpa, |c| c.found().len() >= 2), "los dos contestaron");
    let hits = chilpa.found().to_vec();
    assert_eq!(hits.len(), 2);
    let good = hits.iter().position(|h| h.source == "com.prueba.fuente").unwrap();
    let liar = hits.iter().position(|h| h.source == "com.prueba.mentiroso").unwrap();
    assert_eq!(hits[good].title, "Una de un source");
    assert!(hits[good].playable, "ahora sí se puede intentar");

    // El que miente: no se intenta abrir, y se dice por qué.
    assert!(!chilpa.play_hit(liar, OUTPUT, true), "no se abre un file://");
    let e = chilpa.now_playing().error.clone();
    assert!(e.contains("http"), "y se dice por qué: {e}");
    assert!(e.starts_with("Source de prueba"), "con el nombre del source: {e}");

    // Y el bueno: un clic, se resuelve y suena.
    assert!(chilpa.play_hit(good, OUTPUT, true), "{}", chilpa.now_playing().error);
    assert!(
        wait_for(&mut chilpa, |c| c.now_playing().seconds > 0.0 || !c.now_playing().playing),
        "arrancó la reproducción"
    );
    let now = chilpa.now_playing().clone();
    assert!(now.error.is_empty(), "sonó sin quejarse: {}", now.error);
    // La ventana muestra la ficha del source, no la URL a la que resolvió:
    // el decodificador de un stream no sabe más que eso.
    assert_eq!(now.title, "Una de un source", "se ve el título, no la URL");
    assert_eq!(now.artist, "Alguien");
    assert!(now.total_seconds > 0.0 || now.seconds > 0.0, "algo salió por el dispositivo: {now:?}");

    let _ = std::fs::remove_dir_all(dir);
}

fn wait_for(
    chilpa: &mut chilpa_api::Chilpa,
    mut cond: impl FnMut(&mut chilpa_api::Chilpa) -> bool,
) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        chilpa.poll();
        if cond(chilpa) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}
