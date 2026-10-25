//! The player bar's volume, end to end, on the file output.
//!
//! One test, because it points `XDG_DATA_HOME` and `XDG_CONFIG_HOME` at a
//! temporary directory -- the volume is a preference and the output is
//! remembered -- and those belong to the whole process.

use std::io::Write;
use std::path::PathBuf;

const OUTPUT: &str = "file:capture";

fn wave(dir: &PathBuf, frames: u32) -> PathBuf {
    let p = dir.join("tone.wav");
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
    for i in 0..frames * 2 {
        f.write_all(&((i % 3_000) as i16).to_le_bytes()).unwrap();
    }
    p
}

/// Polls until the track has started, and says what the verdict is.
fn started_bit_perfect(chilpa: &mut chilpa_api::Chilpa) -> bool {
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < limit {
        let now = chilpa.poll();
        if !now.format.is_empty() {
            return now.bit_perfect;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the track never started");
}

#[test]
fn shared_volume_scales_samples_and_only_direct_alsa_locks_it() {
    let dir = std::env::temp_dir().join("chilpa-volume-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    let tone = wave(&dir, 441_000).display().to_string();

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.remember(OUTPUT, true);
    chilpa.set_player_volume(40);
    for percent in [41, 63, 27] { chilpa.preview_player_volume(percent); }
    assert_eq!(chilpa.player_volume().percent, 27);
    assert_eq!(chilpa.preference("playback.volume").as_deref(), Some("40"), "dragging does not write settings");
    chilpa.persist_player_volume();
    assert_eq!(chilpa.preference("playback.volume").as_deref(), Some("27"));
    chilpa.set_player_volume(40);
    let view = chilpa.player_volume();
    assert_eq!((view.percent, view.locked, view.hardware), (40, false, false));

    chilpa.remember("alsa:hw:999,0", false);
    let direct = chilpa.player_volume();
    assert!(direct.locked, "direct ALSA does not scale samples");
    assert!(!direct.why.is_empty(), "and says why");

    chilpa.remember(OUTPUT, false);

    chilpa.play_tracks(&[tone.clone()], OUTPUT, false);
    assert!(!started_bit_perfect(&mut chilpa), "scaled samples are not bit-perfect");
    chilpa.stop();

    chilpa.set_player_volume(100);
    chilpa.play_tracks(&[tone], OUTPUT, false);
    assert!(started_bit_perfect(&mut chilpa), "at full volume nothing touches them");
    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}
