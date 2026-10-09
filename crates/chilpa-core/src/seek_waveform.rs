//! The seek bar's waveform of a file, read in the background: a second
//! decoder over the same file, as fast as it decodes, apart from the one
//! that plays. A local file is done in seconds; the document grows under
//! the view meanwhile. A DSD file goes through the DSD-to-PCM converter,
//! the one playback uses when the device takes no DSD.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub use chilpa_audio::seek_waveform::{columns, Accumulator, Feed, Snapshot, BINS};
use chilpa_decode::{Block, Decoded};

/// How often a growing document is handed out.
const PUBLISH_EVERY: Duration = Duration::from_millis(100);

/// Reads `path` whole and hands each step of the document to `publish`,
/// the last one complete. It stops where `stop` says so, and says why
/// when the file cannot be read or has no known length.
pub fn analyze_file(path: &Path, stop: &AtomicBool, publish: &mut dyn FnMut(&Snapshot)) -> Result<(), String> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dsf") || e.eq_ignore_ascii_case("dff")) {
        return analyze_dsd_file(path, stop, publish);
    }
    let mut decoded = Decoded::open(path).map_err(|e| e.to_string())?;
    let format = decoded.stream_format();
    let total = decoded.frame_count().ok_or_else(|| chilpa_i18n::t("the length of the track is not known"))?;
    let mut accumulator = Accumulator::new(format.channels, total);
    let channels = usize::from(format.channels.max(1));
    let (mut at, mut revision, mut last) = (0u64, 0u64, Instant::now());
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let block = match decoded.next_block() {
            Ok(Some(block)) => block,
            Ok(None) => break,
            Err(e) => return Err(e.to_string()),
        };
        let frames = match &block {
            Block::Int { data, bits } => {
                accumulator.feed_int(at, data, *bits);
                data.len() / channels
            }
            Block::Float(data) => {
                accumulator.feed_float(at, data);
                data.len() / channels
            }
        };
        at += frames as u64;
        if accumulator.complete() {
            break;
        }
        if last.elapsed() >= PUBLISH_EVERY {
            revision += 1;
            publish(&accumulator.snapshot(revision));
            last = Instant::now();
        }
    }
    accumulator.finish();
    publish(&accumulator.snapshot(revision + 1));
    Ok(())
}

/// The same for a DSD file: converted to PCM block by block, as playback
/// converts it, and read for its loudness like any other.
fn analyze_dsd_file(path: &Path, stop: &AtomicBool, publish: &mut dyn FnMut(&Snapshot)) -> Result<(), String> {
    let source = crate::dsd_playback::open_dsd_file(path).map_err(|e| e.to_string())?;
    let mut converter = chilpa_dsd::DsdPcmConverter::new(source, 0.0).map_err(|e| e.to_string())?;
    let channels = usize::from(converter.channels().max(1));
    let mut accumulator = Accumulator::new(converter.channels(), converter.total_frames());
    let (mut at, mut revision, mut last) = (0u64, 0u64, Instant::now());
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let Some(samples) = converter.next_block().map_err(|e| e.to_string())? else { break };
        accumulator.feed_float(at, &samples);
        at += (samples.len() / channels) as u64;
        if accumulator.complete() {
            break;
        }
        if last.elapsed() >= PUBLISH_EVERY {
            revision += 1;
            publish(&accumulator.snapshot(revision));
            last = Instant::now();
        }
    }
    accumulator.finish();
    publish(&accumulator.snapshot(revision + 1));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn wav(frames: u32, loud_from: u32) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("chilpa-seek-waveform-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{frames}-{loud_from}.wav"));
        let data_len = frames * 2;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for frame in 0..frames {
            let value: i16 = if frame >= loud_from { 16000 } else { 2000 };
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn a_file_is_read_whole_and_its_louder_half_stands_out() {
        let path = wav(16_000, 8_000);
        let mut seen = Vec::new();
        analyze_file(&path, &AtomicBool::new(false), &mut |s| seen.push(s.clone())).unwrap();
        let last = seen.last().expect("at least the complete document");
        assert!(last.complete);
        assert_eq!(last.analyzed, BINS as u16);
        assert_eq!(last.bins[BINS - 1], 255, "the loud half fills the height");
        assert!(last.bins[0] > 20 && last.bins[0] < 50, "the quiet half is an eighth: {}", last.bins[0]);
    }

    #[test]
    fn a_stop_ends_the_pass_without_a_document() {
        let path = wav(16_000, 8_000);
        let mut seen = Vec::new();
        analyze_file(&path, &AtomicBool::new(true), &mut |s| seen.push(s.clone())).unwrap();
        assert!(seen.is_empty());
    }

    #[test]
    fn a_file_that_is_not_there_says_so() {
        let missing = std::env::temp_dir().join("chilpa-seek-waveform-missing.wav");
        assert!(analyze_file(&missing, &AtomicBool::new(false), &mut |_| {}).is_err());
    }
}
