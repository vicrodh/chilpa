//! Cómo se recorre la cola: repetición y orden.
//!
//! Lo que se cuida aquí es que repetir no sea un lazo del que no se sale:
//! parar corta la vuelta, saltar avanza aunque esté en repeat-one, y una
//! pista que no entrega un solo frame no deja girando el hilo.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_with, Controls, Repeat, SharedQueue};
use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

/// Cuenta frames y, cuando llega al tope, pide parar: es lo que corta una
/// repetición que si no seguiría para siempre.
#[derive(Clone)]
struct Topado {
    frames: Rc<RefCell<u64>>,
    tope: u64,
    stop: chilpa_audio::sink::StopSignal,
}

impl AudioSink for Topado {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let n = (frames.len() / 4) as u64;
        let mut total = self.frames.borrow_mut();
        *total += n;
        if *total >= self.tope {
            self.stop.stop();
        }
        Ok(n as usize)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

fn onda(nombre: &str, frames: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-repetir-{nombre}.wav"));
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
        f.write_all(&v.to_le_bytes()).unwrap();
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

/// Reproduce hasta entregar `tope` frames y regresa qué pistas sonaron.
fn sonadas(pistas: &[PathBuf], repeat: Repeat, tope: u64) -> Vec<PathBuf> {
    let controls = Controls::new();
    controls.repeat.set(repeat);
    let sink = Topado {
        frames: Rc::new(RefCell::new(0)),
        tope,
        stop: controls.stop.clone(),
    };
    let resultados =
        play_queue_with(&SharedQueue::new(pistas.to_vec()), &caps(), Policy::BitPerfect, &controls, |_| Ok(sink.clone()),
            &mut |_, _, _| {}, &mut |_, _| {})
            .expect("la cola corre");
    resultados.into_iter().map(|r| r.path).collect()
}

#[test]
fn sin_repeticion_la_cola_se_acaba() {
    let a = onda("off-a", 4_410);
    let b = onda("off-b", 4_410);
    let pistas = vec![a.clone(), b.clone()];
    // El tope es más alto que las dos pistas juntas: si la cola no se
    // acabara sola, este test no terminaría.
    assert_eq!(sonadas(&pistas, Repeat::Off, 1_000_000), vec![a.clone(), b.clone()]);
    for p in pistas {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn repeat_all_vuelve_a_empezar() {
    let a = onda("all-a", 4_410);
    let b = onda("all-b", 4_410);
    let pistas = vec![a.clone(), b.clone()];
    // Tope a dos vueltas y media; el paro llega a media tercera pista.
    let sonadas = sonadas(&pistas, Repeat::All, 4_410 * 5);
    assert!(sonadas.len() > 2, "dio más de una vuelta: {sonadas:?}");
    assert_eq!(sonadas[0], a, "empezó por la primera");
    assert_eq!(sonadas[2], a, "y volvió a ella tras la última");
    for p in pistas {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn repeat_one_se_queda_en_la_misma() {
    let a = onda("one-a", 4_410);
    let b = onda("one-b", 4_410);
    let pistas = vec![a.clone(), b.clone()];
    let sonadas = sonadas(&pistas, Repeat::One, 4_410 * 3);
    assert!(sonadas.len() > 1, "se repitió: {sonadas:?}");
    assert!(sonadas.iter().all(|p| *p == a), "nunca pasó a la segunda: {sonadas:?}");
    for p in pistas {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn en_repeat_one_saltar_si_avanza() {
    let a = onda("salto-a", 44_100);
    let b = onda("salto-b", 4_410);
    let pistas = vec![a.clone(), b.clone()];

    let controls = Controls::new();
    controls.repeat.set(Repeat::One);
    // Se pide el salto antes de empezar: la primera pista termina enseguida
    // y, aunque esté en repeat-one, la cola tiene que pasar a la segunda.
    controls.skip.skip();
    let sink = Topado {
        frames: Rc::new(RefCell::new(0)),
        tope: 4_410 * 2,
        stop: controls.stop.clone(),
    };
    let resultados =
        play_queue_with(&SharedQueue::new(pistas.clone()), &caps(), Policy::BitPerfect, &controls, |_| Ok(sink.clone()),
            &mut |_, _, _| {}, &mut |_, _| {})
            .expect("la cola corre");
    let rutas: Vec<PathBuf> = resultados.into_iter().map(|r| r.path).collect();
    assert_eq!(rutas[0], a);
    assert_eq!(rutas[1], b, "el salto ganó sobre la repetición: {rutas:?}");
    for p in pistas {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn un_orden_propio_cambia_lo_que_suena() {
    let a = onda("orden-a", 4_410);
    let b = onda("orden-b", 4_410);
    let c = onda("orden-c", 4_410);
    let pistas = vec![a.clone(), b.clone(), c.clone()];

    let controls = Controls::new();
    assert!(controls.order.set(vec![2, 0, 1], 3), "el orden es una permutación");
    let sink = Topado {
        frames: Rc::new(RefCell::new(0)),
        tope: 1_000_000,
        stop: controls.stop.clone(),
    };
    let resultados = play_queue_with(
        &SharedQueue::new(pistas.clone()),
        &caps(),
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("la cola corre");
    let rutas: Vec<PathBuf> = resultados.into_iter().map(|r| r.path).collect();
    assert_eq!(rutas, vec![c, a, b], "sonaron en el orden pedido");
    for p in pistas {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn lo_que_se_encola_a_media_cola_si_suena() {
    let a = onda("crece-a", 4_410);
    let b = onda("crece-b", 4_410);

    let cola = SharedQueue::new(vec![a.clone()]);
    let controls = Controls::new();
    let sink = Topado {
        frames: Rc::new(RefCell::new(0)),
        tope: 1_000_000,
        stop: controls.stop.clone(),
    };
    // La segunda entra cuando la primera ya arrancó: la cola no se rearma,
    // le crece la cola.
    let mut agregada = false;
    let resultados = play_queue_with(
        &cola,
        &caps(),
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {
            if !agregada {
                agregada = true;
                cola.extend(std::slice::from_ref(&b));
            }
        },
        &mut |_, _| {},
    )
    .expect("la cola corre");

    let rutas: Vec<PathBuf> = resultados.into_iter().map(|r| r.path).collect();
    assert_eq!(rutas, vec![a.clone(), b.clone()], "la agregada también sonó");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn una_pista_quitada_no_suena_y_las_demas_no_se_mueven() {
    let a = onda("quita-a", 4_410);
    let b = onda("quita-b", 4_410);
    let c = onda("quita-c", 4_410);

    let cola = SharedQueue::new(vec![a.clone(), b.clone(), c.clone()]);
    // Se quita la de en medio antes de empezar.
    assert!(cola.remove(1));
    assert!(!cola.remove(1), "quitarla dos veces no cuenta");
    assert_eq!(cola.len(), 3, "la posición se conserva vacía");

    let controls = Controls::new();
    let sink = Topado {
        frames: Rc::new(RefCell::new(0)),
        tope: 1_000_000,
        stop: controls.stop.clone(),
    };
    let resultados = play_queue_with(
        &cola,
        &caps(),
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("la cola corre");

    let rutas: Vec<PathBuf> = resultados.into_iter().map(|r| r.path).collect();
    assert_eq!(rutas, vec![a.clone(), c.clone()], "la de en medio se saltó");
    for p in [a, b, c] {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn quitar_toda_la_cola_con_repeticion_no_deja_girando_el_hilo() {
    let a = onda("vacia-a", 4_410);
    let cola = SharedQueue::new(vec![a.clone()]);
    cola.remove(0);

    let controls = Controls::new();
    controls.repeat.set(Repeat::All);
    let sink = Topado {
        frames: Rc::new(RefCell::new(0)),
        tope: 1_000_000,
        stop: controls.stop.clone(),
    };
    let resultados = play_queue_with(
        &cola,
        &caps(),
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("la cola corre");
    assert!(resultados.is_empty(), "no sonó nada, y terminó");
    let _ = std::fs::remove_file(a);
}
