//! La cola vista desde la fachada: lo que se encola queda encolado.
//!
//! Volver atrás relanzaba la cola con las pistas que quedaban, y con eso se
//! perdían las de antes: la segunda vez que se pedía "anterior" ya no había
//! a dónde volver, y guardar la lista guardaba media. Aquí se fija que la
//! cola completa sobreviva.

use std::io::Write;
use std::path::PathBuf;

/// Apunta la biblioteca a un directorio temporal.
///
/// Sin esto, la prueba escribe en la biblioteca de quien la corra: le mete
/// escuchas y, lo que de verdad duele, le migra el esquema a la versión del
/// código que se está probando. Eso ya rompió un binario viejo que seguía
/// abierto.
fn other_library(dir: &std::path::Path) {
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));
}

/// La salida de captura escribe a un archivo, así que esto corre sin sonido
/// y sin hardware.
const OUTPUT: &str = "capture";

fn folder(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-api-cola-{name}"));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Un WAV corto, con un tono constante.
fn wave(dir: &PathBuf, name: &str, frames: u32) -> PathBuf {
    let p = dir.join(name);
    let data = frames * 4;
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&44_100u32.to_le_bytes()).unwrap();
    f.write_all(&176_400u32.to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    for i in 0..frames {
        let v = (i % 3_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    p
}

/// Espera hasta que la cola haya llegado por lo menos a esa posición.
///
/// A la salida de captura no la frena un reloj: entrega lo que le dan tan
/// rápido como puede, así que una cola corta puede pasar por varias pistas
/// entre dos lecturas. Por eso se espera "al menos" y no una posición
/// exacta, que sería una prueba que falla según el día.
fn wait_at_least(chilpa: &mut chilpa_api::Chilpa, index: usize) -> bool {
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < limit {
        if chilpa.poll().index >= index {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

#[test]
fn going_back_does_not_trim_the_queue() {
    let dir = folder("atras");
    other_library(&dir);
    // Pistas de medio segundo: alcanza para que la cola avance sola.
    for n in ["01.wav", "02.wav", "03.wav"] {
        wave(&dir, n, 22_050);
    }

    let mut chilpa = chilpa_api::Chilpa::new();
    let titles = chilpa.play_folder(&dir, OUTPUT, true);
    assert_eq!(titles.len(), 3, "se encolaron las tres");

    assert!(wait_at_least(&mut chilpa, 2), "la cola avanzó sola");
    let antes = chilpa.now_playing().index;
    chilpa.previous_track(OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, antes - 1), "volvió a sonar");
    assert_eq!(chilpa.now_playing().queue_len, 3, "la cola sigue completa");

    // Y con la cola completa, guardarla guarda las tres.
    let list = dir.join("cola.m3u");
    assert!(chilpa.save_queue(&list), "se guardó");
    let text = std::fs::read_to_string(&list).unwrap();
    for n in ["01.wav", "02.wav", "03.wav"] {
        assert!(text.contains(n), "falta {n} en:\n{text}");
    }

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shuffle_and_repeat_are_remembered_by_the_facade() {
    let mut chilpa = chilpa_api::Chilpa::new();
    assert!(!chilpa.shuffle(), "arranca en orden");
    assert_eq!(chilpa.repeat(), chilpa_api::Repeat::Off);
    chilpa.set_shuffle(true);
    chilpa.set_repeat(chilpa_api::Repeat::All);
    assert!(chilpa.shuffle());
    assert_eq!(chilpa.repeat(), chilpa_api::Repeat::All);
}

#[test]
fn enqueuing_with_nothing_playing_starts_the_queue() {
    let dir = folder("encolar");
    other_library(&dir);
    let paths: Vec<String> = ["01.wav", "02.wav"]
        .iter()
        .map(|n| wave(&dir, n, 4_410).display().to_string())
        .collect();

    let mut chilpa = chilpa_api::Chilpa::new();
    let titles = chilpa.enqueue(&paths, OUTPUT, true);
    assert_eq!(titles.len(), 2, "encolar con nada sonando es reproducir");
    assert!(wait_at_least(&mut chilpa, 1), "la cola avanzó");
    assert_eq!(chilpa.queue_titles().len(), 2);

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}
