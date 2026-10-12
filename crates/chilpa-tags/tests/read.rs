//! Reading tags from real files, generated with ffmpeg.

use chilpa_tags::read;
use std::path::PathBuf;
use std::process::Command;

fn has_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// A one-second FLAC carrying whatever tags it is given.
fn file(name: &str, tags: &[(&str, &str)]) -> Option<PathBuf> {
    if !has_ffmpeg() {
        return None;
    }
    let mut target = std::env::temp_dir();
    target.push(format!("chilpa-tags-{name}.flac"));
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"]);
    cmd.args(["-c:a", "flac"]);
    for (key, value) in tags {
        cmd.arg("-metadata").arg(format!("{key}={value}"));
    }
    cmd.arg(&target);
    cmd.output().ok()?.status.success().then_some(target)
}

#[test]
fn the_basic_fields_and_the_duration_are_read() {
    let Some(path) = file(
        "basic",
        &[
            ("title", "A test song"),
            ("artist", "Artist"),
            ("album", "Album"),
            ("track", "3"),
            ("disc", "2/3"),
            ("date", "2019-03-15"),
        ],
    ) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let info = read(&path).unwrap();
    assert_eq!(info.tags.title.as_deref(), Some("A test song"));
    assert_eq!(info.tags.artist_line(), "Artist");
    assert_eq!(info.tags.album.as_deref(), Some("Album"));
    assert_eq!(info.tags.track_number, Some(3));
    assert_eq!(info.tags.disc_number, Some(2));
    assert_eq!(info.tags.disc_total, Some(3));
    assert_eq!(info.tags.date.as_deref(), Some("2019-03-15"), "the date is not truncated");
    assert_eq!(info.tags.year(), Some(2019));
    assert!(info.duration.as_millis() >= 900, "it lasted {:?}", info.duration);
    assert_eq!(info.sample_rate_hz, Some(44_100));
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_flac_reports_disc_subtitle_and_work() {
    let Some(path) = file("disc-work", &[("DISCSUBTITLE", "Scene One"), ("WORK", "Das Rheingold")]) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let info = read(&path).unwrap();
    assert_eq!(info.tags.disc_subtitle.as_deref(), Some("Scene One"));
    assert_eq!(info.tags.work.as_deref(), Some("Das Rheingold"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_file_without_tags_does_not_fail_and_reports_its_properties() {
    let Some(path) = file("no-tags", &[]) else {
        eprintln!("no ffmpeg: skipped");
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
    let Some(path) = file("genres", &[("GENRE", "Rock")]) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let info = read(&path).unwrap();
    assert_eq!(info.tags.genres, vec!["Rock".to_string()]);
    let _ = std::fs::remove_file(path);
}
