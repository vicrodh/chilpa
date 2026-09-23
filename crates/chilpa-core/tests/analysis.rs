//! Analizar un archivo de verdad: decodificarlo y sacarle el pulso.
//!
//! La prueba del analizador solo, con muestras en memoria, vive en
//! `chilpa-analysis`. Esta comprueba el camino completo: abrir el archivo,
//! decodificarlo, mezclarlo a mono y contar.

use chilpa_core::analysis_job::analyze_file;
use std::io::Write;
use std::path::PathBuf;

/// Un WAV de 16 bits en estéreo con golpes a ese tempo.
fn pulse(name: &str, bpm: f32, seconds: u32) -> PathBuf {
    let rate = 44_100u32;
    let frames = rate * seconds;
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-analisis-{name}.wav"));

    let data = frames * 4;
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();

    let period = (rate as f32 * 60.0 / bpm) as usize;
    let tail = (rate / 33) as usize;
    for i in 0..frames as usize {
        let from_beat = i % period;
        let v = if from_beat < tail {
            let decay = 1.0 - from_beat as f32 / tail as f32;
            // Un golpe con algo de tono, para que no sea un clic puro.
            let tone = (2.0 * std::f32::consts::PI * 80.0 * i as f32 / rate as f32).sin();
            (decay * decay * tone * 20_000.0) as i16
        } else {
            0
        };
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    p
}

#[test]
fn a_file_beat_comes_out_through_the_whole_path() {
    let path = pulse("128", 128.0, 25);
    let features = analyze_file(&path).expect("se pudo leer y analizar");
    let bpm = features.bpm.expect("se le encontró pulso");
    assert!((bpm - 128.0).abs() < 3.0, "salió {bpm}");
    assert!(
        features.tempo_confidence.is_some_and(|c| c > 0.1),
        "confianza: {:?}",
        features.tempo_confidence
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_file_without_a_beat_is_analyzed_and_says_nothing() {
    // Silencio: se lee bien, pero no hay qué contar.
    let path = pulse("silencio", 128.0, 25);
    // Se sobrescribe el audio con ceros dejando el encabezado.
    let mut data = std::fs::read(&path).unwrap();
    for b in data.iter_mut().skip(44) {
        *b = 0;
    }
    std::fs::write(&path, &data).unwrap();

    let features = analyze_file(&path).expect("el archivo se leyó");
    assert_eq!(features.bpm, None, "no se inventó un tempo");
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_file_that_is_not_audio_brings_nothing_down() {
    let mut p = std::env::temp_dir();
    p.push("chilpa-analisis-basura.wav");
    std::fs::write(&p, b"esto no es un WAV").unwrap();
    assert!(analyze_file(&p).is_none());
    let _ = std::fs::remove_file(p);
}
