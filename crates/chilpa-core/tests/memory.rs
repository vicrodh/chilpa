//! Reproducir no debe cargar la pista entera en memoria.
//!
//! Es el candado del hallazgo más caro de la auditoría: en QBZ, AAC y ALAC en
//! MP4 se decodificaban completos a un `Vec<f32>`, unos 2.7 GB por una hora a
//! 24/96. La prueba reproduce un archivo grande contra un dispositivo que
//! tira lo que recibe y mira cuánto creció la memoria del proceso.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_core::playback::play_file;
use std::path::PathBuf;
use std::process::Command;

/// Un dispositivo que acepta todo y no guarda nada.
struct CountingSink(StreamFormat, u64);

impl AudioSink for &mut CountingSink {
    fn format(&self) -> StreamFormat {
        self.0
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let n = frames.len() / self.0.frame_bytes();
        self.1 += n as u64;
        Ok(n)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())
        })
        .map(|kb| kb / 1024.0)
        .unwrap_or(0.0)
}

/// Un FLAC largo de 24 bits a 96 kHz. Comprimido ocupa poco en disco, pero
/// su PCM son cientos de megas: justo lo que revienta si alguien lo carga
/// entero.
fn long_file() -> Option<PathBuf> {
    let mut target = std::env::temp_dir();
    target.push("chilpa-memoria-24-96.flac");
    if target.is_file() {
        return Some(target);
    }
    let ok = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi"])
        .args(["-i", "sine=frequency=440:duration=300:sample_rate=96000"])
        .args(["-af", "aformat=sample_fmts=s32:channel_layouts=stereo"])
        .args(["-c:a", "flac", "-sample_fmt", "s32"])
        .arg(&target)
        .output()
        .ok()?
        .status
        .success();
    ok.then_some(target)
}

fn caps() -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![96_000],
        formats: vec![SampleFormat::S24In32, SampleFormat::S32],
        channels: vec![2],
        probed: true,
    }
}

#[test]
fn una_pista_de_cinco_minutos_no_se_carga_entera_en_memoria() {
    let Some(path) = long_file() else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    // 5 minutos de 24/96 estéreo son ~345 MB de PCM. Si alguien decodifica
    // la pista completa, se nota aquí y no en la máquina de alguien más.
    let antes = rss_mb();
    let mut sink = CountingSink(StreamFormat::new(SampleFormat::S24In32, 96_000, 2), 0);
    let played = play_file(&path, &caps(), Policy::BitPerfect, &StopSignal::new(), |format| {
        sink.0 = format;
        Ok(&mut sink)
    })
    .expect("debe reproducir");

    let after = rss_mb();
    let creció = after - antes;

    assert!(played.outcome.may_advance_queue(), "terminó bien");
    assert!(sink.1 > 96_000 * 290, "salieron {} frames", sink.1);
    assert!(
        creció < 60.0,
        "la memoria creció {:.0} MB reproduciendo; se está guardando la pista entera",
        creció
    );
    // El archivo se deja: generarlo cuesta más que borrarlo.
}
