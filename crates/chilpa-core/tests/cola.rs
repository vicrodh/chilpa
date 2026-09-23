//! La cola encadena sin hueco cuando puede, y se detiene cuando debe.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_core::queue_playback::{finished, play_queue, renegotiations};
use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

/// Cuántas veces se abrió el dispositivo y cuántos frames recibió cada vez.
#[derive(Default)]
struct Registro {
    aperturas: Vec<StreamFormat>,
    frames: u64,
    fallar_tras: Option<u64>,
}

struct Falso {
    registro: Rc<RefCell<Registro>>,
    stream: StreamFormat,
}

impl AudioSink for Falso {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let mut r = self.registro.borrow_mut();
        if let Some(limite) = r.fallar_tras {
            if r.frames >= limite {
                return Err(SinkError::DeviceLost);
            }
        }
        // Se escribe de a poco, como un dispositivo real, para que una
        // pérdida pueda caer a media pista.
        let n = (frames.len() / self.stream.frame_bytes()).min(256);
        r.frames += n as u64;
        Ok(n)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

fn escribir_wav(path: &PathBuf, frames: u32, bits: u16, rate: u32) {
    let bytes = u32::from(bits) / 8;
    let data_len = frames * 2 * bytes;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 2 * bytes).to_le_bytes()).unwrap();
    f.write_all(&(2 * bits / 8).to_le_bytes()).unwrap();
    f.write_all(&bits.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for i in 0..frames {
        let v = (i % 1000) as i32;
        f.write_all(&v.to_le_bytes()[..bytes as usize]).unwrap();
        f.write_all(&(-v).to_le_bytes()[..bytes as usize]).unwrap();
    }
}

fn pista(nombre: &str, frames: u32, bits: u16, rate: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-cola-{nombre}.wav"));
    escribir_wav(&p, frames, bits, rate);
    p
}

fn caps() -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![44_100, 96_000],
        formats: vec![SampleFormat::S16, SampleFormat::S24In32],
        channels: vec![2],
        probed: true,
    }
}

fn reproducir(
    pistas: &[PathBuf],
    registro: &Rc<RefCell<Registro>>,
) -> Vec<chilpa_core::queue_playback::TrackResult> {
    play_queue(pistas, &caps(), Policy::BitPerfect, &StopSignal::new(), |stream| {
        registro.borrow_mut().aperturas.push(stream);
        Ok(Falso { registro: Rc::clone(registro), stream })
    })
    .expect("la cola corre")
}

#[test]
fn tres_pistas_del_mismo_formato_abren_el_dispositivo_una_vez() {
    let pistas = vec![
        pista("a", 1_000, 16, 44_100),
        pista("b", 2_000, 16, 44_100),
        pista("c", 1_500, 16, 44_100),
    ];
    let registro = Rc::new(RefCell::new(Registro::default()));
    let resultados = reproducir(&pistas, &registro);

    assert_eq!(resultados.len(), 3);
    assert_eq!(registro.borrow().aperturas.len(), 1, "el dispositivo se abrió una sola vez");
    assert_eq!(renegotiations(&resultados), 0, "nada de huecos entre pistas");
    assert_eq!(registro.borrow().frames, 4_500, "salieron todos los frames de las tres");
    assert_eq!(finished(&resultados).count(), 3);
    for p in pistas {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn cambiar_de_formato_reabre_el_dispositivo_y_se_reporta() {
    let pistas = vec![
        pista("cd", 1_000, 16, 44_100),
        pista("hires", 1_000, 24, 96_000),
        pista("hires2", 500, 24, 96_000),
    ];
    let registro = Rc::new(RefCell::new(Registro::default()));
    let resultados = reproducir(&pistas, &registro);

    let aperturas = registro.borrow().aperturas.clone();
    assert_eq!(aperturas.len(), 2, "solo se reabre cuando cambia el formato");
    assert_eq!(aperturas[0].sample_rate_hz, 44_100);
    assert_eq!(aperturas[1].sample_rate_hz, 96_000);
    assert_eq!(renegotiations(&resultados), 1, "la segunda pista dice que hubo corte");
    assert!(resultados[2].transition.is_seamless(), "la tercera sí encadena");
    for p in pistas {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn si_el_dispositivo_se_pierde_la_cola_se_detiene_ahi() {
    // El hallazgo de QBZ: una escritura fallida se veía como fin de pista y
    // la cola entera se saltaba sin avisar.
    let pistas = vec![
        pista("uno", 1_000, 16, 44_100),
        pista("dos", 1_000, 16, 44_100),
        pista("tres", 1_000, 16, 44_100),
    ];
    let registro = Rc::new(RefCell::new(Registro { fallar_tras: Some(500), ..Default::default() }));
    let resultados = reproducir(&pistas, &registro);

    assert_eq!(resultados.len(), 1, "la cola se detuvo en la pista que falló");
    assert!(!resultados[0].outcome.may_advance_queue(), "esa pista no deja avanzar");
    assert!(resultados[0].frames() > 0, "alcanzó a sonar antes de perderse");
    assert_eq!(finished(&resultados).count(), 0);
    for p in pistas {
        let _ = std::fs::remove_file(p);
    }
}
