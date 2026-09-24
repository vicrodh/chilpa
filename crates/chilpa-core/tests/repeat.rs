//! How the queue is traversed: repeat and order.
//!
//! What is guarded here is that repeat is not a loop with no way out:
//! stopping cuts the lap, skipping advances even under repeat-one, and a
//! track that delivers not one frame does not leave the thread spinning.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_with, Controls, Repeat, SharedQueue};
use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

/// Counts frames and, on reaching the cap, asks to stop: that is what cuts a
/// repeat that would otherwise go on forever.
#[derive(Clone)]
struct CappedSink {
    frames: Rc<RefCell<u64>>,
    tope: u64,
    stop: chilpa_audio::sink::StopSignal,
}

impl AudioSink for CappedSink {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let n = (frames.len() / 4) as u64;
        let mut total = self.frames.borrow_mut();
        *total += n;
        if *total >= self.tope {
            self.stop.stop();
        }
        Ok(n as usize)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

fn wave(name: &str, frames: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-repeat-{name}.wav"));
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
        f.write_all(&v.to_le_bytes()).unwrap();
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

/// Plays until `cap` frames are delivered and returns which tracks played.
fn played(tracks: &[PathBuf], repeat: Repeat, tope: u64) -> Vec<PathBuf> {
    let controls = Controls::new();
    controls.repeat.set(repeat);
    let sink = CappedSink {
        frames: Rc::new(RefCell::new(0)),
        tope,
        stop: controls.stop.clone(),
    };
    let results =
        play_queue_with(&SharedQueue::new(tracks.to_vec()), &caps(), Policy::BitPerfect, &controls, |_| Ok(sink.clone()),
            &mut |_, _, _| {}, &mut |_, _| {})
            .expect("the queue runs");
    results.into_iter().map(|r| r.path).collect()
}

#[test]
fn without_repeat_the_queue_ends() {
    let a = wave("off-a", 4_410);
    let b = wave("off-b", 4_410);
    let tracks = vec![a.clone(), b.clone()];
    // The cap is higher than both tracks together: if the queue did not
    // end on its own, this test would never finish.
    assert_eq!(played(&tracks, Repeat::Off, 1_000_000), vec![a.clone(), b.clone()]);
    for p in tracks {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn repeat_all_starts_over() {
    let a = wave("all-a", 4_410);
    let b = wave("all-b", 4_410);
    let tracks = vec![a.clone(), b.clone()];
    // Cap at two and a half laps; the stop lands mid third track.
    let played = played(&tracks, Repeat::All, 4_410 * 5);
    assert!(played.len() > 2, "it went round more than once: {played:?}");
    assert_eq!(played[0], a, "it started with the first");
    assert_eq!(played[2], a, "and came back to it after the last");
    for p in tracks {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn repeat_one_stays_on_the_same_track() {
    let a = wave("one-a", 4_410);
    let b = wave("one-b", 4_410);
    let tracks = vec![a.clone(), b.clone()];
    let played = played(&tracks, Repeat::One, 4_410 * 3);
    assert!(played.len() > 1, "it repeated: {played:?}");
    assert!(played.iter().all(|p| *p == a), "it never moved to the second: {played:?}");
    for p in tracks {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn under_repeat_one_skipping_does_advance() {
    let a = wave("skip-a", 44_100);
    let b = wave("skip-b", 4_410);
    let tracks = vec![a.clone(), b.clone()];

    let controls = Controls::new();
    controls.repeat.set(Repeat::One);
    // The skip is asked for before starting: the first track ends right
    // away and, even under repeat-one, the queue has to move to the second.
    controls.skip.skip();
    let sink = CappedSink {
        frames: Rc::new(RefCell::new(0)),
        tope: 4_410 * 2,
        stop: controls.stop.clone(),
    };
    let results =
        play_queue_with(&SharedQueue::new(tracks.clone()), &caps(), Policy::BitPerfect, &controls, |_| Ok(sink.clone()),
            &mut |_, _, _| {}, &mut |_, _| {})
            .expect("the queue runs");
    let paths: Vec<PathBuf> = results.into_iter().map(|r| r.path).collect();
    assert_eq!(paths[0], a);
    assert_eq!(paths[1], b, "the skip beat the repeat: {paths:?}");
    for p in tracks {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn an_explicit_order_changes_what_plays() {
    let a = wave("order-a", 4_410);
    let b = wave("order-b", 4_410);
    let c = wave("order-c", 4_410);
    let tracks = vec![a.clone(), b.clone(), c.clone()];

    let controls = Controls::new();
    assert!(controls.order.set(vec![2, 0, 1], 3), "the order is a permutation");
    let sink = CappedSink {
        frames: Rc::new(RefCell::new(0)),
        tope: 1_000_000,
        stop: controls.stop.clone(),
    };
    let results = play_queue_with(
        &SharedQueue::new(tracks.clone()),
        &caps(),
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("the queue runs");
    let paths: Vec<PathBuf> = results.into_iter().map(|r| r.path).collect();
    assert_eq!(paths, vec![c, a, b], "they played in the order asked for");
    for p in tracks {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn what_is_enqueued_mid_queue_does_play() {
    let a = wave("grow-a", 4_410);
    let b = wave("grow-b", 4_410);

    let queue = SharedQueue::new(vec![a.clone()]);
    let controls = Controls::new();
    let sink = CappedSink {
        frames: Rc::new(RefCell::new(0)),
        tope: 1_000_000,
        stop: controls.stop.clone(),
    };
    // The second comes in once the first has started: the queue is not
    // rebuilt, it grows.
    let mut added = false;
    let results = play_queue_with(
        &queue,
        &caps(),
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {
            if !added {
                added = true;
                queue.extend(std::slice::from_ref(&b));
            }
        },
        &mut |_, _| {},
    )
    .expect("the queue runs");

    let paths: Vec<PathBuf> = results.into_iter().map(|r| r.path).collect();
    assert_eq!(paths, vec![a.clone(), b.clone()], "the added one played too");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn a_removed_track_does_not_play_and_the_others_do_not_move() {
    let a = wave("remove-a", 4_410);
    let b = wave("remove-b", 4_410);
    let c = wave("remove-c", 4_410);

    let queue = SharedQueue::new(vec![a.clone(), b.clone(), c.clone()]);
    // The middle one is removed before starting.
    assert!(queue.remove(1));
    assert!(!queue.remove(1), "removing it twice does not count");
    assert_eq!(queue.len(), 3, "the position is kept, empty");

    let controls = Controls::new();
    let sink = CappedSink {
        frames: Rc::new(RefCell::new(0)),
        tope: 1_000_000,
        stop: controls.stop.clone(),
    };
    let results = play_queue_with(
        &queue,
        &caps(),
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("the queue runs");

    let paths: Vec<PathBuf> = results.into_iter().map(|r| r.path).collect();
    assert_eq!(paths, vec![a.clone(), c.clone()], "the middle one was skipped");
    for p in [a, b, c] {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn emptying_the_queue_under_repeat_does_not_leave_the_thread_spinning() {
    let a = wave("empty-a", 4_410);
    let queue = SharedQueue::new(vec![a.clone()]);
    queue.remove(0);

    let controls = Controls::new();
    controls.repeat.set(Repeat::All);
    let sink = CappedSink {
        frames: Rc::new(RefCell::new(0)),
        tope: 1_000_000,
        stop: controls.stop.clone(),
    };
    let results = play_queue_with(
        &queue,
        &caps(),
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("the queue runs");
    assert!(results.is_empty(), "nothing played, and it finished");
    let _ = std::fs::remove_file(a);
}
