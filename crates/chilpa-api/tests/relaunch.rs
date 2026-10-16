//! Pressing something that relaunches the queue, while the queue is playing.
//!
//! The player is one thread serving one order at a time, and an order to play
//! blocks in it until the whole queue is done. So anything that relaunches has
//! to stop what is playing first, or its order sits in the channel unread --
//! with repeat on, forever.
//!
//! Going back to the previous track did not stop first, and nothing noticed
//! for two reasons worth writing down. The window looked right: `spawn` sets
//! the position back immediately, so the number moved even though the music
//! did not. And the test that covered it waited for the position to be "at
//! least" where it already was, which was true before the button was pressed.
//!
//! What this measures is the music, not the number: which file is playing,
//! and how long it took to get there. Verified both ways on 2026-09-24 --
//! with the stop the earlier track is playing inside 1.5 s; with the stop
//! taken out it never arrives, because the rest of the queue has to run out
//! first.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The capture output writes to a file, so this runs without sound and
/// without hardware.
const OUTPUT: &str = "file:capture";

/// Points the library at a temporary directory, so the test does not write
/// plays into the library of whoever runs it.
fn other_library(dir: &std::path::Path) {
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
}

fn folder(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-api-relaunch-{name}"));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A WAV of that many seconds.
///
/// Long on purpose. The capture has no clock and swallows what it is given
/// about four times faster than a device would, so short tracks leave nothing
/// to interrupt: every press would land after the queue had already finished
/// and the test would pass with the player broken. Sixteen seconds each
/// leaves around eight seconds of waiting behind the press, which is several
/// times the window below even on a busy machine.
fn wave(dir: &std::path::Path, name: &str, seconds: u32) -> PathBuf {
    let p = dir.join(name);
    let frames = 44_100 * seconds;
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

/// Waits until that file is the one playing.
fn playing_is(chilpa: &mut chilpa_api::Chilpa, name: &str, patience: Duration) -> bool {
    let limit = Instant::now() + patience;
    while Instant::now() < limit {
        if chilpa.poll().path.ends_with(name) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn going_back_plays_the_earlier_track_instead_of_waiting_for_the_queue() {
    let dir = folder("back");
    other_library(&dir);
    for n in ["01.wav", "02.wav", "03.wav"] {
        wave(&dir, n, 16);
    }

    let mut chilpa = chilpa_api::Chilpa::new();
    assert_eq!(chilpa.play_folder(&dir, OUTPUT, true).len(), 3, "all three were queued");
    assert!(playing_is(&mut chilpa, "02.wav", Duration::from_secs(20)), "it got to the second");

    chilpa.previous_track(OUTPUT, true);
    // The window is allowed a moment for the device to close and reopen, and
    // no more than that. Without the stop this never arrives: the second and
    // third tracks have to run out first.
    assert!(
        playing_is(&mut chilpa, "01.wav", Duration::from_millis(1500)),
        "going back has to play the earlier track, not queue itself behind the rest",
    );
}

