//! Leer tags de archivos de verdad, generados con ffmpeg.

use chilpa_tags::read;
use std::path::PathBuf;
use std::process::Command;

fn hay_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Un FLAC de un segundo con los tags que se le pidan.
fn archivo(nombre: &str, tags: &[(&str, &str)]) -> Option<PathBuf> {
    if !hay_ffmpeg() {
        return None;
    }
    let mut destino = std::env::temp_dir();
    destino.push(format!("chilpa-tags-{nombre}.flac"));
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"]);
    cmd.args(["-c:a", "flac"]);
    for (clave, valor) in tags {
        cmd.arg("-metadata").arg(format!("{clave}={valor}"));
    }
    cmd.arg(&destino);
    cmd.output().ok()?.status.success().then_some(destino)
}

#[test]
fn se_leen_los_campos_basicos_y_la_duracion() {
    let Some(path) = archivo(
        "basico",
        &[
            ("title", "Cancion de prueba"),
            ("artist", "Artista"),
            ("album", "Disco"),
            ("track", "3"),
            ("date", "2019-03-15"),
        ],
    ) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let info = read(&path).unwrap();
    assert_eq!(info.tags.title.as_deref(), Some("Cancion de prueba"));
    assert_eq!(info.tags.artist_line(), "Artista");
    assert_eq!(info.tags.album.as_deref(), Some("Disco"));
    assert_eq!(info.tags.track_number, Some(3));
    assert_eq!(info.tags.date.as_deref(), Some("2019-03-15"), "la fecha no se trunca");
    assert_eq!(info.tags.year(), Some(2019));
    assert!(info.duration.as_millis() >= 900, "duro {:?}", info.duration);
    assert_eq!(info.sample_rate_hz, Some(44_100));
    let _ = std::fs::remove_file(path);
}

#[test]
fn un_archivo_sin_tags_no_falla_y_reporta_sus_propiedades() {
    let Some(path) = archivo("sin-tags", &[]) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let info = read(&path).unwrap();
    assert!(info.tags.title.is_none());
    assert!(info.tags.artists.is_empty());
    assert_eq!(info.bit_depth, Some(16));
    let _ = std::fs::remove_file(path);
}

#[test]
fn el_genero_se_lee_como_lista() {
    let Some(path) = archivo("generos", &[("GENRE", "Rock")]) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let info = read(&path).unwrap();
    assert_eq!(info.tags.genres, vec!["Rock".to_string()]);
    let _ = std::fs::remove_file(path);
}
