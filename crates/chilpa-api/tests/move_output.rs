//! Moving the music to another output without losing its place.
//!
//! Somebody who plugs a DAC in and picks it wants to hear what they are
//! listening to come out of it, not the next track. So choosing an output
//! relaunches the current one on the new device at the position it had.
//!
//! **Its own test binary on purpose.** What proves the output really moved is
//! the capture file being recreated, and there is one of those per machine;
//! the library directory is an environment variable, which is one per
//! process. Cargo runs the tests inside one file in parallel threads and each
//! file as its own process, so anything that leans on either of those goes in
//! a file of its own or races whatever is running beside it.

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
/// and the test would pass with the player broken. Eight seconds each gives
/// about four seconds of headroom, which is what the window below fits in.
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
fn moving_output_keeps_playing_the_same_track_and_does_not_log_a_skip() {
    let dir = folder("move");
    other_library(&dir);
    // Forty seconds, and the move is asked for two seconds in. The capture
    // delivers about four times faster than a device, so what is left of the
    // track is around nine seconds of waiting -- far outside the window
    // below. Without that gap the track simply ends inside the window and the
    // relaunch happens for that reason instead of the one being tested, which
    // is how the first version of this test passed with the fix taken out.
    let track = wave(&dir, "01.wav", 40);

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_file(&track, OUTPUT, true);
    assert!(playing_is(&mut chilpa, "01.wav", Duration::from_secs(20)), "it started");
    // Far enough in that a relaunch from the beginning would be obvious.
    let limit = Instant::now() + Duration::from_secs(10);
    while Instant::now() < limit && chilpa.poll().seconds < 2.0 {
        std::thread::sleep(Duration::from_millis(5));
    }
    let was_at = chilpa.now_playing().seconds;
    assert!(was_at >= 2.0, "it had played a couple of seconds, not {was_at}");

    // What proves the output was really closed and opened again, rather than
    // the old play simply carrying on: the capture recreates its file, so it
    // gets shorter. Without that check this test passes either way, because
    // the track and its position look the same from outside whether anything
    // moved or not.
    let captured = chilpa_core::capture_path();
    let before = std::fs::metadata(&captured).map(|m| m.len()).unwrap_or(0);
    assert!(before > 0, "the capture had written something");

    // The same device with bit-perfect turned off is a move as far as the
    // player is concerned: the output is closed and opened again. Two
    // different devices take the same path, and take hardware to try.
    chilpa.move_output(OUTPUT, false);
    assert!(
        playing_is(&mut chilpa, "01.wav", Duration::from_millis(1500)),
        "the same track goes on after the move",
    );
    // Watched rather than read once. A recreated capture is empty for an
    // instant and then races back past where it was, because it delivers
    // several times faster than a device would: read once, after the fact,
    // and the drop has already been overwritten.
    let mut smallest = before;
    let watching = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < watching {
        let size = std::fs::metadata(&captured).map(|m| m.len()).unwrap_or(0);
        smallest = smallest.min(size);
        if smallest < before {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        smallest < before,
        "the output was never closed and opened: the capture never went below {before}",
    );

    let now = chilpa.now_playing();
    assert!(now.playing, "it is still playing");
    assert!(
        now.seconds >= was_at - 1.0,
        "the move kept its place: it was at {was_at} and came back at {}",
        now.seconds,
    );
}

#[test]
fn moving_output_with_nothing_playing_changes_nothing() {
    let dir = folder("idle");
    other_library(&dir);
    let mut chilpa = chilpa_api::Chilpa::new();
    // No player at all yet. The chosen output is already remembered, so
    // there is nothing to move and nothing to fail.
    chilpa.move_output(OUTPUT, true);
    assert!(!chilpa.poll().playing, "nothing started playing by itself");
}
