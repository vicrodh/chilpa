//! Las listas guardadas, de punta a punta.
//!
//! Es una sola prueba a propósito: apunta `XDG_DATA_HOME` a un directorio
//! temporal para no escribir en la biblioteca de quien la corra, y esa
//! variable es del proceso entero. Con una sola prueba no hay con quién
//! pelearse.

use std::io::Write;
use std::path::PathBuf;

const OUTPUT: &str = "capture";

fn wave(dir: &PathBuf, name: &str, frames: u32) -> PathBuf {
    let p = dir.join(name);
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
        let v = (i % 3_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    p
}

#[test]
fn save_the_queue_play_it_and_delete_it() {
    let dir = std::env::temp_dir().join("chilpa-api-listas");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));

    let paths: Vec<String> = ["01.wav", "02.wav", "03.wav"]
        .iter()
        .map(|n| wave(&dir, n, 4_410).display().to_string())
        .collect();

    let mut chilpa = chilpa_api::Chilpa::new();
    assert!(chilpa.playlists().is_empty(), "una biblioteca nueva no trae listas");

    // Sin cola no hay nada que guardar, y se dice.
    assert!(chilpa.save_queue_as("Vacía").is_none());
    assert!(chilpa.now_playing().error.contains("cola"));

    chilpa.play_tracks(&paths, OUTPUT, true);
    // Y una lista sin nombre tampoco va.
    assert!(chilpa.save_queue_as("   ").is_none());

    let id = chilpa.save_queue_as("De la mañana").expect("se guardó");
    let lists = chilpa.playlists();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].name, "De la mañana");
    assert_eq!(lists[0].track_count, 3);
    assert_eq!(lists[0].provider, "local", "es de aquí");

    let titles = chilpa.play_saved_playlist(id, OUTPUT, true);
    assert_eq!(titles.len(), 3, "vuelve a sonar completa");
    assert_eq!(chilpa.queue_titles().len(), 3);

    chilpa.rename_playlist(id, "De la noche");
    assert_eq!(chilpa.playlists()[0].name, "De la noche");

    chilpa.delete_playlist(id);
    assert!(chilpa.playlists().is_empty());

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}
