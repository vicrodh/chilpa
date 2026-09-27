//! Writing a file's tags does not change its music.
//!
//! Its own test binary: the capture file is one per machine, and the
//! library directory is one per process.

use chilpa_api::Chilpa;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const OUTPUT: &str = "file:capture";

/// Plays a file to its end and hands back what came out.
fn played(chilpa: &mut Chilpa, path: &Path) -> Vec<u8> {
    chilpa.play_file(path, OUTPUT, true);
    let limit = Instant::now() + Duration::from_secs(20);
    let mut started = false;
    while Instant::now() < limit {
        let now = chilpa.poll();
        started |= now.playing;
        if started && !now.playing {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(started, "it never started");
    chilpa.stop();
    std::fs::read(chilpa_core::capture_path()).unwrap()
}

fn until(chilpa: &mut Chilpa, state: &str) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        if chilpa.metadata_poll().state == state {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the editor never got to \"{state}\": {:?}", chilpa.metadata_view());
}

#[test]
fn the_music_is_the_same_after_writing_its_tags() {
    let dir = std::env::temp_dir().join("chilpa-api-metadata-audio");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("music/A record")).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));

    let file = dir.join("music/A record/01.flac");
    let made = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-c:a", "flac"])
        .args(["-metadata", "title=One", "-metadata", "artist=Someone", "-metadata", "album_artist=Someone", "-metadata", "album=A record"])
        .arg(&file)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !made {
        eprintln!("no ffmpeg: skipped");
        return;
    }

    let mut chilpa = Chilpa::new();
    chilpa.scan(&dir.join("music"));
    let before = played(&mut chilpa, &file);
    assert!(before.len() > 100_000, "two seconds came out: {} bytes", before.len());

    let album = chilpa.albums_sorted(chilpa_api::AlbumSort::Title, 10, 0)[0].id;
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    let track = chilpa.metadata_view().tracks[0].id;

    // what_is_playing_is_not_written_to
    chilpa.play_file(&file, OUTPUT, true);
    assert!(chilpa.metadata_set_mode("files"));
    chilpa.metadata_set_track(track, "title", "A much longer title than the one the file was made with");
    assert!(!chilpa.metadata_save(), "the file is loaded in the player");
    assert!(!chilpa.metadata_view().error.is_empty());
    chilpa.stop();
    // Stopping is taken in by the next poll.
    let limit = Instant::now() + Duration::from_secs(5);
    while chilpa.poll().playing && Instant::now() < limit {
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(chilpa.metadata_save(), "{}", chilpa.metadata_view().error);
    until(&mut chilpa, "closed");
    assert!(chilpa.metadata_view().error.is_empty(), "{}", chilpa.metadata_view().error);
    assert_eq!(
        chilpa_tags::read(&file).unwrap().tags.title.as_deref(),
        Some("A much longer title than the one the file was made with")
    );

    let after = played(&mut chilpa, &file);
    assert_eq!(after.len(), before.len());
    assert!(after == before, "what comes out is what came out before, sample for sample");
    let _ = std::fs::remove_dir_all(dir);
}
