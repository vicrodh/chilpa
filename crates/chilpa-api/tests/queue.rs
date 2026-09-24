//! The queue as the facade sees it: what is queued stays queued.
//!
//! Going back used to relaunch the queue with the tracks that were left, and
//! with that the earlier ones were lost: the second time "previous" was
//! asked for there was nowhere to go back to, and saving the list saved half
//! of it. This pins that the whole queue survives.

use std::io::Write;
use std::path::PathBuf;

/// Points the library at a temporary directory.
///
/// Without it the test writes into the library of whoever runs it: it puts
/// plays in and, what really hurts, migrates the schema to the version of
/// the code being tested. That already broke an old binary that was still
/// open.
fn other_library(dir: &std::path::Path) {
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
}

/// The capture output writes to a file, so this runs without sound and
/// without hardware.
const OUTPUT: &str = "file:capture";

fn folder(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-api-cola-{name}"));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A short WAV, with a constant tone.
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

/// Waits until the queue has reached at least that position.
///
/// No clock holds the capture output back: it delivers what it is given as
/// fast as it can, so a short queue can pass through several tracks between
/// two reads. That is why it waits for "at least" and not an exact position,
/// which would be a test that fails depending on the day.
fn wait_at_least(chilpa: &mut chilpa_api::Chilpa, index: usize) -> bool {
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < limit {
        if chilpa.poll().index >= index {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

#[test]
fn going_back_does_not_trim_the_queue() {
    let dir = folder("back");
    other_library(&dir);
    // Half second tracks: enough for the queue to advance on its own.
    for n in ["01.wav", "02.wav", "03.wav"] {
        wave(&dir, n, 22_050);
    }

    let mut chilpa = chilpa_api::Chilpa::new();
    let titles = chilpa.play_folder(&dir, OUTPUT, true);
    assert_eq!(titles.len(), 3, "all three were queued");

    assert!(wait_at_least(&mut chilpa, 2), "the queue advanced on its own");
    let before = chilpa.now_playing().index;
    chilpa.previous_track(OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, before - 1), "it played again");
    assert_eq!(chilpa.now_playing().queue_len, 3, "the queue is still whole");

    // And with the whole queue, saving it saves all three.
    let list = dir.join("cola.m3u");
    assert!(chilpa.save_queue(&list), "it was saved");
    let text = std::fs::read_to_string(&list).unwrap();
    for n in ["01.wav", "02.wav", "03.wav"] {
        assert!(text.contains(n), "{n} is missing from:\n{text}");
    }

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shuffle_and_repeat_are_remembered_by_the_facade() {
    let mut chilpa = chilpa_api::Chilpa::new();
    assert!(!chilpa.shuffle(), "it starts in order");
    assert_eq!(chilpa.repeat(), chilpa_api::Repeat::Off);
    chilpa.set_shuffle(true);
    chilpa.set_repeat(chilpa_api::Repeat::All);
    assert!(chilpa.shuffle());
    assert_eq!(chilpa.repeat(), chilpa_api::Repeat::All);
}

#[test]
fn enqueuing_with_nothing_playing_starts_the_queue() {
    let dir = folder("queue");
    other_library(&dir);
    let paths: Vec<String> = ["01.wav", "02.wav"]
        .iter()
        .map(|n| wave(&dir, n, 4_410).display().to_string())
        .collect();

    let mut chilpa = chilpa_api::Chilpa::new();
    let titles = chilpa.enqueue(&paths, OUTPUT, true);
    assert_eq!(titles.len(), 2, "queueing with nothing playing means playing");
    assert!(wait_at_least(&mut chilpa, 1), "the queue advanced");
    assert_eq!(chilpa.queue_titles().len(), 2);

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}
