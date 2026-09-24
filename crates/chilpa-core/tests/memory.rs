//! Playing must not load the whole track into memory.
//!
//! It is the lock on the audit's most expensive finding: in QBZ, AAC and
//! ALAC in MP4 were decoded whole into a `Vec<f32>`, about 2.7 GB for an
//! hour at 24/96. The test plays a large file against a device that throws
//! away what it receives and watches how much the process's memory grew.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_core::playback::play_file;
use std::path::PathBuf;
use std::process::Command;

/// A device that accepts everything and keeps nothing.
struct CountingSink(StreamFormat, u64);

impl AudioSink for &mut CountingSink {
    fn format(&self) -> StreamFormat {
        self.0
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let n = frames.len() / self.0.frame_bytes();
        self.1 += n as u64;
        Ok(n)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())
        })
        .map(|kb| kb / 1024.0)
        .unwrap_or(0.0)
}

/// A long 24 bit FLAC at 96 kHz. Compressed it takes little on disk, but its
/// PCM is hundreds of megabytes: exactly what blows up if anyone loads it
/// whole.
fn long_file() -> Option<PathBuf> {
    let mut target = std::env::temp_dir();
    target.push("chilpa-memoria-24-96.flac");
    if target.is_file() {
        return Some(target);
    }
    let ok = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi"])
        .args(["-i", "sine=frequency=440:duration=300:sample_rate=96000"])
        .args(["-af", "aformat=sample_fmts=s32:channel_layouts=stereo"])
        .args(["-c:a", "flac", "-sample_fmt", "s32"])
        .arg(&target)
        .output()
        .ok()?
        .status
        .success();
    ok.then_some(target)
}

fn caps() -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![96_000],
        formats: vec![SampleFormat::S24In32, SampleFormat::S32],
        channels: vec![2],
        probed: true,
    }
}

#[test]
fn a_five_minute_track_is_not_loaded_whole_into_memory() {
    let Some(path) = long_file() else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    // 5 minutes of 24/96 stereo is ~345 MB of PCM. If anyone decodes the
    // whole track, it shows up here and not on someone else's machine.
    let before = rss_mb();
    let mut sink = CountingSink(StreamFormat::new(SampleFormat::S24In32, 96_000, 2), 0);
    let played = play_file(&path, &caps(), Policy::BitPerfect, &StopSignal::new(), |format| {
        sink.0 = format;
        Ok(&mut sink)
    })
    .expect("it must play");

    let after = rss_mb();
    let grew = after - before;

    assert!(played.outcome.may_advance_queue(), "it finished cleanly");
    assert!(sink.1 > 96_000 * 290, "{} frames came out", sink.1);
    assert!(
        grew < 60.0,
        "memory grew {:.0} MB while playing; the whole track is being kept",
        grew
    );
    // The file is left behind: generating it costs more than deleting it.
}
