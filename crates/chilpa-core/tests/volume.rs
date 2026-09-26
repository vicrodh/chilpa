//! The player bar's volume, applied to the samples.
//!
//! Scaling the samples is a conversion: with bit-perfect off it is done and
//! said, so the verdict does not read bit-perfect; under the bit-perfect
//! policy it is never done, whatever the volume says.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy, Transform};
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_with, Controls, SharedQueue};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Keeps every sample it is handed.
struct Keep(Arc<Mutex<Vec<i16>>>);

impl AudioSink for Keep {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let mut kept = self.0.lock().unwrap();
        kept.extend(frames.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
        Ok(frames.len() / 4)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// A second of stereo at a constant 10,000.
fn wave() -> PathBuf {
    let p = std::env::temp_dir().join("chilpa-volume.wav");
    let frames = 4_410u32;
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
    for _ in 0..frames * 2 {
        f.write_all(&10_000i16.to_le_bytes()).unwrap();
    }
    p
}

/// Plays the wave at half volume under `policy`: what reached the device,
/// and whether the delivery said the volume changed it.
fn at_half(policy: Policy) -> (Vec<i16>, bool) {
    let queue = SharedQueue::new(vec![wave()]);
    let caps = DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true };
    let controls = Controls::new();
    controls.volume.set(0.5);
    let kept = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&kept);
    let mut said = false;
    play_queue_with(&queue, &caps, policy, &controls, |_| Ok(Keep(Arc::clone(&sink))), &mut |_, _, delivery| {
        said = delivery.transforms.contains(&Transform::VolumeApplied);
    }, &mut |_, _| {})
    .expect("it played");
    let samples = kept.lock().unwrap().clone();
    (samples, said)
}

#[test]
fn the_volume_scales_the_samples_and_says_so_with_bit_perfect_off() {
    let (samples, said) = at_half(Policy::AllowAnything);
    assert!(!samples.is_empty());
    assert!(samples.iter().all(|s| *s == 5_000), "every sample at half");
    assert!(said, "the delivery says the samples were changed");
}

#[test]
fn the_volume_never_touches_the_samples_under_bit_perfect() {
    let (samples, said) = at_half(Policy::BitPerfect);
    assert!(samples.iter().all(|s| *s == 10_000), "every sample as the file has it");
    assert!(!said);
}
