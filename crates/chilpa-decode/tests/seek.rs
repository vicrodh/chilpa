//! Buscar deja la lectura en el frame exacto, no cerca.
//!
//! Se escribe un WAV cuya muestra número N vale N, así que basta mirar la
//! primera muestra que sale después de buscar para saber si quedó donde se
//! pidió.

use chilpa_decode::{Block, Decoded};
use std::io::Write;
use std::path::PathBuf;

const RATE: u32 = 44_100;

fn escribir_rampa(path: &PathBuf, frames: u32) {
    let data_len = frames * 4; // 2 canales de 16 bits
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&RATE.to_le_bytes()).unwrap();
    f.write_all(&(RATE * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for i in 0..frames {
        // El canal izquierdo lleva el número de frame; el derecho, su negado.
        let v = (i % 30_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&(-v).to_le_bytes()).unwrap();
    }
}

/// Lee bloques, tira los frames que el plan indica y regresa la primera
/// muestra de música que queda.
fn primera_muestra(decoded: &mut Decoded, mut descartar: u64) -> i32 {
    loop {
        let bloque = decoded.next_block().unwrap().expect("hay música");
        let Block::Int { data, .. } = bloque else { panic!("el WAV es entero") };
        let frames = (data.len() / 2) as u64;
        if descartar >= frames {
            descartar -= frames;
            continue;
        }
        return data[(descartar as usize) * 2];
    }
}

fn temporal(nombre: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-seek-{nombre}.wav"));
    p
}

#[test]
fn buscar_deja_la_lectura_en_el_frame_pedido() {
    let path = temporal("rampa");
    escribir_rampa(&path, 60_000);
    let mut decoded = Decoded::open(&path).unwrap();

    for objetivo in [0u64, 1, 44_100, 12_345, 59_999] {
        let plan = decoded.seek_to_frame(objetivo).unwrap();
        assert!(plan.request <= objetivo, "el contenedor nunca se pasa del objetivo");
        let muestra = primera_muestra(&mut decoded, plan.discard);
        assert_eq!(
            muestra,
            (objetivo % 30_000) as i32,
            "tras buscar a {objetivo} la música empieza en ese frame"
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn el_archivo_declara_su_duracion_en_frames() {
    let path = temporal("duracion");
    escribir_rampa(&path, 1_000);
    let decoded = Decoded::open(&path).unwrap();
    assert_eq!(decoded.frame_count(), Some(1_000));
    assert_eq!(decoded.encoder_delay(), 0, "un WAV no trae relleno de codificador");
    assert_eq!(decoded.encoder_padding(), 0);
    let _ = std::fs::remove_file(path);
}
