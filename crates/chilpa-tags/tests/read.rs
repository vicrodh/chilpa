//! Leer tags de archivos de verdad, generados con ffmpeg.

use chilpa_tags::read;
use std::path::PathBuf;
use std::process::Command;

fn hay_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Un FLAC de un segundo con los tags que se le pidan.
fn file(name: &str, tags: &[(&str, &str)]) -> Option<PathBuf> {
    if !hay_ffmpeg() {
        return None;
    }
    let mut target = std::env::temp_dir();
    target.push(format!("chilpa-tags-{name}.flac"));
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"]);
    cmd.args(["-c:a", "flac"]);
    for (clave, value) in tags {
        cmd.arg("-metadata").arg(format!("{clave}={valor}"));
    }
    cmd.arg(&target);
    cmd.output().ok()?.status.success().then_some(target)
}

#[test]
fn the_basic_fields_and_the_duration_are_read() {
    let Some(path) = file(
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
fn a_file_without_tags_does_not_fail_and_reports_its_properties() {
    let Some(path) = file("sin-tags", &[]) else {
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
fn the_genre_is_read_as_a_list() {
    let Some(path) = file("generos", &[("GENRE", "Rock")]) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let info = read(&path).unwrap();
    assert_eq!(info.tags.genres, vec!["Rock".to_string()]);
    let _ = std::fs::remove_file(path);
}
