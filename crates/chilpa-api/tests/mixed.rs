//! AutoMix: the queue reordered so the tempo does not jump.
//!
//! A single test, as in `playlists.rs`: it points `XDG_DATA_HOME` at a
//! temporary directory and that variable belongs to the whole process.

use std::io::Write;
use std::path::PathBuf;

const OUTPUT: &str = "capture";

/// A short WAV of silence. What matters here is the order, not the sound.
fn wave(dir: &PathBuf, name: &str) -> PathBuf {
    let p = dir.join(name);
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
fn the_mix_needs_tempos_and_reorders_once_it_has_them() {
    let dir = std::env::temp_dir().join("chilpa-api-mezcla");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));

    let paths: Vec<String> = ["a.wav", "b.wav", "c.wav", "d.wav"]
        .iter()
        .map(|n| wave(&dir, n).display().to_string())
        .collect();

    // The mix reads the tempos from the library, so the tracks have to be
    // scanned: playing a file does not record it.
    {
        let lib =
            chilpa_library::Library::open(&chilpa_library::Library::default_path()).unwrap();
        chilpa_library::scan_root(&lib, &dir).unwrap();
    }

    let mut chilpa = chilpa_api::Chilpa::new();

    // With no queue there is nothing to mix, and it is said.
    assert_eq!(chilpa.automix(), 0);
    assert!(chilpa.now_playing().error.contains("cola"));

    chilpa.play_tracks(&paths, OUTPUT, true);
    // With no analysis there is nothing either, and that is said too.
    assert_eq!(chilpa.automix(), 0, "sin tempos no se mezcla a ciegas");
    assert!(
        chilpa.now_playing().error.contains("tempo"),
        "dijo: {}",
        chilpa.now_playing().error
    );

    // The tempos are put in by hand, as if the analysis had already run.
    let lib = chilpa_library::Library::open(&chilpa_library::Library::default_path()).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    for (path, bpm) in paths.iter().zip([90.0f32, 140.0, 92.0, 138.0]) {
        let id = lib
            .track_by_path(source, std::path::Path::new(path))
            .unwrap()
            .expect("la pista quedó registrada en el escaneo");
        lib.save_features(id, "chilpa", 1, Some(bpm), Some(0.8), None, None).unwrap();
    }
    drop(lib);

    let mixed = chilpa.automix();
    assert_eq!(mixed, 4, "las cuatro entraron a la mezcla");
    assert!(!chilpa.shuffle(), "mezclar apaga el aleatorio");

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}
