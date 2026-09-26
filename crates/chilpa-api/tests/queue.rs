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

fn queue_names(chilpa: &chilpa_api::Chilpa) -> Vec<String> {
    chilpa
        .queue_order()
        .into_iter()
        .map(|index| {
            std::path::Path::new(&chilpa.queue_paths()[index])
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
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

#[test]
fn play_next_puts_the_tracks_right_after_the_one_playing() {
    let dir = folder("next");
    other_library(&dir);
    // Long enough to still be on the first while this runs, and paused.
    let queued: Vec<String> =
        ["01.wav", "02.wav", "03.wav"].iter().map(|n| wave(&dir, n, 441_000).display().to_string()).collect();
    let extra: Vec<String> = ["x1.wav", "x2.wav"].iter().map(|n| wave(&dir, n, 4_410).display().to_string()).collect();

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&queued, OUTPUT, true);
    chilpa.set_paused(true);
    assert!(wait_at_least(&mut chilpa, 0));
    let playing = chilpa.now_playing().index;
    chilpa.play_next(&extra, OUTPUT, true);

    let names: Vec<String> = chilpa
        .queue_order()
        .into_iter()
        .map(|i| std::path::Path::new(&chilpa.queue_paths()[i]).file_name().unwrap().to_string_lossy().to_string())
        .collect();
    let mut expected = vec![format!("0{}.wav", playing + 1), "x1.wav".into(), "x2.wav".into()];
    expected.extend((playing + 2..=3).map(|n| format!("0{n}.wav")));
    assert_eq!(names, expected, "the two go right after the one playing, the rest after them");

    // Taking one out takes it out of the order too.
    let x1 = chilpa.queue_order()[1];
    assert!(chilpa.remove_from_queue(x1));
    assert!(!chilpa.queue_order().contains(&x1));

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stop_after_finishes_the_marked_track_on_capture_and_stops() {
    let dir = folder("stop-after");
    other_library(&dir);
    let paths: Vec<String> = ["01.wav", "02.wav"]
        .iter()
        .map(|name| wave(&dir, name, 441_000).display().to_string())
        .collect();

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    chilpa.set_paused(true);
    assert!(chilpa.set_stop_after(Some(0)));
    chilpa.set_paused(false);

    let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while chilpa.poll().playing && std::time::Instant::now() < limit {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!chilpa.now_playing().playing, "the mark stopped playback");
    assert_eq!(chilpa.now_playing().index, 0, "the second track never started");
    assert_eq!(chilpa.stop_after(), None, "the mark was consumed");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_sleep_timer_stops_a_paused_capture_queue() {
    let dir = folder("timer");
    other_library(&dir);
    let path = wave(&dir, "01.wav", 441_000).display().to_string();

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&[path], OUTPUT, true);
    chilpa.set_paused(true);
    chilpa.set_sleep_timer(Some(std::time::Duration::from_millis(30)));

    let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while chilpa.poll().playing && std::time::Instant::now() < limit {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!chilpa.now_playing().playing, "the timer stopped playback");
    assert_eq!(chilpa.sleep_timer_remaining(), None, "the timer is one-shot");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn play_later_is_a_named_group_after_play_next() {
    let dir = folder("later");
    other_library(&dir);
    let make = |names: &[&str], frames| {
        names.iter().map(|name| wave(&dir, name, frames).display().to_string()).collect::<Vec<_>>()
    };
    let original = make(&["01.wav", "02.wav", "03.wav"], 441_000);
    let next = make(&["n1.wav", "n2.wav"], 4_410);
    let later = make(&["l1.wav", "l2.wav"], 4_410);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&original, OUTPUT, true);
    chilpa.set_paused(true);
    chilpa.play_next(&next, OUTPUT, true);
    let key = chilpa.play_later(&later, "Another album", OUTPUT, true).expect("the group was made");

    let names: Vec<String> = chilpa
        .queue_order()
        .into_iter()
        .map(|index| {
            std::path::Path::new(&chilpa.queue_paths()[index])
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, ["01.wav", "n1.wav", "n2.wav", "l1.wav", "l2.wav", "02.wav", "03.wav"]);
    assert_eq!(chilpa.queue_groups()[0].label, "Another album");

    let before = chilpa.queue_order();
    assert!(chilpa.collapse_queue_group(key, true));
    assert_eq!(chilpa.queue_order(), before, "folding is only presentation");
    assert!(chilpa.remove_queue_group(key));
    assert!(chilpa.queue_order().iter().all(|index| !later.contains(&chilpa.queue_paths()[*index])));

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn queued_rows_can_be_moved_without_moving_the_playing_track() {
    let dir = folder("move");
    other_library(&dir);
    let paths: Vec<String> = ["01.wav", "02.wav", "03.wav", "04.wav"]
        .iter()
        .map(|name| wave(&dir, name, 441_000).display().to_string())
        .collect();
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    chilpa.set_paused(true);

    assert!(chilpa.move_queued(&[2, 3], 1));
    assert_eq!(queue_names(&chilpa), ["01.wav", "03.wav", "04.wav", "02.wav"]);
    assert!(!chilpa.move_queued(&[0], 3), "the playing row stays put");
    assert_eq!(queue_names(&chilpa), ["01.wav", "03.wav", "04.wav", "02.wav"]);

    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}
