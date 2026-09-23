//! Un DSF sintético que se lee entero, con los bits intactos.

use chilpa_audio::dop::DopPacker;
use chilpa_dsd::{DsdSource, DsfReader};
use std::io::Write;
use std::path::PathBuf;

const BLOCK: u32 = 4096;
const RATE: u32 = 2_822_400; // DSD64

/// Escribe un DSF de dos canales con `bloques` bloques por canal.
///
/// El byte n del canal izquierdo vale n, y el del derecho su complemento,
/// así que cualquier corrimiento o mezcla entre canales se nota.
fn write_dsf(path: &PathBuf, blocks: u32, bits_per_sample: u32, samples: u64) {
    let data = u64::from(blocks) * u64::from(BLOCK) * 2;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"DSD ").unwrap();
    f.write_all(&28u64.to_le_bytes()).unwrap();
    f.write_all(&(28u64 + 52 + 12 + data).to_le_bytes()).unwrap();
    f.write_all(&0u64.to_le_bytes()).unwrap(); // sin tags
    f.write_all(b"fmt ").unwrap();
    f.write_all(&52u64.to_le_bytes()).unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap(); // versión
    f.write_all(&0u32.to_le_bytes()).unwrap(); // format id: DSD crudo
    f.write_all(&2u32.to_le_bytes()).unwrap(); // estéreo
    f.write_all(&2u32.to_le_bytes()).unwrap(); // canales
    f.write_all(&RATE.to_le_bytes()).unwrap();
    f.write_all(&bits_per_sample.to_le_bytes()).unwrap();
    f.write_all(&samples.to_le_bytes()).unwrap();
    f.write_all(&BLOCK.to_le_bytes()).unwrap();
    f.write_all(&0u32.to_le_bytes()).unwrap(); // reservado
    f.write_all(b"data").unwrap();
    f.write_all(&(12 + data).to_le_bytes()).unwrap();
    for b in 0..blocks {
        for canal in 0..2u32 {
            for i in 0..BLOCK {
                let v = ((b * BLOCK + i) % 251) as u8;
                f.write_all(&[if canal == 0 { v } else { !v }]).unwrap();
            }
        }
    }
}

fn temporal(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-dsd-{name}.dsf"));
    p
}

#[test]
fn se_lee_el_encabezado_y_todos_los_bloques() {
    let path = temporal("basico");
    let samples = u64::from(BLOCK) * 3 * 8;
    write_dsf(&path, 3, 1, samples);

    let mut r = DsfReader::open(&path).unwrap();
    assert_eq!(r.info().channels, 2);
    assert_eq!(r.info().dsd_rate_hz, RATE);
    assert_eq!(r.info().multiple(), 64, "DSD64");
    assert_eq!(r.info().samples, samples);

    let mut blocks = 0;
    let mut left_bytes = 0;
    while let Some(channels) = r.next_block().unwrap() {
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].len(), channels[1].len());
        left_bytes += channels[0].len();
        blocks += 1;
    }
    assert_eq!(blocks, 3);
    assert_eq!(left_bytes as u64, samples / 8, "salieron todos los bytes válidos");
    let _ = std::fs::remove_file(path);
}

#[test]
fn los_bits_se_entregan_con_el_primero_en_tiempo_arriba() {
    // DSF guarda el bit primero en tiempo abajo; aquí debe salir invertido.
    let path = temporal("orden");
    write_dsf(&path, 1, 1, u64::from(BLOCK) * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let channels = r.next_block().unwrap().unwrap();
    assert_eq!(channels[0][0], 0u8.reverse_bits());
    assert_eq!(channels[0][1], 1u8.reverse_bits());
    assert_eq!(channels[1][1], (!1u8).reverse_bits(), "el canal derecho es el complemento");
    let _ = std::fs::remove_file(path);

    // Con bits_per_sample = 8 el archivo ya viene con el primero arriba y no
    // se toca.
    let path = temporal("orden-msb");
    write_dsf(&path, 1, 8, u64::from(BLOCK) * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let channels = r.next_block().unwrap().unwrap();
    assert_eq!(channels[0][1], 1u8, "sin invertir");
    let _ = std::fs::remove_file(path);
}

#[test]
fn el_relleno_del_ultimo_bloque_no_sale() {
    let path = temporal("relleno");
    // Dos bloques en el archivo, pero solo 100 bytes por canal son música.
    write_dsf(&path, 2, 1, 100 * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let primero = r.next_block().unwrap().unwrap();
    assert_eq!(primero[0].len(), 100, "solo lo válido");
    assert!(r.next_block().unwrap().is_none(), "el resto era relleno");
    let _ = std::fs::remove_file(path);
}

#[test]
fn los_bits_del_archivo_llegan_intactos_al_empaquetado_dop() {
    // El equivalente de bit-perfect en DSD: la carga útil de cada palabra
    // DoP es exactamente el par de bytes que traía el archivo.
    let path = temporal("dop");
    write_dsf(&path, 1, 1, u64::from(BLOCK) * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let channels = r.next_block().unwrap().unwrap();

    let mut packer = DopPacker::new();
    let mut words = Vec::new();
    packer.pack(&[&channels[0], &channels[1]], &mut words);

    assert_eq!(words.len(), channels[0].len() / 2 * 2, "una palabra por canal y por frame");
    for (frame, par) in words.chunks(2).enumerate().take(8) {
        let marker = if frame % 2 == 0 { 0x05 } else { 0xFA };
        for (canal, word) in par.iter().enumerate() {
            let unjustified = word >> 8;
            assert_eq!((unjustified >> 16) & 0xFF, marker, "el marker alterna por frame");
            let payload = (unjustified & 0xFFFF) as u16;
            let expected =
                u16::from_be_bytes([channels[canal][frame * 2], channels[canal][frame * 2 + 1]]);
            assert_eq!(payload, expected, "la carga útil es el DSD del archivo, sin tocar");
        }
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn buscar_cae_en_el_arranque_de_un_bloque() {
    let path = temporal("buscar");
    write_dsf(&path, 4, 1, u64::from(BLOCK) * 4 * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let dropped = r.seek_to_sample(u64::from(BLOCK) * 8 * 2 + 40).unwrap();
    assert_eq!(dropped, u64::from(BLOCK) * 8 * 2, "se alinea al bloque, hacia atrás");
    let channels = r.next_block().unwrap().unwrap();
    let expected = ((BLOCK * 2) % 251) as u8;
    assert_eq!(channels[0][0], expected.reverse_bits(), "empieza en el bloque correcto");
    let _ = std::fs::remove_file(path);
}
