//! Saltar mientras suena: la pista sigue desde el punto pedido.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::seek::SeekSignal;
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_core::queue_playback::play_queue_with;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

/// Guarda lo entregado para poder mirarlo después.
#[derive(Clone, Default)]
struct Capturador(std::rc::Rc<std::cell::RefCell<Vec<i16>>>);

impl AudioSink for Capturador {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let mut muestras = self.0.borrow_mut();
        for par in frames.chunks_exact(2) {
            muestras.push(i16::from_le_bytes([par[0], par[1]]));
        }
        Ok(frames.len() / 4)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// Un WAV donde la muestra del canal izquierdo dice su número de frame.
fn rampa(nombre: &str, frames: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-saltar-{nombre}.wav"));
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
        let v = (i % 30_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&(-v).to_le_bytes()).unwrap();
    }
    p
}

fn caps() -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![44_100],
        formats: vec![SampleFormat::S16],
        channels: vec![2],
        probed: true,
    }
}

#[test]
fn un_salto_pedido_antes_de_empezar_arranca_en_ese_punto() {
    let path = rampa("arranque", 44_100 * 3);
    let seek = SeekSignal::new();
    seek.request(Duration::from_secs(2));
    let cap = Capturador::default();
    let mut posiciones = Vec::new();

    play_queue_with(
        std::slice::from_ref(&path),
        &caps(),
        Policy::BitPerfect,
        &StopSignal::new(),
        &chilpa_audio::sink::PauseSignal::new(),
        &seek,
        &chilpa_audio::sink::SkipSignal::new(),
        |_| Ok(cap.clone()),
        &mut |_, _, _| {},
        &mut |_, frames| posiciones.push(frames),
    )
    .unwrap();

    // La primera muestra que sale es la del frame 88 200, o sea el segundo 2.
    assert_eq!(cap.0.borrow()[0], (88_200 % 30_000) as i16, "arrancó en el punto pedido");
    // Y la posición que se reporta cuenta desde el archivo, no desde el salto.
    assert!(
        posiciones.first().is_some_and(|p| *p >= 88_200),
        "la primera posición fue {:?}",
        posiciones.first()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn saltar_mas_alla_del_final_termina_la_pista_sin_tronar() {
    let path = rampa("pasado", 44_100);
    let seek = SeekSignal::new();
    seek.request(Duration::from_secs(60));
    let cap = Capturador::default();
    let resultados = play_queue_with(
        std::slice::from_ref(&path),
        &caps(),
        Policy::BitPerfect,
        &StopSignal::new(),
        &chilpa_audio::sink::PauseSignal::new(),
        &seek,
        &chilpa_audio::sink::SkipSignal::new(),
        |_| Ok(cap.clone()),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(resultados.len(), 1);
    assert!(resultados[0].outcome.may_advance_queue(), "terminó como fin natural");
    let _ = std::fs::remove_file(path);
}
