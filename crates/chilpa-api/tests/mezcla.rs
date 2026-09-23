//! AutoMix: la cola reordenada para que el tempo no dé saltos.
//!
//! Una sola prueba, como en `listas.rs`: apunta `XDG_DATA_HOME` a un
//! directorio temporal y esa variable es del proceso entero.

use std::io::Write;
use std::path::PathBuf;

const SALIDA: &str = "capture";

/// Un WAV corto de silencio. Lo que importa aquí es el orden, no el sonido.
fn onda(dir: &PathBuf, nombre: &str) -> PathBuf {
    let p = dir.join(nombre);
    let frames = 4_410u32;
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
    f.write_all(&vec![0u8; data as usize]).unwrap();
    p
}

#[test]
fn la_mezcla_necesita_tempos_y_reordena_cuando_los_hay() {
    let dir = std::env::temp_dir().join("chilpa-api-mezcla");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));

    let rutas: Vec<String> = ["a.wav", "b.wav", "c.wav", "d.wav"]
        .iter()
        .map(|n| onda(&dir, n).display().to_string())
        .collect();

    // La mezcla lee los tempos de la biblioteca, así que las pistas tienen
    // que estar escaneadas: reproducir un archivo no lo registra.
    {
        let lib =
            chilpa_library::Library::open(&chilpa_library::Library::default_path()).unwrap();
        chilpa_library::scan_root(&lib, &dir).unwrap();
    }

    let mut chilpa = chilpa_api::Chilpa::new();

    // Sin cola no hay nada que mezclar, y se dice.
    assert_eq!(chilpa.automix(), 0);
    assert!(chilpa.now_playing().error.contains("cola"));

    chilpa.play_tracks(&rutas, SALIDA, true);
    // Sin análisis tampoco, y también se dice.
    assert_eq!(chilpa.automix(), 0, "sin tempos no se mezcla a ciegas");
    assert!(
        chilpa.now_playing().error.contains("tempo"),
        "dijo: {}",
        chilpa.now_playing().error
    );

    // Se meten los tempos a mano, como si el análisis ya hubiera corrido.
    let lib = chilpa_library::Library::open(&chilpa_library::Library::default_path()).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    for (ruta, bpm) in rutas.iter().zip([90.0f32, 140.0, 92.0, 138.0]) {
        let id = lib
            .track_by_path(source, std::path::Path::new(ruta))
            .unwrap()
            .expect("la pista quedó registrada en el escaneo");
        lib.save_features(id, "chilpa", 1, Some(bpm), Some(0.8), None).unwrap();
    }
    drop(lib);

    let mezcladas = chilpa.automix();
    assert_eq!(mezcladas, 4, "las cuatro entraron a la mezcla");
    assert!(!chilpa.shuffle(), "mezclar apaga el aleatorio");

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}
