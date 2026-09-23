//! Analizar un archivo de verdad: decodificarlo y sacarle el pulso.
//!
//! La prueba del analizador solo, con muestras en memoria, vive en
//! `chilpa-analysis`. Esta comprueba el camino completo: abrir el archivo,
//! decodificarlo, mezclarlo a mono y contar.

use chilpa_core::analysis_job::analizar_archivo;
use std::io::Write;
use std::path::PathBuf;

/// Un WAV de 16 bits en estéreo con golpes a ese tempo.
fn pulso(nombre: &str, bpm: f32, segundos: u32) -> PathBuf {
    let rate = 44_100u32;
    let frames = rate * segundos;
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-analisis-{nombre}.wav"));

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

    let periodo = (rate as f32 * 60.0 / bpm) as usize;
    let cola = (rate / 33) as usize;
    for i in 0..frames as usize {
        let desde_golpe = i % periodo;
        let v = if desde_golpe < cola {
            let caida = 1.0 - desde_golpe as f32 / cola as f32;
            // Un golpe con algo de tono, para que no sea un clic puro.
            let tono = (2.0 * std::f32::consts::PI * 80.0 * i as f32 / rate as f32).sin();
            (caida * caida * tono * 20_000.0) as i16
        } else {
            0
        };
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    p
}

#[test]
fn el_pulso_de_un_archivo_sale_por_el_camino_completo() {
    let ruta = pulso("128", 128.0, 25);
    let features = analizar_archivo(&ruta).expect("se pudo leer y analizar");
    let bpm = features.bpm.expect("se le encontró pulso");
    assert!((bpm - 128.0).abs() < 3.0, "salió {bpm}");
    assert!(
        features.tempo_confidence.is_some_and(|c| c > 0.1),
        "confianza: {:?}",
        features.tempo_confidence
    );
    let _ = std::fs::remove_file(ruta);
}

#[test]
fn un_archivo_sin_pulso_se_analiza_y_no_dice_nada() {
    // Silencio: se lee bien, pero no hay qué contar.
    let ruta = pulso("silencio", 128.0, 25);
    // Se sobrescribe el audio con ceros dejando el encabezado.
    let mut datos = std::fs::read(&ruta).unwrap();
    for b in datos.iter_mut().skip(44) {
        *b = 0;
    }
    std::fs::write(&ruta, &datos).unwrap();

    let features = analizar_archivo(&ruta).expect("el archivo se leyó");
    assert_eq!(features.bpm, None, "no se inventó un tempo");
    let _ = std::fs::remove_file(ruta);
}

#[test]
fn un_archivo_que_no_es_audio_no_tumba_nada() {
    let mut p = std::env::temp_dir();
    p.push("chilpa-analisis-basura.wav");
    std::fs::write(&p, b"esto no es un WAV").unwrap();
    assert!(analizar_archivo(&p).is_none());
    let _ = std::fs::remove_file(p);
}
