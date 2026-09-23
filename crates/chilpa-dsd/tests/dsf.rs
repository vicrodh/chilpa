//! Un DSF sintético que se lee entero, con los bits intactos.

use chilpa_audio::dop::DopPacker;
use chilpa_dsd::{DsdSource, DsfReader};
use std::io::Write;
use std::path::PathBuf;

const BLOQUE: u32 = 4096;
const TASA: u32 = 2_822_400; // DSD64

/// Escribe un DSF de dos canales con `bloques` bloques por canal.
///
/// El byte n del canal izquierdo vale n, y el del derecho su complemento,
/// así que cualquier corrimiento o mezcla entre canales se nota.
fn escribir_dsf(path: &PathBuf, bloques: u32, bits_por_muestra: u32, muestras: u64) {
    let datos = u64::from(bloques) * u64::from(BLOQUE) * 2;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"DSD ").unwrap();
    f.write_all(&28u64.to_le_bytes()).unwrap();
    f.write_all(&(28u64 + 52 + 12 + datos).to_le_bytes()).unwrap();
    f.write_all(&0u64.to_le_bytes()).unwrap(); // sin tags
    f.write_all(b"fmt ").unwrap();
    f.write_all(&52u64.to_le_bytes()).unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap(); // versión
    f.write_all(&0u32.to_le_bytes()).unwrap(); // format id: DSD crudo
    f.write_all(&2u32.to_le_bytes()).unwrap(); // estéreo
    f.write_all(&2u32.to_le_bytes()).unwrap(); // canales
    f.write_all(&TASA.to_le_bytes()).unwrap();
    f.write_all(&bits_por_muestra.to_le_bytes()).unwrap();
    f.write_all(&muestras.to_le_bytes()).unwrap();
    f.write_all(&BLOQUE.to_le_bytes()).unwrap();
    f.write_all(&0u32.to_le_bytes()).unwrap(); // reservado
    f.write_all(b"data").unwrap();
    f.write_all(&(12 + datos).to_le_bytes()).unwrap();
    for b in 0..bloques {
        for canal in 0..2u32 {
            for i in 0..BLOQUE {
                let v = ((b * BLOQUE + i) % 251) as u8;
                f.write_all(&[if canal == 0 { v } else { !v }]).unwrap();
            }
        }
    }
}

fn temporal(nombre: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-dsd-{nombre}.dsf"));
    p
}

#[test]
fn se_lee_el_encabezado_y_todos_los_bloques() {
    let path = temporal("basico");
    let muestras = u64::from(BLOQUE) * 3 * 8;
    escribir_dsf(&path, 3, 1, muestras);

    let mut r = DsfReader::open(&path).unwrap();
    assert_eq!(r.info().channels, 2);
    assert_eq!(r.info().dsd_rate_hz, TASA);
    assert_eq!(r.info().multiple(), 64, "DSD64");
    assert_eq!(r.info().samples, muestras);

    let mut bloques = 0;
    let mut bytes_izq = 0;
    while let Some(canales) = r.next_block().unwrap() {
        assert_eq!(canales.len(), 2);
        assert_eq!(canales[0].len(), canales[1].len());
        bytes_izq += canales[0].len();
        bloques += 1;
    }
    assert_eq!(bloques, 3);
    assert_eq!(bytes_izq as u64, muestras / 8, "salieron todos los bytes válidos");
    let _ = std::fs::remove_file(path);
}

#[test]
fn los_bits_se_entregan_con_el_primero_en_tiempo_arriba() {
    // DSF guarda el bit primero en tiempo abajo; aquí debe salir invertido.
    let path = temporal("orden");
    escribir_dsf(&path, 1, 1, u64::from(BLOQUE) * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let canales = r.next_block().unwrap().unwrap();
    assert_eq!(canales[0][0], 0u8.reverse_bits());
    assert_eq!(canales[0][1], 1u8.reverse_bits());
    assert_eq!(canales[1][1], (!1u8).reverse_bits(), "el canal derecho es el complemento");
    let _ = std::fs::remove_file(path);

    // Con bits_per_sample = 8 el archivo ya viene con el primero arriba y no
    // se toca.
    let path = temporal("orden-msb");
    escribir_dsf(&path, 1, 8, u64::from(BLOQUE) * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let canales = r.next_block().unwrap().unwrap();
    assert_eq!(canales[0][1], 1u8, "sin invertir");
    let _ = std::fs::remove_file(path);
}

#[test]
fn el_relleno_del_ultimo_bloque_no_sale() {
    let path = temporal("relleno");
    // Dos bloques en el archivo, pero solo 100 bytes por canal son música.
    escribir_dsf(&path, 2, 1, 100 * 8);
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
    escribir_dsf(&path, 1, 1, u64::from(BLOQUE) * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let canales = r.next_block().unwrap().unwrap();

    let mut packer = DopPacker::new();
    let mut palabras = Vec::new();
    packer.pack(&[&canales[0], &canales[1]], &mut palabras);

    assert_eq!(palabras.len(), canales[0].len() / 2 * 2, "una palabra por canal y por frame");
    for (frame, par) in palabras.chunks(2).enumerate().take(8) {
        let marker = if frame % 2 == 0 { 0x05 } else { 0xFA };
        for (canal, palabra) in par.iter().enumerate() {
            let sin_justificar = palabra >> 8;
            assert_eq!((sin_justificar >> 16) & 0xFF, marker, "el marker alterna por frame");
            let carga = (sin_justificar & 0xFFFF) as u16;
            let esperado =
                u16::from_be_bytes([canales[canal][frame * 2], canales[canal][frame * 2 + 1]]);
            assert_eq!(carga, esperado, "la carga útil es el DSD del archivo, sin tocar");
        }
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn buscar_cae_en_el_arranque_de_un_bloque() {
    let path = temporal("buscar");
    escribir_dsf(&path, 4, 1, u64::from(BLOQUE) * 4 * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let cayo = r.seek_to_sample(u64::from(BLOQUE) * 8 * 2 + 40).unwrap();
    assert_eq!(cayo, u64::from(BLOQUE) * 8 * 2, "se alinea al bloque, hacia atrás");
    let canales = r.next_block().unwrap().unwrap();
    let esperado = ((BLOQUE * 2) % 251) as u8;
    assert_eq!(canales[0][0], esperado.reverse_bits(), "empieza en el bloque correcto");
    let _ = std::fs::remove_file(path);
}
