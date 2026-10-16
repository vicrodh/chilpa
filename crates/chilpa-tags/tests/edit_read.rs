//! What the editor reads from real files, generated with ffmpeg.

use chilpa_tags::edit::{inspect, snapshot, Refusal};
use std::path::{Path, PathBuf};
use std::process::Command;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chilpa-edit-read-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A one-second FLAC carrying those tags. `None` without ffmpeg.
fn tone(dir: &Path, name: &str, tags: &[(&str, &str)]) -> Option<PathBuf> {
    let target = dir.join(format!("{name}.flac"));
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"]);
    for (key, value) in tags {
        cmd.arg("-metadata").arg(format!("{key}={value}"));
    }
    cmd.arg(&target);
    cmd.output().ok()?.status.success().then_some(target)
}

/// What a file is on disk: its size and when it last changed.
fn state(path: &Path) -> (u64, std::time::SystemTime) {
    let meta = std::fs::metadata(path).unwrap();
    (meta.len(), meta.modified().unwrap())
}

#[test]
fn a_full_date_is_reported_whole() {
    let dir = folder("date");
    let Some(path) = tone(&dir, "a", &[("date", "2019-03-15")]) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    assert_eq!(snapshot(&path).unwrap().date.as_deref(), Some("2019-03-15"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_file_without_tags_is_an_empty_snapshot() {
    let dir = folder("empty");
    let Some(path) = tone(&dir, "a", &[]) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let said = snapshot(&path).unwrap();
    assert_eq!(said.title, None);
    assert!(said.artist_credit.is_empty() && said.genres.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_missing_file_is_refused_and_the_rest_are_inspected() {
    let dir = folder("missing");
    let Some(path) = tone(&dir, "a", &[("title", "One")]) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let layers = inspect(&[dir.join("gone.flac"), path]);
    assert_eq!(layers.files, 2);
    assert_eq!(layers.refusals, vec![Some(Refusal::Missing), None]);
    assert!(!layers.all_writable());
    assert_eq!(layers.layers.len(), 1, "the one that is there was looked at");
    assert_eq!(layers.layers[0].name, "Vorbis comments");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_dsf_is_refused_as_dsd() {
    let dir = folder("dsd");
    let path = dir.join("a.dsf");
    std::fs::write(&path, b"not really a dsf").unwrap();
    assert_eq!(inspect(&[path]).refusals, vec![Some(Refusal::Dsd)]);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(target_family = "unix")]
#[test]
fn a_read_only_file_is_refused_as_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = folder("readonly");
    let Some(path) = tone(&dir, "a", &[("title", "One")]) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    // Root opens anything for writing, and a test run as root proves nothing.
    if std::fs::OpenOptions::new().write(true).open(&path).is_ok() {
        eprintln!("running as someone who can write anywhere: skipped");
        return;
    }
    assert_eq!(inspect(&[path.clone()]).refusals, vec![Some(Refusal::ReadOnly)]);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn inspecting_changes_no_file() {
    let dir = folder("untouched");
    let Some(path) = tone(&dir, "a", &[("title", "One")]) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let before = state(&path);
    let _ = inspect(&[path.clone()]);
    let _ = snapshot(&path);
    assert_eq!(state(&path), before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_same_file_twice_is_looked_at_once() {
    let dir = folder("twice");
    let Some(path) = tone(&dir, "a", &[("title", "One")]) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let layers = inspect(&[path.clone(), path]);
    assert_eq!(layers.files, 1);
    assert_eq!(layers.refusals.len(), 2, "and answered for both");
    let _ = std::fs::remove_dir_all(dir);
}
