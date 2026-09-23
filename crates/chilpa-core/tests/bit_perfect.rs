//! El invariante del proyecto: lo que sale por el cable es, muestra por
//! muestra, lo que traía el archivo.
//!
//! La prueba no necesita un DAC: el dispositivo es un capturador en memoria
//! que guarda cada byte que recibe. Se genera un WAV, se reproduce y se
//! compara lo capturado contra las muestras originales. Si alguien mete un
//! resampleo, una conversión a flotante o un volumen por software en la
//! ruta, esta prueba lo caza.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::render::Outcome;
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_core::playback::play_file;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Dispositivo de mentiras que se queda con todo lo que le escriben.
#[derive(Default)]
struct Capturador {
    stream: Option<StreamFormat>,
    bytes: Vec<u8>,
}

struct Handle<'a>(&'a mut Capturador);

impl AudioSink for Handle<'_> {
    fn format(&self) -> StreamFormat {
        self.0.stream.expect("abierto")
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        self.0.bytes.extend_from_slice(frames);
        Ok(frames.len() / self.0.stream.expect("abierto").frame_bytes())
    }

    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// Escribe un WAV PCM con esas muestras enteras.
fn write_wav(path: &Path, samples: &[i32], bits: u16, rate: u32, channels: u16) {
    let bytes_per_sample = u32::from(bits) / 8;
    let data_len = samples.len() as u32 * bytes_per_sample;
    let byte_rate = rate * u32::from(channels) * bytes_per_sample;
    let block_align = channels * bits / 8;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM entero
    f.write_all(&channels.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    f.write_all(&block_align.to_le_bytes()).unwrap();
    f.write_all(&bits.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for &s in samples {
        match bits {
            16 => f.write_all(&(s as i16).to_le_bytes()).unwrap(),
            24 => f.write_all(&s.to_le_bytes()[..3]).unwrap(),
            _ => f.write_all(&s.to_le_bytes()).unwrap(),
        }
    }
}

fn temporal(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-bitperfect-{name}.wav"));
    p
}

/// Muestras que pegan en los bordes: silencio, ±1 LSB, y los extremos del
/// rango, que es justo donde el escalado por flotante se equivocaba.
fn samples(bits: u8) -> Vec<i32> {
    let max = (1i64 << (bits - 1)) as i32 - 1;
    let min = -(1i64 << (bits - 1)) as i32;
    let mut v = vec![0, 1, -1, max, min, max - 1, min + 1];
    for i in 0..64 {
        v.push((max / 64) * i - (max / 2));
    }
    v.extend_from_slice(&[max, min]);
    // Estéreo: el canal derecho va invertido. El mínimo no tiene opuesto
    // dentro del rango, así que ahí se usa el máximo.
    v.iter().flat_map(|&s| [s, if s == min { max } else { -s }]).collect()
}

fn caps(format: SampleFormat, rate: u32) -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![rate],
        formats: vec![format],
        channels: vec![2],
        probed: true,
    }
}

#[test]
fn a_16_bit_wav_goes_out_identical_over_the_wire() {
    let originals = samples(16);
    let path = temporal("16");
    write_wav(&path, &originals, 16, 44_100, 2);

    let mut cap = Capturador { stream: None, bytes: Vec::new() };
    let played = play_file(
        &path,
        &caps(SampleFormat::S16, 44_100),
        Policy::BitPerfect,
        &StopSignal::new(),
        |format| {
            cap.stream = Some(format);
            Ok(Handle(&mut cap))
        },
    )
    .expect("debe reproducir");

    assert!(played.delivery.is_bit_perfect(), "la entrega se declara bit-perfect");
    assert!(matches!(played.outcome, Outcome::Finished { .. }));

    let came_out: Vec<i32> = cap
        .bytes
        .chunks_exact(2)
        .map(|c| i32::from(i16::from_le_bytes([c[0], c[1]])))
        .collect();
    assert_eq!(came_out.len(), originals.len(), "salieron todas las muestras");
    assert_eq!(came_out, originals, "ni un LSB de diferencia");
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_24_bit_wav_goes_out_identical_over_the_wire() {
    let originals = samples(24);
    let path = temporal("24");
    write_wav(&path, &originals, 24, 96_000, 2);

    let mut cap = Capturador { stream: None, bytes: Vec::new() };
    let played = play_file(
        &path,
        &caps(SampleFormat::S24In32, 96_000),
        Policy::BitPerfect,
        &StopSignal::new(),
        |format| {
            cap.stream = Some(format);
            Ok(Handle(&mut cap))
        },
    )
    .expect("debe reproducir");

    assert!(played.delivery.is_bit_perfect());
    // S24In32 va justificado a la izquierda: los 24 bits válidos arriba.
    let came_out: Vec<i32> = cap
        .bytes
        .chunks_exact(4)
        .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) >> 8)
        .collect();
    assert_eq!(came_out, originals, "ni un LSB de diferencia a 24 bits");
    let _ = std::fs::remove_file(path);
}

#[test]
fn with_a_device_that_cannot_do_it_we_fail_instead_of_resampling() {
    let path = temporal("rechazo");
    write_wav(&path, &samples(16), 16, 44_100, 2);
    let solo_48k = caps(SampleFormat::S16, 48_000);

    let mut cap = Capturador { stream: None, bytes: Vec::new() };
    let r = play_file(&path, &solo_48k, Policy::BitPerfect, &StopSignal::new(), |format| {
        cap.stream = Some(format);
        Ok(Handle(&mut cap))
    });
    assert!(r.is_err(), "no se resamplea a escondidas");
    assert!(cap.bytes.is_empty(), "no se le escribió nada al dispositivo");
    let _ = std::fs::remove_file(path);
}
