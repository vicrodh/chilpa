//! DSD through the real silent output, from facade to bytes on disk.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const DSD64: u32 = 2_822_400;
const OUTPUT: &str = "file:capture";

fn dsf(path: &Path, samples: u64) {
    let channels = 2u32;
    let block = 4096u32;
    let per_channel = ((samples / 8) as usize).div_ceil(block as usize) * block as usize;
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
    out.extend_from_slice(&DSD64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&samples.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_chunk.to_le_bytes());
    out.resize(out.len() + data_len, 0x69);
    std::fs::write(path, out).unwrap();
}

fn test_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("chilpa-api-dsd-capture");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn shared_dsd_reaches_file_capture_as_pcm() {
    let dir = test_dir();
    std::env::set_var("TMPDIR", &dir);
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    let track = dir.join("input.dsf");
    let capture = dir.join("chilpa-capture.raw");
    dsf(&track, u64::from(DSD64) / 10);

    let mut chilpa = chilpa_api::Chilpa::new();
    assert_eq!(chilpa.play_tracks(&[track.display().to_string()], OUTPUT, false).len(), 1);
    let limit = Instant::now() + Duration::from_secs(10);
    let mut started = false;
    while Instant::now() < limit {
        let now = chilpa.poll();
        started |= !now.format.is_empty();
        if started && !now.playing {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let now = chilpa.now_playing();
    assert!(started, "the DSD track started");
    assert!(now.format.contains("DSD64 · PCM at 88200 Hz"), "announced {}", now.format);
    assert!(!now.bit_perfect, "DSD-to-PCM cannot be bit-perfect");
    assert!(std::fs::metadata(&capture).is_ok_and(|meta| meta.len() > 0), "PCM reached file:capture");
    chilpa.stop();
    let _ = std::fs::remove_dir_all(dir);
}
