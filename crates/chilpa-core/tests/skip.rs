//! Seeking while it plays: the track goes on from the point asked for.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::seek::SeekSignal;
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_with, Controls, Progress, SharedQueue};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

/// Keeps what was delivered so it can be looked at afterwards.
#[derive(Clone, Default)]
struct Capture(std::rc::Rc<std::cell::RefCell<Vec<i16>>>);

impl AudioSink for Capture {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let mut samples = self.0.borrow_mut();
        for pair in frames.chunks_exact(2) {
            samples.push(i16::from_le_bytes([pair[0], pair[1]]));
        }
        Ok(frames.len() / 4)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// A WAV where the left channel's sample says its own frame number.
fn ramp(name: &str, frames: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-skip-{name}.wav"));
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
        let v = (i % 30_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&(-v).to_le_bytes()).unwrap();
    }
    p
}

fn caps() -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![44_100],
        formats: vec![SampleFormat::S16],
        channels: vec![2],
        probed: true,
    }
}

#[test]
fn a_seek_asked_for_before_the_start_begins_at_that_point() {
    let path = ramp("startup", 44_100 * 3);
    let seek = SeekSignal::new();
    seek.request(Duration::from_secs(2));
    let cap = Capture::default();
    let mut positions = Vec::new();

    play_queue_with(
        &SharedQueue::new(vec![path.clone()]),
        &caps(),
        Policy::BitPerfect,
        &Controls { seek: seek.clone(), ..Controls::new() },
        |_| Ok(cap.clone()),
        &mut |_, _, _| {},
        &mut |_, progress| positions.push(progress.position),
    )
    .unwrap();

    // The first sample out is the one from frame 88,200, that is second 2.
    assert_eq!(cap.0.borrow()[0], (88_200 % 30_000) as i16, "it started at the point asked for");
    // And the reported position counts from the file, not from the seek.
    assert!(
        positions.first().is_some_and(|p| *p >= 88_200),
        "the first position was {:?}",
        positions.first()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_seek_halfway_through_reports_the_position_it_moved_to() {
    // The position is what was delivered plus where the last seek landed,
    // and "delivered" counted from the start of the track: a seek to second
    // 2 after one second had gone out reported second 3.
    let length = 44_100 * 3;
    let path = ramp("halfway", length);
    let seek = SeekSignal::new();
    let asked = seek.clone();
    let cap = Capture::default();
    let mut positions = Vec::new();
    let mut sought = false;

    play_queue_with(
        &SharedQueue::new(vec![path.clone()]),
        &caps(),
        Policy::BitPerfect,
        &Controls { seek: seek.clone(), ..Controls::new() },
        |_| Ok(cap.clone()),
        &mut |_, _, _| {},
        &mut |_, progress| {
            let frames = progress.position;
            positions.push(frames);
            if !sought && frames >= 44_100 {
                asked.request(Duration::from_secs(2));
                sought = true;
            }
        },
    )
    .unwrap();

    assert!(sought, "the track played past its first second");
    let furthest = positions.iter().copied().max().unwrap_or(0);
    assert!(furthest <= u64::from(length), "a position of {furthest} in a track of {length} frames");
    let _ = std::fs::remove_file(path);
}

#[test]
fn after_a_seek_forward_what_was_heard_is_less_than_where_the_track_is() {
    // The history logged the position as the time listened: a seek to the
    // last second of a track logged the whole track as heard.
    let path = ramp("heard", 44_100 * 3);
    let seek = SeekSignal::new();
    seek.request(Duration::from_secs(2));
    let mut last = Progress::default();
    play_queue_with(
        &SharedQueue::new(vec![path.clone()]),
        &caps(),
        Policy::BitPerfect,
        &Controls { seek: seek.clone(), ..Controls::new() },
        |_| Ok(Capture::default()),
        &mut |_, _, _| {},
        &mut |_, progress| last = progress,
    )
    .unwrap();
    assert_eq!(last, Progress { position: 132_300, delivered: 44_100 }, "at the end, one second of three heard");
    let _ = std::fs::remove_file(path);
}

#[test]
fn seeking_past_the_end_finishes_the_track_without_crashing() {
    let path = ramp("past", 44_100);
    let seek = SeekSignal::new();
    seek.request(Duration::from_secs(60));
    let cap = Capture::default();
    let results = play_queue_with(
        &SharedQueue::new(vec![path.clone()]),
        &caps(),
        Policy::BitPerfect,
        &Controls { seek: seek.clone(), ..Controls::new() },
        |_| Ok(cap.clone()),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].outcome.may_advance_queue(), "it ended as a natural end");
    let _ = std::fs::remove_file(path);
}
