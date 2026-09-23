//! Dos pistas seguidas salen pegadas, sin hueco ni muestras repetidas.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_with, Controls, SharedQueue};
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Clone, Default)]
struct Capturador(Rc<RefCell<Vec<i16>>>);

impl AudioSink for Capturador {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let mut m = self.0.borrow_mut();
        for par in frames.chunks_exact(2) {
            m.push(i16::from_le_bytes([par[0], par[1]]));
        }
        Ok(frames.len() / 4)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// Un WAV cuyo canal izquierdo cuenta desde `desde`.
fn rampa(nombre: &str, desde: i32, frames: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-gapless-{nombre}.wav"));
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
        let v = ((desde + i as i32) % 30_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&(-v).to_le_bytes()).unwrap();
    }
    p
}

fn caps(rates: &[u32]) -> DeviceCaps {
    DeviceCaps {
        sample_rates: rates.to_vec(),
        formats: vec![SampleFormat::S16, SampleFormat::S24In32],
        channels: vec![2],
        probed: true,
    }
}

fn reproducir(pistas: &[PathBuf], caps: &DeviceCaps) -> (Vec<i16>, usize) {
    let cap = Capturador::default();
    let mut aperturas = 0;
    play_queue_with(
        &SharedQueue::new(pistas.to_vec()),
        caps,
        Policy::BitPerfect,
        &Controls::new(),
        |_| {
            aperturas += 1;
            Ok(cap.clone())
        },
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("la cola corre");
    let muestras = cap.0.borrow().clone();
    (muestras, aperturas)
}

#[test]
fn dos_pistas_del_mismo_formato_salen_pegadas() {
    // La segunda continúa la cuenta de la primera: si hubiera hueco,
    // repetición o recorte, la secuencia se rompe en la unión.
    let a = rampa("a", 0, 5_000);
    let b = rampa("b", 5_000, 5_000);
    let (muestras, aperturas) = reproducir(&[a.clone(), b.clone()], &caps(&[44_100]));

    assert_eq!(aperturas, 1, "el dispositivo se abrió una sola vez");
    assert_eq!(muestras.len(), 20_000, "salieron los 10 000 frames estéreo");

    let izquierdo: Vec<i16> = muestras.iter().step_by(2).copied().collect();
    let esperado: Vec<i16> = (0..10_000).map(|i| (i % 30_000) as i16).collect();
    assert_eq!(izquierdo, esperado, "la unión entre pistas no es continua");

    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn si_cambia_el_formato_se_reabre_y_no_se_pierde_nada() {
    let a = rampa("cd", 0, 4_000);
    // La segunda pista es de otra frecuencia: encadenar sin hueco es
    // imposible, pero no debe perderse ni una muestra.
    let b = {
        let mut p = std::env::temp_dir();
        p.push("chilpa-gapless-hires.wav");
        let frames = 3_000u32;
        let data = frames * 4;
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&96_000u32.to_le_bytes()).unwrap();
        f.write_all(&384_000u32.to_le_bytes()).unwrap();
        f.write_all(&4u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data.to_le_bytes()).unwrap();
        for i in 0..frames {
            let v = (i % 30_000) as i16;
            f.write_all(&v.to_le_bytes()).unwrap();
            f.write_all(&(-v).to_le_bytes()).unwrap();
        }
        p
    };

    let (muestras, aperturas) = reproducir(&[a.clone(), b.clone()], &caps(&[44_100, 96_000]));
    assert_eq!(aperturas, 2, "cambiar de frecuencia obliga a reabrir");
    assert_eq!(muestras.len(), (4_000 + 3_000) * 2, "no se perdió ninguna muestra");

    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}
