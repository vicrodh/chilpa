//! Sidecars left unread. The switch is the process's own, so this is the
//! only test of its binary: nothing else reads sidecars while it is on.
use chilpa_library::sidecar::{set_ignored, store, Home, Override, Sidecar};
use chilpa_library::{scan_root, Library};
use std::path::Path;
use std::process::Command;

/// A tagged FLAC. False without ffmpeg.
fn tagged(target: &Path, title: &str) -> bool {
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
        .args(["-metadata", &format!("title={title}"), "-metadata", "artist=File artist", "-metadata", "album=File album", "-metadata", "track=1"])
        .arg(target)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn title(lib: &Library) -> String {
    lib.connection().query_row("SELECT title FROM tracks", [], |r| r.get(0)).unwrap()
}

#[test]
fn an_ignored_sidecar_leaves_the_file_s_word_and_is_read_again_when_it_counts() {
    let dir = std::env::temp_dir().join("chilpa-sidecar-ignored");
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("Record/01.flac");
    if !tagged(&file, "From the file") {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let home = Home::of(&file.to_string_lossy(), None);
    let mut sidecar = Sidecar::new(0);
    sidecar.track_mut("01.flac").title = Override::Value("From the sidecar".into());
    store(&home, &sidecar, 1).unwrap();
    let lib = Library::in_memory().unwrap();

    scan_root(&lib, &dir).unwrap();
    assert_eq!(title(&lib), "From the sidecar");

    set_ignored(true);
    let unread = scan_root(&lib, &dir);
    let while_ignored = title(&lib);
    set_ignored(false);
    unread.unwrap();
    assert_eq!(while_ignored, "From the file", "the scan reads the folder again without it");
    assert!(home.sidecar().exists(), "and the sidecar stays where it was");

    scan_root(&lib, &dir).unwrap();
    assert_eq!(title(&lib), "From the sidecar", "read again once it counts");
    let _ = std::fs::remove_dir_all(dir);
}
