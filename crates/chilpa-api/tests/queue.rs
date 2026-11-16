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
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
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
    wave_at(dir, name, frames, 44_100)
}

/// The same, at a sample rate of its own.
fn wave_at(dir: &PathBuf, name: &str, frames: u32, rate: u32) -> PathBuf {
    let p = dir.join(name);
    let data = frames * 4;
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 4).to_le_bytes()).unwrap();
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

#[test]
fn the_upcoming_queue_and_its_groups_survive_a_session() {
    let dir = folder("persist");
    other_library(&dir);
    let original: Vec<String> = ["01.wav", "02.wav", "03.wav"]
        .iter()
        .map(|name| wave(&dir, name, 441_000).display().to_string())
        .collect();
    let later: Vec<String> = ["l1.wav", "l2.wav"]
        .iter()
        .map(|name| wave(&dir, name, 4_410).display().to_string())
        .collect();

    {
        let mut chilpa = chilpa_api::Chilpa::new();
        chilpa.play_tracks(&original, OUTPUT, true);
        chilpa.set_paused(true);
        let key = chilpa.play_later(&later, "Saved album", OUTPUT, true).unwrap();
        assert!(chilpa.collapse_queue_group(key, true));
    }

    {
        let mut restored = chilpa_api::Chilpa::new();
        assert!(restored.now_playing().playing && restored.now_playing().paused);
        restored.poll();
        assert!(restored.now_playing().paused, "polling does not turn a restored queue into a phantom playing one");
        assert_eq!(queue_names(&restored), ["01.wav", "l1.wav", "l2.wav", "02.wav", "03.wav"]);
        assert_eq!(restored.queue_groups()[0].label, "Saved album");
        assert!(restored.queue_groups()[0].collapsed);
        restored.enqueue_here(&[original[1].clone()]);
        restored.play_next_here(&[original[2].clone()]);
        assert!(restored.now_playing().paused, "editing a restored queue must not start it");
        assert_eq!(queue_names(&restored), ["01.wav", "03.wav", "l1.wav", "l2.wav", "02.wav", "03.wav", "02.wav"]);
        restored.use_output(OUTPUT, false);
        restored.set_paused(false);
        assert!(restored.now_playing().playing && !restored.now_playing().paused, "resume starts the restored queue");
        restored.clear_queue();
    }
    let empty = chilpa_api::Chilpa::new();
    assert!(empty.queue_paths().is_empty(), "clearing also clears the saved session");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn queue_origins_survive_restart_for_duplicate_paths_and_insertions() {
    use chilpa_api::{navigation::{Route, Screen}, queue_origin::QueueOrigin};
    let dir = folder("origins");
    other_library(&dir);
    let path = wave(&dir, "same.wav", 4_410_000).display().to_string();
    let album = QueueOrigin { screen: "album".into(), id: 42, ..Default::default() };
    let search = QueueOrigin { screen: "search".into(), query: "Exact artist".into(), source: "one,two".into(), ..Default::default() };
    {
        let mut app = chilpa_api::Chilpa::new();
        app.set_browse_route(&Route { screen: Screen::Album, id: 42, key: String::new() });
        app.play_tracks(&[path.clone()], OUTPUT, true);
        app.set_paused(true);
        app.with_queue_origin(Some(search.clone()), |app| app.play_next_here(&[path.clone()]));
        app.set_browse_route(&Route { screen: Screen::Playlist, id: 9, key: String::new() });
        app.enqueue_here(&[path.clone()]);
    }
    let saved = dir.join("data/chilpa/listenlist.json");
    let document: serde_json::Value = serde_json::from_slice(&std::fs::read(saved).unwrap()).unwrap();
    let active = document["active"].as_u64().unwrap();
    let buffer = document["buffers"].as_array().unwrap().iter().find(|b| b["id"] == active).unwrap();
    assert_eq!(buffer["paths"].as_array().unwrap().len(), 3);
    assert_eq!(buffer["origins"][0], serde_json::to_value(&album).unwrap());
    assert_eq!(buffer["origins"][1], serde_json::to_value(&search).unwrap());
    assert_eq!(buffer["origins"][2]["screen"], "playlist");
    assert_eq!(buffer["origins"][2]["id"], 9);
    let mut restored = chilpa_api::Chilpa::new();
    assert!(restored.now_playing().paused);
    assert_eq!(serde_json::from_str::<QueueOrigin>(&restored.playing_origin_json()).unwrap(), album);
    restored.clear_queue();
    drop(restored);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_queue_that_ran_out_shows_nothing_playing_but_keeps_its_tracks() {
    let dir = folder("ran-out");
    other_library(&dir);
    let paths: Vec<String> = ["01.wav", "02.wav"]
        .iter()
        .map(|name| wave(&dir, name, 22_050).display().to_string())
        .collect();

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 1), "the queue reached its last track");
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while chilpa.poll().playing && std::time::Instant::now() < limit {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let now = chilpa.now_playing();
    assert!(!now.playing, "the queue ended");
    assert_eq!(now.path, "", "the last track is not kept on the player's face");
    assert_eq!(now.title, "");
    assert_eq!(now.seconds, 0.0);
    assert_eq!(now.queue_len, 2, "the queue itself stays");
    assert_eq!(chilpa.queue_paths().len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_stop_after_mark_leaves_the_track_it_stopped_on_in_the_player() {
    let dir = folder("marked");
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
    assert!(!chilpa.now_playing().playing);
    assert!(chilpa.now_playing().path.ends_with("01.wav"), "a mark is not the queue running out");
    let _ = std::fs::remove_dir_all(dir);
}

// A track that cannot be opened -- a file that is gone, a server item that is
// not there -- is skipped and the queue goes on. A device that refuses the
// format is not a missing track, and stops it as before.

/// The messages are read in English whatever the machine speaks.
fn english() {
    std::env::set_var("LC_ALL", "C");
}

/// Makes the capture take frames no faster than they would sound, so a
/// track lasts long enough to be seen and to be deleted under the player.
struct Realtime;

impl Realtime {
    fn on() -> Self {
        std::env::set_var("CHILPA_CAPTURE_REALTIME", "1");
        Realtime
    }
}

impl Drop for Realtime {
    fn drop(&mut self) {
        std::env::remove_var("CHILPA_CAPTURE_REALTIME");
    }
}

/// A fifth of a second at 44.1 kHz.
const SHORT: u32 = 8_820;

fn strings(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|path| path.display().to_string()).collect()
}

fn wait_until_stopped(chilpa: &mut chilpa_api::Chilpa) -> bool {
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < limit {
        if !chilpa.poll().playing {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

/// The positions that played, in order, until playback stops or the time
/// runs out. A position is written once each time it comes up.
fn heard(chilpa: &mut chilpa_api::Chilpa, for_at_most: std::time::Duration) -> Vec<usize> {
    let limit = std::time::Instant::now() + for_at_most;
    let mut heard = Vec::new();
    while std::time::Instant::now() < limit {
        let now = chilpa.poll();
        if !now.playing {
            break;
        }
        if heard.last() != Some(&now.index) {
            heard.push(now.index);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    heard
}

/// Waits until the player has said the first track started.
fn wait_started(chilpa: &mut chilpa_api::Chilpa) -> bool {
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < limit {
        if chilpa.poll().seconds > 0.0 {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

#[test]
fn a_missing_track_in_the_middle_is_skipped_and_the_queue_goes_on() {
    let dir = folder("skip-middle");
    other_library(&dir);
    english();
    let paths = strings(&[wave(&dir, "01.wav", 4_410), dir.join("02.wav"), wave(&dir, "03.wav", 4_410)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 2), "the track after the missing one started");
    assert!(wait_until_stopped(&mut chilpa));
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track");
    assert_eq!(chilpa.queue_paths().len(), 3, "the missing track stays queued");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_missing_first_track_starts_the_next_one() {
    let dir = folder("skip-first");
    other_library(&dir);
    english();
    let paths = strings(&[dir.join("01.wav"), wave(&dir, "02.wav", 4_410), wave(&dir, "03.wav", 4_410)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 2), "the queue played on from the second track");
    assert!(wait_until_stopped(&mut chilpa));
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_missing_last_track_ends_the_queue_as_if_it_ran_out() {
    let dir = folder("skip-last");
    other_library(&dir);
    english();
    let paths = strings(&[wave(&dir, "01.wav", 4_410), wave(&dir, "02.wav", 4_410), dir.join("03.wav")]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 1));
    assert!(wait_until_stopped(&mut chilpa));
    let now = chilpa.now_playing();
    assert_eq!(now.path, "", "the queue ran out: nothing is left on the player's face");
    assert_eq!(now.error, "Skipped 1 unavailable track", "the notice stays once the queue ends");
    assert_eq!(now.queue_len, 3);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_queue_with_nothing_playable_stops_after_one_pass() {
    let dir = folder("skip-all");
    other_library(&dir);
    english();
    let paths = strings(&[dir.join("01.wav"), dir.join("02.wav"), dir.join("03.wav")]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.set_repeat(chilpa_api::Repeat::All);
    chilpa.play_tracks(&paths, OUTPUT, true);
    let limit = std::time::Instant::now() + std::time::Duration::from_millis(300);
    while std::time::Instant::now() < limit {
        let now = chilpa.poll();
        assert!(!now.playing, "nothing was started");
        assert_eq!(now.error, "Skipped 3 unavailable tracks", "each track is counted once");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(chilpa.queue_paths().len(), 3, "the queue stays");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn repeat_all_plays_around_a_broken_track_without_spinning_on_it() {
    let dir = folder("skip-repeat-all");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let paths = strings(&[wave(&dir, "01.wav", SHORT), dir.join("02.wav"), wave(&dir, "03.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.set_repeat(chilpa_api::Repeat::All);
    chilpa.play_tracks(&paths, OUTPUT, true);
    let heard = heard(&mut chilpa, std::time::Duration::from_millis(1_500));
    assert!(heard.len() >= 4, "it went round more than once: {heard:?}");
    assert!(!heard.contains(&1), "the missing track never came up: {heard:?}");
    assert!(chilpa.now_playing().playing, "it is still playing");
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track", "counted once, not once a lap");
    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn repeat_one_on_a_missing_track_moves_on_and_repeats_the_next() {
    let dir = folder("skip-repeat-one");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let paths = strings(&[dir.join("01.wav"), wave(&dir, "02.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.set_repeat(chilpa_api::Repeat::One);
    chilpa.play_tracks(&paths, OUTPUT, true);
    let heard = heard(&mut chilpa, std::time::Duration::from_millis(800));
    assert_eq!(heard, [1], "only the track that plays, repeating");
    assert!(chilpa.now_playing().playing);
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track");
    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_deleted_under_repeat_one_is_left_for_the_next() {
    let dir = folder("skip-repeat-one-deleted");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let first = wave(&dir, "01.wav", SHORT);
    let paths = strings(&[first.clone(), wave(&dir, "02.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    // Without gapless nothing opens the repeat ahead of time, so the player
    // opens the deleted file again and fails on it.
    chilpa.set_gapless(false);
    chilpa.set_repeat(chilpa_api::Repeat::One);
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_started(&mut chilpa));
    std::fs::remove_file(&first).unwrap();
    let heard = heard(&mut chilpa, std::time::Duration::from_millis(1_200));
    assert_eq!(heard.last(), Some(&1), "it moved on to the next track: {heard:?}");
    assert!(chilpa.now_playing().playing, "and repeats that one");
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track");
    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_deleted_after_it_was_checked_is_skipped_when_it_fails() {
    let dir = folder("skip-deleted");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let second = wave(&dir, "02.wav", SHORT);
    let paths = strings(&[wave(&dir, "01.wav", SHORT * 2), second.clone(), wave(&dir, "03.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.set_gapless(false);
    chilpa.play_tracks(&paths, OUTPUT, true);
    // The first track has started, so the one after it was already looked
    // at and found there; the player only learns it is gone when it opens it.
    assert!(wait_started(&mut chilpa));
    std::fs::remove_file(&second).unwrap();
    assert!(wait_at_least(&mut chilpa, 2), "the track after the deleted one started");
    assert!(wait_until_stopped(&mut chilpa));
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_stop_mark_on_a_missing_track_stops_before_it() {
    let dir = folder("skip-stop-after");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let paths = strings(&[wave(&dir, "01.wav", SHORT), dir.join("02.wav"), wave(&dir, "03.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(chilpa.set_stop_after(Some(1)));
    let heard = heard(&mut chilpa, std::time::Duration::from_secs(5));
    assert_eq!(heard, [0], "what follows the mark never plays");
    let now = chilpa.now_playing();
    assert!(!now.playing);
    assert!(now.path.ends_with("01.wav"), "a mark is not the queue running out");
    assert_eq!(now.error, "Skipped 1 unavailable track");
    assert_eq!(chilpa.stop_after(), None, "the mark was consumed");
    let (rows, _) = chilpa.listen_rows(0, "", false);
    let available: Vec<bool> = rows.iter().filter_map(|row| match row {
        chilpa_api::TrackEntry::Track(track) => Some(track.available),
        _ => None,
    }).collect();
    assert_eq!(available, [true, false, true], "the queue dims the missing row");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_shuffled_order_is_kept_around_a_missing_track() {
    let dir = folder("skip-shuffle");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let paths: Vec<PathBuf> = ["01.wav", "02.wav", "03.wav", "04.wav", "05.wav"].iter().map(|name| wave(&dir, name, SHORT)).collect();

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.set_shuffle(true);
    chilpa.play_tracks(&strings(&paths), OUTPUT, true);
    let order = chilpa.queue_order();
    assert_eq!(order.len(), 5);
    // The third in the shuffled order goes away before it comes up.
    let gone = order[2];
    std::fs::remove_file(&paths[gone]).unwrap();
    let heard = heard(&mut chilpa, std::time::Duration::from_secs(5));
    let expected: Vec<usize> = order.iter().copied().filter(|&index| index != gone).collect();
    assert_eq!(heard, expected, "the shuffled order, without the missing track");
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_rate_the_output_refuses_stops_the_queue_instead_of_skipping() {
    let dir = folder("skip-device");
    other_library(&dir);
    english();
    // The capture takes no 7 kHz and there is no resampler: the output
    // refuses it, which says nothing about the file.
    let paths = strings(&[wave(&dir, "01.wav", 4_410), wave_at(&dir, "02.wav", 700, 7_000), wave(&dir, "03.wav", 4_410)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_until_stopped(&mut chilpa));
    std::thread::sleep(std::time::Duration::from_millis(200));
    let now = chilpa.poll();
    assert!(!now.playing, "nothing was relaunched");
    assert!(now.index < 2, "the track after the refused one never started");
    assert!(!now.error.is_empty() && !now.error.starts_with("Skipped"), "the output's own message: {}", now.error);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn several_skips_are_said_in_one_notice() {
    let dir = folder("skip-count");
    other_library(&dir);
    english();
    let paths = strings(&[wave(&dir, "01.wav", 4_410), dir.join("02.wav"), dir.join("03.wav"), wave(&dir, "04.wav", 4_410)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 3), "the last track started");
    assert!(wait_until_stopped(&mut chilpa));
    assert_eq!(chilpa.now_playing().error, "Skipped 2 unavailable tracks");
    let _ = std::fs::remove_dir_all(dir);
}

// A track that is there but cannot be played -- the decoder refuses it, or
// it breaks partway -- is passed over by the player itself, which says so,
// and it counts in the same notice as a missing one.

/// A file with an audio name and nothing in it.
fn empty_file(dir: &PathBuf, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"").unwrap();
    path
}

/// A file with an audio name and bytes that are no audio at all.
fn garbage_file(dir: &PathBuf, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"this is text and no audio at all. ".repeat(256)).unwrap();
    path
}

/// A second of two channel DSD64 silence as a DSF, cut halfway through its
/// audio: it opens, plays its first half and breaks.
fn broken_dsf(dir: &PathBuf, name: &str) -> PathBuf {
    let (rate, channels, block) = (2_822_400u64, 2u32, 4096u32);
    let per_channel = ((rate / 8) as usize).div_ceil(block as usize) * block as usize;
    let data_len = per_channel * channels as usize;
    let data_chunk = 12 + data_len as u64;
    let mut out = Vec::new();
    out.extend_from_slice(b"DSD ");
    out.extend_from_slice(&28u64.to_le_bytes());
    out.extend_from_slice(&(28 + 52 + data_chunk).to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&52u64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&(rate as u32).to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_chunk.to_le_bytes());
    out.resize(out.len() + data_len / 2, 0x69);
    let path = dir.join(name);
    std::fs::write(&path, out).unwrap();
    path
}

/// Which rows of the playing queue, the ones already played included, are
/// shown as playable.
fn available_rows(chilpa: &mut chilpa_api::Chilpa) -> Vec<bool> {
    let state: serde_json::Value = serde_json::from_str(&chilpa.listen_json()).unwrap();
    let active: u64 = state["active"].as_str().unwrap().parse().unwrap();
    let (rows, _) = chilpa.listen_rows(active, "", true);
    rows.iter().filter_map(|row| match row {
        chilpa_api::TrackEntry::Track(track) => Some(track.available),
        _ => None,
    }).collect()
}

#[test]
fn a_file_the_decoder_refuses_is_skipped_and_the_queue_goes_on() {
    let dir = folder("refused-middle");
    other_library(&dir);
    english();
    let paths = strings(&[wave(&dir, "01.wav", 4_410), empty_file(&dir, "02.wav"), wave(&dir, "03.wav", 4_410)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 2), "the track after the refused one started");
    assert!(wait_until_stopped(&mut chilpa));
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track");
    assert_eq!(available_rows(&mut chilpa), [true, false, true], "the queue dims the refused row");
    assert_eq!(chilpa.queue_paths().len(), 3, "the refused track stays queued");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_first_track_that_is_not_audio_starts_the_next_one() {
    let dir = folder("refused-first");
    other_library(&dir);
    english();
    let paths = strings(&[garbage_file(&dir, "01.flac"), wave(&dir, "02.wav", 4_410), wave(&dir, "03.wav", 4_410)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 2), "the queue played on from the second track");
    assert!(wait_until_stopped(&mut chilpa));
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_refused_last_track_ends_the_queue_as_if_it_ran_out() {
    let dir = folder("refused-last");
    other_library(&dir);
    english();
    let paths = strings(&[wave(&dir, "01.wav", 4_410), wave(&dir, "02.wav", 4_410), garbage_file(&dir, "03.flac")]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 1));
    assert!(wait_until_stopped(&mut chilpa));
    let now = chilpa.now_playing();
    assert_eq!(now.path, "", "the queue ran out: nothing is left on the player's face");
    assert_eq!(now.error, "Skipped 1 unavailable track", "the notice stays once the queue ends");
    assert_eq!(now.queue_len, 3);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_that_breaks_partway_is_skipped_and_the_queue_goes_on() {
    let dir = folder("breaks-first");
    other_library(&dir);
    english();
    let paths = strings(&[broken_dsf(&dir, "01.dsf"), wave(&dir, "02.wav", 4_410)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 1), "the track after the broken one started");
    assert!(wait_until_stopped(&mut chilpa));
    let now = chilpa.now_playing();
    assert_eq!(now.error, "Skipped 1 unavailable track");
    assert_eq!(now.path, "", "the queue ran out after its last track");
    assert_eq!(available_rows(&mut chilpa), [false, true]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_last_track_that_breaks_partway_ends_the_queue_as_if_it_ran_out() {
    let dir = folder("breaks-last");
    other_library(&dir);
    english();
    let paths = strings(&[wave(&dir, "01.wav", 4_410), broken_dsf(&dir, "02.dsf")]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 1));
    assert!(wait_until_stopped(&mut chilpa));
    let now = chilpa.now_playing();
    assert_eq!(now.path, "", "the queue ran out, it did not fail");
    assert_eq!(now.error, "Skipped 1 unavailable track", "not the decoder's own message");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_queue_where_nothing_decodes_stops_after_one_pass() {
    let dir = folder("refused-all");
    other_library(&dir);
    english();
    let paths = strings(&[empty_file(&dir, "01.wav"), garbage_file(&dir, "02.flac"), garbage_file(&dir, "03.dsf")]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.set_repeat(chilpa_api::Repeat::All);
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_until_stopped(&mut chilpa));
    let limit = std::time::Instant::now() + std::time::Duration::from_millis(300);
    while std::time::Instant::now() < limit {
        let now = chilpa.poll();
        assert!(!now.playing, "nothing was relaunched");
        assert_eq!(now.error, "Skipped 3 unavailable tracks", "each track is counted once");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(chilpa.queue_paths().len(), 3, "the queue stays");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn repeat_all_plays_around_a_refused_track_without_spinning_on_it() {
    let dir = folder("refused-repeat-all");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let paths = strings(&[wave(&dir, "01.wav", SHORT), garbage_file(&dir, "02.flac"), wave(&dir, "03.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.set_repeat(chilpa_api::Repeat::All);
    chilpa.play_tracks(&paths, OUTPUT, true);
    let heard = heard(&mut chilpa, std::time::Duration::from_millis(1_500));
    assert!(heard.len() >= 4, "it went round more than once: {heard:?}");
    assert!(!heard.contains(&1), "the refused track never came up: {heard:?}");
    assert!(chilpa.now_playing().playing, "it is still playing");
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track", "counted once, not once a lap");
    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn repeat_one_on_a_track_that_breaks_moves_on_and_repeats_the_next() {
    let dir = folder("breaks-repeat-one");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let paths = strings(&[broken_dsf(&dir, "01.dsf"), wave(&dir, "02.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.set_repeat(chilpa_api::Repeat::One);
    chilpa.play_tracks(&paths, OUTPUT, true);
    let heard = heard(&mut chilpa, std::time::Duration::from_millis(1_500));
    assert_eq!(heard, [0, 1], "the broken track is not repeated, the next one is: {heard:?}");
    assert!(chilpa.now_playing().playing);
    assert_eq!(chilpa.now_playing().error, "Skipped 1 unavailable track");
    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_stop_mark_on_a_refused_track_stops_before_it() {
    let dir = folder("refused-stop-after");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let paths = strings(&[wave(&dir, "01.wav", SHORT), garbage_file(&dir, "02.flac"), wave(&dir, "03.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(chilpa.set_stop_after(Some(1)));
    let heard = heard(&mut chilpa, std::time::Duration::from_secs(5));
    assert_eq!(heard, [0], "what follows the mark never plays");
    let now = chilpa.now_playing();
    assert!(!now.playing);
    assert!(now.path.ends_with("01.wav"), "a mark is not the queue running out");
    assert_eq!(now.error, "Skipped 1 unavailable track");
    assert_eq!(chilpa.stop_after(), None, "the mark was consumed");
    assert_eq!(available_rows(&mut chilpa), [true, false, true]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn refused_and_missing_tracks_are_said_in_one_notice() {
    let dir = folder("refused-and-missing");
    other_library(&dir);
    english();
    let paths = strings(&[wave(&dir, "01.wav", 4_410), dir.join("02.wav"), garbage_file(&dir, "03.flac"), wave(&dir, "04.wav", 4_410)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 3), "the last track started");
    assert!(wait_until_stopped(&mut chilpa));
    assert_eq!(chilpa.now_playing().error, "Skipped 2 unavailable tracks");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_rate_the_output_refuses_after_a_refused_track_still_stops_the_queue() {
    let dir = folder("refused-then-rate");
    other_library(&dir);
    english();
    let paths = strings(&[garbage_file(&dir, "01.flac"), wave_at(&dir, "02.wav", 700, 7_000), wave(&dir, "03.wav", 4_410)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_until_stopped(&mut chilpa));
    std::thread::sleep(std::time::Duration::from_millis(200));
    let now = chilpa.poll();
    assert!(!now.playing, "nothing was relaunched");
    assert!(now.index < 2, "the track after the refused rate never started");
    assert!(now.error.contains("7000"), "the output's own message: {}", now.error);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_skip_left_over_from_a_replaced_queue_marks_nothing_in_the_new_one() {
    let dir = folder("skip-stale");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let first = garbage_file(&dir, "01.wav");
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&strings(&[first.clone(), wave(&dir, "02.wav", SHORT * 10)]), OUTPUT, true);
    // The player refuses the first track and says so; nobody reads it yet.
    std::thread::sleep(std::time::Duration::from_millis(500));
    // The file is put right and a new queue holding it starts before the old
    // word is read.
    wave(&dir, "01.wav", SHORT);
    chilpa.play_tracks(&strings(&[first.clone(), wave(&dir, "03.wav", SHORT)]), OUTPUT, true);
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut played = false;
    while !played && std::time::Instant::now() < limit {
        let now = chilpa.poll();
        played = now.playing && now.path == first.display().to_string();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(played, "the track that was put right plays in the new queue");
    let error = chilpa.now_playing().error.clone();
    assert!(!error.starts_with("Skipped"), "nothing of the new queue was skipped: {error}");
    assert_eq!(available_rows(&mut chilpa), [true, true], "nothing of the new queue is dimmed");
    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_that_breaks_partway_is_logged_as_cut_short() {
    let dir = folder("breaks-logged");
    other_library(&dir);
    english();
    let paths = strings(&[broken_dsf(&dir, "01.dsf"), wave(&dir, "02.wav", 4_410), broken_dsf(&dir, "03.dsf")]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_at_least(&mut chilpa, 2), "the last track started");
    assert!(wait_until_stopped(&mut chilpa));
    let logged = chilpa.listening_summary(&chilpa_api::listening::ListenFilter::default()).unwrap();
    assert_eq!(logged.events, 3, "each track that was heard is logged: {logged:?}");
    assert_eq!(logged.skips, 2, "the two that broke, in the middle of the queue and at its end, are cut short: {logged:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_stop_mark_on_a_later_copy_of_a_refused_track_still_stops_the_queue() {
    let dir = folder("refused-copy-stop-after");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let refused = garbage_file(&dir, "01.flac");
    let paths = strings(&[refused.clone(), wave(&dir, "02.wav", SHORT), refused, wave(&dir, "04.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.set_repeat(chilpa_api::Repeat::One);
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_started(&mut chilpa));
    assert_eq!(chilpa.now_playing().index, 1, "the first copy was refused");
    assert!(chilpa.set_stop_after(Some(2)), "a mark on the later copy is taken");
    chilpa.next_track();
    let heard = heard(&mut chilpa, std::time::Duration::from_secs(3));
    assert!(!heard.contains(&3), "what follows the mark never plays: {heard:?}");
    assert!(!chilpa.now_playing().playing, "the queue stopped at the mark");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_stop_mark_put_on_a_copy_already_left_out_still_stops_the_queue() {
    let dir = folder("refused-copy-left-out-stop-after");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let refused = garbage_file(&dir, "01.flac");
    let paths = strings(&[refused.clone(), wave(&dir, "02.wav", SHORT * 3), refused, wave(&dir, "04.wav", SHORT)]);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&paths, OUTPUT, true);
    assert!(wait_started(&mut chilpa));
    assert_eq!(chilpa.now_playing().index, 1, "the first copy was refused");
    // The later copy was taken out of the player's walk when the second
    // track started; the mark goes on it afterwards.
    assert!(chilpa.set_stop_after(Some(2)), "a mark on the later copy is taken");
    let heard = heard(&mut chilpa, std::time::Duration::from_secs(3));
    assert!(!heard.contains(&3), "what follows the mark never plays: {heard:?}");
    assert!(!chilpa.now_playing().playing, "the queue stopped at the mark");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_failure_left_over_from_a_replaced_queue_is_not_the_new_one_s() {
    let dir = folder("failure-stale");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let mut chilpa = chilpa_api::Chilpa::new();
    // A queue of one track that is no audio: the player passes over it and
    // fails, since nothing could play. Nobody reads either word yet.
    chilpa.play_tracks(&strings(&[garbage_file(&dir, "01.flac")]), OUTPUT, true);
    std::thread::sleep(std::time::Duration::from_millis(500));
    // Another queue starts before the old play's end is read.
    chilpa.play_tracks(&strings(&[wave(&dir, "02.wav", SHORT * 20)]), OUTPUT, true);
    assert!(wait_started(&mut chilpa), "the new queue plays");
    let now = chilpa.now_playing();
    assert!(now.playing, "the old play's failure did not stop the new one");
    assert!(now.error.is_empty(), "nor is it said of the new queue: {}", now.error);
    // The old play ended by itself and never says it stopped: the relaunch
    // must not be left waiting for that, or this stop is taken for it.
    chilpa.stop();
    assert!(wait_until_stopped(&mut chilpa), "a stop asked for afterwards stops the window too");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tracks_of_a_replaced_queue_read_late_are_not_logged_as_played_through() {
    let dir = folder("listens-stale");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let mut chilpa = chilpa_api::Chilpa::new();
    // The first track breaks half a second in and the second starts; nobody
    // reads any of it before another queue replaces this one.
    chilpa.play_tracks(&strings(&[broken_dsf(&dir, "01.dsf"), wave(&dir, "02.wav", SHORT * 20)]), OUTPUT, true);
    std::thread::sleep(std::time::Duration::from_millis(1_500));
    chilpa.play_tracks(&strings(&[wave(&dir, "03.wav", SHORT)]), OUTPUT, true);
    assert!(wait_until_stopped(&mut chilpa), "the new queue ran out");
    let logged = chilpa.listening_summary(&chilpa_api::listening::ListenFilter::default()).unwrap();
    assert_eq!(
        logged.events - logged.skips,
        1,
        "only the new queue's track played through; the one that broke and the one cut off by the new queue did not: {logged:?}"
    );
    assert_eq!(logged.skips, 2, "and both of those are in the history, cut short: {logged:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stopping_a_replacement_that_still_waits_behind_the_old_play_shows_stopped() {
    let dir = folder("stop-waiting");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&strings(&[wave(&dir, "01.wav", SHORT * 50)]), OUTPUT, true);
    assert!(wait_started(&mut chilpa), "the first queue plays");
    // Another queue replaces it and is stopped at once: its order is still
    // waiting behind the play it replaced. It never starts, and it has to
    // say so, or nothing ever tells the window it is not playing.
    chilpa.play_tracks(&strings(&[wave(&dir, "02.wav", SHORT * 50)]), OUTPUT, true);
    chilpa.stop();
    assert!(wait_until_stopped(&mut chilpa), "the window shows it stopped");
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!chilpa.poll().playing, "and stays so");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_move_to_another_output_stopped_at_once_keeps_what_was_heard() {
    let dir = folder("move-then-stop");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&strings(&[wave(&dir, "01.wav", SHORT * 50)]), OUTPUT, true);
    assert!(wait_started(&mut chilpa), "the track plays");
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while chilpa.poll().seconds < 0.5 && std::time::Instant::now() < limit {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Moved and stopped in one breath: the play on the new output is most
    // likely still waiting behind the old one, and ends without having
    // started. What was heard before the move is one listen, cut short.
    chilpa.move_output(OUTPUT, true);
    chilpa.stop();
    assert!(!chilpa.now_playing().playing, "the window shows it stopped as the stop returns");
    // The listen is written when both plays have said how they ended.
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut logged = chilpa.listening_summary(&chilpa_api::listening::ListenFilter::default()).unwrap();
    while logged.events == 0 && std::time::Instant::now() < limit {
        chilpa.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
        logged = chilpa.listening_summary(&chilpa_api::listening::ListenFilter::default()).unwrap();
    }
    assert_eq!((logged.events, logged.skips), (1, 1), "one listen, cut short: {logged:?}");
    assert!(logged.listened_ms >= 400, "with what was heard: {logged:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stop_shows_stopped_as_it_returns_while_the_player_still_unwinds() {
    let dir = folder("stop-at-once");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_tracks(&strings(&[wave(&dir, "01.wav", SHORT * 50)]), OUTPUT, true);
    assert!(wait_started(&mut chilpa), "the track plays");
    chilpa.stop();
    assert!(!chilpa.now_playing().playing, "stopped as the call returns, before the player has said so");
    // What the play says while it unwinds does not bring it back.
    for _ in 0..30 {
        assert!(!chilpa.poll().playing);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn moving_the_output_while_another_queue_waits_to_play_does_not_start_the_old_one_again() {
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};
    let dir = folder("move-during-switch");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.use_output(OUTPUT, false);
    let old = dir.join("blocked.wav");
    assert!(std::process::Command::new("mkfifo").arg(&old).status().unwrap().success());
    let (entered, seen) = channel();
    let (release, released) = channel::<()>();
    let pipe = old.clone();
    let writer = std::thread::spawn(move || {
        let mut pipe = std::fs::OpenOptions::new().write(true).open(pipe).unwrap();
        entered.send(()).unwrap();
        // Even a failed assertion releases the blocked player within a deadline.
        let _ = released.recv_timeout(Duration::from_secs(10));
        let _ = pipe.write_all(&b"this is text and no audio. ".repeat(256));
    });
    chilpa.play_from(&strings(&[old.clone()]), 0);
    seen.recv_timeout(Duration::from_secs(10)).expect("the old worker is held in its decoder open");
    let new = wave(&dir, "next.wav", SHORT * 50);
    let other = chilpa.listen_create("Other", &strings(&[new.clone()]));
    assert!(chilpa.listen_play(other, ""));
    const NEXT_OUTPUT: &str = "file:switched";
    chilpa.use_output(NEXT_OUTPUT, false);
    let before_release = chilpa.poll().clone();
    release.send(()).unwrap();
    writer.join().unwrap();
    assert!(!before_release.playing && before_release.path == old.display().to_string(), "the switch really waited behind the blocked old play: {before_release:?}");

    let limit = Instant::now() + Duration::from_secs(10);
    let wanted = new.display().to_string();
    let mut started = false;
    while Instant::now() < limit {
        let now = chilpa.poll();
        // spawn_from sets the path early; duration and progress require Started
        // and actual writes by the successor.
        if now.path == wanted && now.total_seconds > 0.0 && now.frames > 0 && now.playing {
            started = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(started, "the waiting queue emitted Started and delivered audio: {:?}", chilpa.now_playing());
    chilpa.audio_output_command("open", "");
    let (_, document) = chilpa.audio_output_doc(0).unwrap();
    let document: serde_json::Value = serde_json::from_str(&document).unwrap();
    assert_eq!(document["now"]["route"]["device"], NEXT_OUTPUT, "the actual Started route uses the newly chosen output");
    chilpa.stop();
    drop(chilpa);
    assert!(std::fs::metadata(chilpa_core::capture_path()).unwrap().len() > 0, "the closed capture contains the delivered audio");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_player_handed_to_a_cast_has_what_it_said_last_read_first() {
    let dir = folder("cast-cut");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let mut chilpa = chilpa_api::Chilpa::new();
    // The only track breaks half a second in. Nobody reads any of it before
    // the playback is sent to a renderer and the player goes with it.
    chilpa.play_tracks(&strings(&[broken_dsf(&dir, "01.dsf")]), OUTPUT, true);
    std::thread::sleep(std::time::Duration::from_millis(1_500));
    // A renderer nobody answers at: the connection fails by itself.
    chilpa.cast_connect_device(chilpa_api::CastDevice {
        id: "uuid:nobody".into(),
        name: "Nobody".into(),
        model: "Test".into(),
        kind: chilpa_api::CastKind::Dlna,
        address: "127.0.0.1:9".parse().unwrap(),
        description: "http://127.0.0.1:9/device.xml".into(),
    });
    let logged = chilpa.listening_summary(&chilpa_api::listening::ListenFilter::default()).unwrap();
    assert_eq!((logged.events, logged.skips), (1, 1), "the track that broke before the player left is in the history, cut short: {logged:?}");
    chilpa.cast_disconnect();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn closing_with_another_queue_waiting_to_play_writes_what_was_heard_and_starts_nothing() {
    let dir = folder("drop-during-switch");
    other_library(&dir);
    english();
    let _realtime = Realtime::on();
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.use_output(OUTPUT, false);
    let (old, new) = (wave(&dir, "01.wav", SHORT * 50), wave(&dir, "02.wav", SHORT * 50));
    chilpa.play_tracks(&strings(&[old]), OUTPUT, true);
    assert!(wait_started(&mut chilpa), "the first queue plays");
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while chilpa.poll().seconds < 0.3 && std::time::Instant::now() < limit {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let other = chilpa.listen_create("Other", &strings(&[new]));
    assert!(chilpa.listen_play(other, ""), "another queue is asked for");
    drop(chilpa);
    let mut again = chilpa_api::Chilpa::new();
    let logged = again.listening_summary(&chilpa_api::listening::ListenFilter::default()).unwrap();
    assert_eq!((logged.events, logged.skips), (1, 1), "the track that was left is one listen, cut short, and the queue that waited never started: {logged:?}");
    let _ = std::fs::remove_dir_all(dir);
}

/// A hundred thousand rows, and hundreds of them in a row that the player
/// passes over: each is marked as its word is read, at a cost that does not
/// grow with the queue. Marking used to look for the entry in every row.
#[test]
fn a_run_of_skips_in_a_very_long_queue_is_read_without_walking_it() {
    const ROWS: usize = 100_000;
    const SKIPS: usize = 400;
    let dir = folder("skip-long");
    other_library(&dir);
    english();
    let good = wave(&dir, "good.wav", 4_410).display().to_string();
    // The run stands at the end, behind the one track that plays.
    let first = ROWS - SKIPS - 1;
    let mut paths = vec![good; first];
    paths.push(wave(&dir, "first.wav", 4_410).display().to_string());
    paths.extend((0..SKIPS).map(|n| garbage_file(&dir, &format!("bad-{n:04}.wav")).display().to_string()));
    assert_eq!(paths.len(), ROWS);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.use_output(OUTPUT, false);
    let started = std::time::Instant::now();
    chilpa.play_from(&paths, first);
    let limit = started + std::time::Duration::from_secs(60);
    let mut played = false;
    while std::time::Instant::now() < limit {
        let now = chilpa.poll();
        played |= now.index == first && now.seconds > 0.0;
        if played && !now.playing {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let took = started.elapsed();
    assert!(played && !chilpa.now_playing().playing, "the track before the run played and the queue ran out: {:?}", chilpa.now_playing().error);
    assert_eq!(chilpa.now_playing().error, format!("Skipped {SKIPS} unavailable tracks"));
    assert!(took < std::time::Duration::from_secs(30), "{SKIPS} skips in {ROWS} rows took {took:?}");
    let _ = std::fs::remove_dir_all(dir);
}
