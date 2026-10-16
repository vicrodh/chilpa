//! A long pause lets the device go, and resuming takes it back from what
//! was heard.
//!
//! A DAC held while nothing plays is a DAC nobody else can use: the same bug
//! QBZ had in its three windows. After a pause of a few seconds the device is
//! let go; on resume it is opened again at the same format, and the music
//! goes on from the frame that was heard -- not from where writing had got
//! to, which was up to half a second further on.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::render::RELEASE_AFTER;
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_with, Controls, SharedQueue};
use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// What the device went through, shared with the test.
#[derive(Default)]
struct Seen {
    /// The left channel of every frame sent: its frame number.
    sent: Vec<i16>,
    /// Where `sent` stood when the device was taken back.
    at_wake: Option<usize>,
    suspends: u32,
    wakes: u32,
}

/// A device that holds the last `HOLD` frames it was sent until it is let go.
#[derive(Clone)]
struct Holding {
    seen: Rc<RefCell<Seen>>,
    suspended: Rc<RefCell<bool>>,
    pause_at: usize,
    pause: chilpa_audio::sink::PauseSignal,
}

const HOLD: u64 = 2_048;

impl AudioSink for Holding {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        assert!(!*self.suspended.borrow(), "written to a device that was let go");
        let mut seen = self.seen.borrow_mut();
        for frame in frames.chunks_exact(4) {
            seen.sent.push(i16::from_le_bytes([frame[0], frame[1]]));
        }
        if seen.sent.len() >= self.pause_at && seen.suspends == 0 {
            self.pause.set(true);
        }
        Ok(frames.len() / 4)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
    fn held(&self) -> u64 {
        if *self.suspended.borrow() { 0 } else { HOLD.min(self.seen.borrow().sent.len() as u64) }
    }
    fn suspend(&mut self) -> Result<(), SinkError> {
        *self.suspended.borrow_mut() = true;
        self.seen.borrow_mut().suspends += 1;
        Ok(())
    }
    fn wake(&mut self) -> Result<(), SinkError> {
        *self.suspended.borrow_mut() = false;
        let mut seen = self.seen.borrow_mut();
        seen.wakes += 1;
        seen.at_wake = Some(seen.sent.len());
        Ok(())
    }
}

/// A WAV where the left channel's sample says its own frame number.
fn ramp(frames: u32) -> PathBuf {
    let p = std::env::temp_dir().join("chilpa-release-ramp.wav");
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

#[test]
fn a_long_pause_lets_the_device_go_and_goes_on_from_what_was_heard() {
    let path = ramp(20_000);
    let controls = Controls::new();
    let seen = Rc::new(RefCell::new(Seen::default()));
    let sink = Holding {
        seen: Rc::clone(&seen),
        suspended: Rc::new(RefCell::new(false)),
        pause_at: 8_000,
        pause: controls.pause.clone(),
    };
    let caps = DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true };
    let resume = controls.pause.clone();
    let waker = std::thread::spawn(move || {
        std::thread::sleep(RELEASE_AFTER + Duration::from_millis(500));
        resume.set(false);
    });
    let results = play_queue_with(
        &SharedQueue::new(vec![path]),
        &caps,
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("it played");
    waker.join().unwrap();

    let seen = seen.borrow();
    assert_eq!((seen.suspends, seen.wakes), (1, 1), "let go once, taken back once");
    let at = seen.at_wake.expect("taken back");
    // The device held the last HOLD frames of what had been sent when it was
    // let go; the first frame after taking it back is the first of those.
    let heard = at as i16 - HOLD as i16;
    assert_eq!(seen.sent[at], heard, "it goes on from what was heard, not from what was sent");
    // And from there to the end, in order, with nothing skipped.
    assert!(seen.sent[at..].windows(2).all(|w| w[1] == w[0] + 1), "no gap after resuming");
    assert_eq!(*seen.sent.last().unwrap(), 19_999);
    assert!(results[0].outcome.may_advance_queue());
}
