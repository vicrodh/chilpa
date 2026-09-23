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
struct CallLog {
    opens: Vec<StreamFormat>,
    frames: u64,
    fail_after: Option<u64>,
}

struct FakeSink {
    log: Rc<RefCell<CallLog>>,
    stream: StreamFormat,
}

impl AudioSink for FakeSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let mut r = self.log.borrow_mut();
        if let Some(limit) = r.fail_after {
            if r.frames >= limit {
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

fn write_wav(path: &PathBuf, frames: u32, bits: u16, rate: u32) {
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

fn track(name: &str, frames: u32, bits: u16, rate: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-cola-{name}.wav"));
    write_wav(&p, frames, bits, rate);
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

fn play(
    tracks: &[PathBuf],
    log: &Rc<RefCell<CallLog>>,
) -> Vec<chilpa_core::queue_playback::TrackResult> {
    play_queue(tracks, &caps(), Policy::BitPerfect, &StopSignal::new(), |stream| {
        log.borrow_mut().opens.push(stream);
        Ok(FakeSink { log: Rc::clone(log), stream })
    })
    .expect("la cola corre")
}

#[test]
fn three_tracks_in_the_same_format_open_the_device_once() {
    let tracks = vec![
        track("a", 1_000, 16, 44_100),
        track("b", 2_000, 16, 44_100),
        track("c", 1_500, 16, 44_100),
    ];
    let log = Rc::new(RefCell::new(CallLog::default()));
    let results = play(&tracks, &log);

    assert_eq!(results.len(), 3);
    assert_eq!(log.borrow().opens.len(), 1, "el dispositivo se abrió una sola vez");
    assert_eq!(renegotiations(&results), 0, "nada de huecos entre pistas");
    assert_eq!(log.borrow().frames, 4_500, "salieron todos los frames de las tres");
    assert_eq!(finished(&results).count(), 3);
    for p in tracks {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn changing_format_reopens_the_device_and_is_reported() {
    let tracks = vec![
        track("cd", 1_000, 16, 44_100),
        track("hires", 1_000, 24, 96_000),
        track("hires2", 500, 24, 96_000),
    ];
    let log = Rc::new(RefCell::new(CallLog::default()));
    let results = play(&tracks, &log);

    let opens = log.borrow().opens.clone();
    assert_eq!(opens.len(), 2, "solo se reabre cuando cambia el formato");
    assert_eq!(opens[0].sample_rate_hz, 44_100);
    assert_eq!(opens[1].sample_rate_hz, 96_000);
    assert_eq!(renegotiations(&results), 1, "la segunda pista dice que hubo corte");
    assert!(results[2].transition.is_seamless(), "la tercera sí encadena");
    for p in tracks {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn if_the_device_is_lost_the_queue_stops_there() {
    // El hallazgo de QBZ: una escritura fallida se veía como fin de pista y
    // la cola entera se saltaba sin avisar.
    let tracks = vec![
        track("uno", 1_000, 16, 44_100),
        track("dos", 1_000, 16, 44_100),
        track("tres", 1_000, 16, 44_100),
    ];
    let log = Rc::new(RefCell::new(CallLog { fail_after: Some(500), ..Default::default() }));
    let results = play(&tracks, &log);

    assert_eq!(results.len(), 1, "la cola se detuvo en la pista que falló");
    assert!(!results[0].outcome.may_advance_queue(), "esa pista no deja avanzar");
    assert!(results[0].frames() > 0, "alcanzó a sonar antes de perderse");
    assert_eq!(finished(&results).count(), 0);
    for p in tracks {
        let _ = std::fs::remove_file(p);
    }
}
