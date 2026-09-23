//! La misma música en varios contenedores y códecs.
//!
//! Dos cosas se verifican aquí:
//!
//! 1. **Los sin pérdida salen idénticos** al original, sin importar el
//!    contenedor. Es el invariante de bit-perfect aplicado a cada formato.
//! 2. **La decodificación es por bloques**, nunca la pista entera en
//!    memoria. En QBZ, AAC y ALAC en MP4 se decodificaban completos a un
//!    `Vec<f32>` (unos 2.7 GB por una hora a 24/96). Aquí se mide el bloque
//!    más grande que entrega el decodificador: si alguien vuelve a leer el
//!    archivo entero, esta prueba lo caza.
//!
//! Los archivos se generan con ffmpeg. Si no está instalado, las pruebas se
//! saltan en vez de fallar.

use chilpa_decode::{Block, Decoded};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const RATE: u32 = 44_100;
const FRAMES: u32 = 44_100 * 3;
/// Un bloque de más de un segundo ya es sospechoso de leer de más.
const MAX_FRAMES_POR_BLOQUE: usize = RATE as usize;

fn hay_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn dir() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("chilpa-formatos");
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// WAV de referencia: el canal izquierdo lleva una rampa y el derecho su
/// negado, así que cualquier desfase o conversión se nota.
fn escribir_wav(path: &Path, bits: u16) {
    let bytes = u32::from(bits) / 8;
    let data_len = FRAMES * 2 * bytes;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&RATE.to_le_bytes()).unwrap();
    f.write_all(&(RATE * 2 * bytes).to_le_bytes()).unwrap();
    f.write_all(&(2 * bits / 8).to_le_bytes()).unwrap();
    f.write_all(&bits.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for i in 0..FRAMES {
        let v = muestra(i, bits);
        match bits {
            16 => {
                f.write_all(&(v as i16).to_le_bytes()).unwrap();
                f.write_all(&(-(v as i16)).to_le_bytes()).unwrap();
            }
            _ => {
                f.write_all(&v.to_le_bytes()[..3]).unwrap();
                f.write_all(&(-v).to_le_bytes()[..3]).unwrap();
            }
        }
    }
}

fn muestra(i: u32, bits: u16) -> i32 {
    let periodo = if bits == 16 { 30_000 } else { 8_000_000 };
    (i as i32 % periodo) - periodo / 2
}

fn convertir(origen: &Path, destino: &Path, args: &[&str]) -> bool {
    let salida = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(origen)
        .args(args)
        .arg(destino)
        .output()
        .expect("ffmpeg corre");
    if !salida.status.success() {
        eprintln!("ffmpeg falló: {}", String::from_utf8_lossy(&salida.stderr));
    }
    salida.status.success()
}

/// Decodifica todo y regresa las muestras del canal izquierdo más el bloque
/// más grande que entregó el decodificador.
fn decodificar(path: &Path) -> (Vec<i32>, usize, u8) {
    let mut decoded = Decoded::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let canales = decoded.stream_format().channels as usize;
    let mut izquierdo = Vec::new();
    let mut mayor = 0usize;
    let mut bits = 0u8;
    while let Some(block) = decoded.next_block().unwrap() {
        match block {
            Block::Int { data, bits: b } => {
                bits = b;
                mayor = mayor.max(data.len() / canales);
                izquierdo.extend(data.iter().step_by(canales));
            }
            Block::Float(data) => {
                mayor = mayor.max(data.len() / canales);
                izquierdo.extend(data.iter().step_by(canales).map(|v| (v * 32768.0) as i32));
            }
        }
    }
    (izquierdo, mayor, bits)
}

#[test]
fn los_formatos_sin_perdida_salen_identicos_y_por_bloques() {
    if !hay_ffmpeg() {
        eprintln!("sin ffmpeg: se salta");
        return;
    }
    let d = dir();
    let wav16 = d.join("ref16.wav");
    let wav24 = d.join("ref24.wav");
    escribir_wav(&wav16, 16);
    escribir_wav(&wav24, 24);

    let esperado16: Vec<i32> = (0..FRAMES).map(|i| muestra(i, 16)).collect();
    let esperado24: Vec<i32> = (0..FRAMES).map(|i| muestra(i, 24)).collect();

    let casos: &[(&str, &Path, &[&str], &Vec<i32>, u8)] = &[
        ("flac16.flac", &wav16, &["-c:a", "flac"], &esperado16, 16),
        ("flac24.flac", &wav24, &["-c:a", "flac"], &esperado24, 24),
        ("alac16.m4a", &wav16, &["-c:a", "alac"], &esperado16, 16),
        ("alac24.m4a", &wav24, &["-c:a", "alac"], &esperado24, 24),
        ("pcm24.wav", &wav24, &["-c:a", "pcm_s24le"], &esperado24, 24),
        ("pcm16.aiff", &wav16, &["-c:a", "pcm_s16be"], &esperado16, 16),
    ];

    for (nombre, origen, args, esperado, bits_esperados) in casos {
        let destino = d.join(nombre);
        if !convertir(origen, &destino, args) {
            eprintln!("{nombre}: ffmpeg no pudo, se salta");
            continue;
        }
        let (salieron, mayor, bits) = decodificar(&destino);
        assert_eq!(bits, *bits_esperados, "{nombre}: los bits del archivo mandan");
        assert_eq!(salieron.len(), esperado.len(), "{nombre}: faltaron o sobraron frames");
        assert_eq!(&salieron, *esperado, "{nombre}: la señal cambió");
        assert!(
            mayor <= MAX_FRAMES_POR_BLOQUE,
            "{nombre}: un bloque de {mayor} frames; se está decodificando de más"
        );
        let _ = std::fs::remove_file(destino);
    }
    let _ = std::fs::remove_file(wav16);
    let _ = std::fs::remove_file(wav24);
}

#[test]
fn los_formatos_con_perdida_se_decodifican_por_bloques() {
    if !hay_ffmpeg() {
        eprintln!("sin ffmpeg: se salta");
        return;
    }
    let d = dir();
    let wav = d.join("ref-perdida.wav");
    escribir_wav(&wav, 16);

    // Opus no entra: el decodificador que usamos todavía no lo trae. Queda
    // como hueco conocido, y la prueba de abajo lo fija.
    for (nombre, args) in [
        ("aac.m4a", ["-c:a", "aac"]),
        ("mp3.mp3", ["-c:a", "mp3"]),
        ("vorbis.ogg", ["-c:a", "libvorbis"]),
    ] {
        let destino = d.join(nombre);
        if !convertir(&wav, &destino, &args) {
            eprintln!("{nombre}: ffmpeg no pudo, se salta");
            continue;
        }
        let (salieron, mayor, _) = decodificar(&destino);
        assert!(!salieron.is_empty(), "{nombre}: no salió audio");
        // Con pérdida la duración se mueve un poco por el relleno del
        // codificador; basta con que ande cerca.
        let diferencia = (salieron.len() as i64 - FRAMES as i64).abs();
        assert!(
            diferencia < RATE as i64 / 2,
            "{nombre}: salieron {} frames contra {FRAMES}",
            salieron.len()
        );
        assert!(
            mayor <= MAX_FRAMES_POR_BLOQUE,
            "{nombre}: un bloque de {mayor} frames; se está decodificando de más"
        );
        let _ = std::fs::remove_file(destino);
    }
    let _ = std::fs::remove_file(wav);
}

#[test]
fn opus_todavia_no_se_puede_y_se_dice_claro() {
    if !hay_ffmpeg() {
        eprintln!("sin ffmpeg: se salta");
        return;
    }
    let d = dir();
    let wav = d.join("ref-opus.wav");
    escribir_wav(&wav, 16);
    let destino = d.join("prueba.opus");
    if !convertir(&wav, &destino, &["-c:a", "libopus"]) {
        eprintln!("sin codificador de opus: se salta");
        return;
    }
    match Decoded::open(&destino) {
        Err(e) => {
            let texto = e.to_string();
            assert!(texto.contains("no soportado"), "el error debe decir qué pasó: {texto}");
        }
        Ok(_) => panic!("ya se puede abrir opus: quita esta prueba y agrégalo a la matriz"),
    }
    let _ = std::fs::remove_file(destino);
    let _ = std::fs::remove_file(wav);
}
