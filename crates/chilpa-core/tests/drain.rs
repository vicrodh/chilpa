//! What is still in the device when a track ends naturally is heard.
//!
//! A device keeps what it has been handed for a while -- two seconds, on the
//! DAC this was found on -- and closing it throws that away. At every change
//! of format between tracks and at the end of the queue, the last of a track
//! was never heard: a queue of a 2 s track at 48 kHz and one at 96 kHz moved
//! to the second after 0.3 s.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_with, Controls, SharedQueue};
use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

/// A second of 16 bit stereo at `rate`.
fn wav(name: &str, rate: u32) -> PathBuf {
    let path = std::env::temp_dir().join(format!("chilpa-drain-{name}.wav"));
    let data = rate * 4;
    let mut f = std::fs::File::create(&path).unwrap();
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
    f.write_all(&vec![0x11; data as usize]).unwrap();
    path
}

#[derive(Default)]
struct Held {
    pending: u64,
    played: u64,
    drains: usize,
    flushes: usize,
}

/// A device with a buffer: what is written waits there, `drain` plays it,
/// and closing throws away what is left, as `snd_pcm_drop` does.
#[derive(Clone)]
struct Buffered(StreamFormat, Rc<RefCell<Held>>);

impl AudioSink for Buffered {
    fn format(&self) -> StreamFormat {
        self.0
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let n = frames.len() / self.0.frame_bytes();
        self.1.borrow_mut().pending += n as u64;
        Ok(n)
    }
    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        let mut held = self.1.borrow_mut();
        held.drains += 1;
        if !give_up() {
            held.played += held.pending;
            held.pending = 0;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), SinkError> {
        let mut held = self.1.borrow_mut();
        held.flushes += 1;
        held.pending = 0;
        Ok(())
    }
    fn close(&mut self) -> Result<(), SinkError> {
        self.1.borrow_mut().pending = 0;
        Ok(())
    }
}

fn caps() -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![44_100, 48_000],
        formats: vec![SampleFormat::S16],
        channels: vec![2],
        probed: true,
    }
}

fn play(paths: Vec<PathBuf>, controls: &Controls) -> Held {
    play_asking(paths, controls, &mut |_, _| {})
}

fn play_asking(
    paths: Vec<PathBuf>,
    controls: &Controls,
    on_progress: &mut dyn FnMut(usize, chilpa_core::queue_playback::Progress),
) -> Held {
    let held = Rc::new(RefCell::new(Held::default()));
    let device = Rc::clone(&held);
    play_queue_with(
        &SharedQueue::new(paths),
        &caps(),
        Policy::BitPerfect,
        controls,
        |format| Ok(Buffered(format, Rc::clone(&device))),
        &mut |_, _, _| {},
        on_progress,
    )
    .expect("the queue plays");
    let held = held.borrow();
    Held { pending: held.pending, played: held.played, drains: held.drains, flushes: held.flushes }
}

#[test]
fn every_track_is_heard_whole_across_a_change_of_rate_and_at_the_end() {
    let (first, second) = (wav("first", 44_100), wav("second", 48_000));
    let held = play(vec![first.clone(), second.clone()], &Controls::new());
    assert_eq!(held.played, 44_100 + 48_000, "both seconds reached the speaker");
    assert_eq!(held.drains, 2, "once before the rate changed, once at the end");
    let _ = std::fs::remove_file(first);
    let _ = std::fs::remove_file(second);
}

#[test]
fn a_skipped_track_is_not_waited_for() {
    let (first, second) = (wav("skipped", 44_100), wav("after-skip", 48_000));
    let controls = Controls::new();
    controls.skip.skip();
    let held = play(vec![first.clone(), second.clone()], &controls);
    assert_eq!(held.drains, 1, "only at the natural end of the second");
    let _ = std::fs::remove_file(first);
    let _ = std::fs::remove_file(second);
}

#[test]
fn a_seek_throws_away_what_the_device_held_from_before() {
    // Half a second of the music being left played before the part asked
    // for.
    let track = wav("seek-flush", 44_100);
    let controls = Controls::new();
    let asked = controls.seek.clone();
    let mut sought = false;
    let held = play_asking(vec![track.clone()], &controls, &mut |_, progress| {
        if !sought && progress.delivered > 4_096 {
            asked.request(std::time::Duration::from_millis(500));
            sought = true;
        }
    });
    assert!(sought);
    assert_eq!(held.flushes, 1, "once, at the seek");
    let _ = std::fs::remove_file(track);
}

#[test]
fn a_skip_throws_away_the_rest_of_the_track_left() {
    // Two tracks at one rate share the device, and the rest of the skipped
    // one was still in it, ahead of the next.
    let (first, second) = (wav("skip-flush-1", 44_100), wav("skip-flush-2", 44_100));
    let controls = Controls::new();
    let asked = controls.skip.clone();
    let mut skipped = false;
    let held = play_asking(vec![first.clone(), second.clone()], &controls, &mut |index, progress| {
        if index == 0 && !skipped && progress.delivered > 4_096 {
            asked.skip();
            skipped = true;
        }
    });
    assert!(skipped);
    assert_eq!(held.flushes, 1, "once, between the two");
    let _ = std::fs::remove_file(first);
    let _ = std::fs::remove_file(second);
}

