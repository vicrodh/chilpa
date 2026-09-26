//! Gapless off: one track does not run into the next.
//!
//! With gapless on, two tracks of the same format share one opening of the
//! device and the second starts on the next sample. Off, the device plays
//! out what it holds and is opened again for the next track: the short
//! break between songs some people want back.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_with, Controls, SharedQueue};
use std::io::Write;
use std::path::PathBuf;

struct Quiet;

impl AudioSink for Quiet {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        Ok(frames.len() / 4)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

fn wave(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("chilpa-gapless-switch-{name}.wav"));
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
    f.write_all(&vec![0u8; data as usize]).unwrap();
    p
}

fn openings(gapless: bool) -> usize {
    let queue = SharedQueue::new(vec![wave("a"), wave("b")]);
    let caps = DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true };
    let controls = Controls::new();
    controls.gapless.set(gapless);
    let mut opened = 0;
    play_queue_with(&queue, &caps, Policy::BitPerfect, &controls, |_| {
        opened += 1;
        Ok(Quiet)
    }, &mut |_, _, _| {}, &mut |_, _| {})
    .expect("it played");
    opened
}

#[test]
fn gapless_keeps_the_device_open_and_off_opens_it_again() {
    assert!(Controls::new().gapless.get(), "gapless is on unless turned off");
    assert_eq!(openings(true), 1, "two tracks, one opening");
    assert_eq!(openings(false), 2, "off, the device is opened again for the second");
}
