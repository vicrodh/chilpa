//! Cutting off what is playing to start something else, and where it starts.
//!
//! Two things are pinned here and the second one is the invariant.
//!
//! **A relaunch takes effect now.** The player is one thread serving one
//! order at a time, and an order to play blocks in it until the whole queue
//! is done. Sending a second one without stopping first leaves it waiting in
//! the channel: with repeat on, forever. Going back to the previous track did
//! exactly that and nothing noticed, because the test that covered it waited
//! for the position to be "at least" where it already was.
//!
//! **The old output is closed before the new one is opened.** This is what
//! bit-perfect costs: the device belongs to Chilpa while a track plays, so
//! moving the music somewhere else has to let go of the first one. A player
//! that opens the second while still holding the first leaves a DAC locked
//! with nothing playing through it, and the only way to get it back is to
//! unplug it. That happened in QBZ and it is the reason the order is
//! asserted here rather than assumed.
//!
//! The sink delivers at roughly the speed a real device would. Without that
//! there is nothing to interrupt: a capture writes a whole track to a file in
//! a millisecond, so every one of these presses would land after the music
//! had already finished, and the test would pass while the player was broken.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::DeviceCaps;
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::player::{Player, PlayerEvent};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What happened to the devices, in the order it happened.
type Log = Arc<Mutex<Vec<String>>>;

/// A sink that takes as long to swallow a block as a device would.
struct Paced {
    device: String,
    log: Log,
}

impl AudioSink for Paced {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let count = frames.len() / 4;
        // What a device would take to play them. This is the whole reason
        // there is something left to interrupt.
        std::thread::sleep(Duration::from_secs_f64(count as f64 / 44_100.0));
        Ok(count)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        self.log.lock().unwrap().push(format!("close {}", self.device));
        Ok(())
    }
}

impl Drop for Paced {
    fn drop(&mut self) {
        // A sink that is dropped without `close` still let go of the device,
        // and for this test that counts the same: what is being pinned is
        // that the first one was released before the second was taken.
        self.log.lock().unwrap().push(format!("drop {}", self.device));
    }
}

fn player_with(log: Log) -> Player {
    let opening = Arc::clone(&log);
    Player::spawn(
        Box::new(|_| {
            Ok(DeviceCaps {
                sample_rates: vec![44_100],
                formats: vec![SampleFormat::S16],
                channels: vec![2],
                probed: true,
            })
        }),
        Box::new(move |device, _format, _exclusive| {
            opening.lock().unwrap().push(format!("open {device}"));
            Ok(Box::new(Paced { device: device.to_string(), log: Arc::clone(&opening) })
                as Box<dyn AudioSink + Send>)
        }),
    )
}

/// A WAV whose left sample says its own frame number, so a position can be
/// read off what came out.
fn tone(name: &str, seconds: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-relaunch-{name}.wav"));
    let frames = 44_100 * seconds;
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

/// Collects events until `wanted` says it has seen enough, or gives up.
fn until(
    player: &Player,
    patience: Duration,
    mut wanted: impl FnMut(&PlayerEvent) -> bool,
) -> bool {
    let limit = Instant::now() + patience;
    while Instant::now() < limit {
        for event in player.poll() {
            if wanted(&event) {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn a_relaunch_takes_effect_while_something_is_playing() {
    let track = tone("now", 30);
    let log: Log = Arc::default();
    let player = player_with(Arc::clone(&log));

    player.play_all(&[track.clone()], "a");
    assert!(
        until(&player, Duration::from_secs(5), |e| matches!(e, PlayerEvent::Started { .. })),
        "it started on the first device",
    );
    // It is genuinely playing, not just opened.
    assert!(
        until(&player, Duration::from_secs(5), |e| {
            matches!(e, PlayerEvent::Progress { frames, .. } if *frames > 0)
        }),
        "samples went out",
    );

    // The bug this pins: without the stop, this order sits in the channel
    // until the 30 second track is over.
    player.stop();
    player.play_all_from(&[track], "b", None);
    let moved = until(&player, Duration::from_secs(5), |e| {
        matches!(e, PlayerEvent::Started { .. })
    });
    assert!(moved, "the relaunch started instead of waiting for the track to end");

    let events = log.lock().unwrap().clone();
    let opened_b = events.iter().position(|e| e == "open b").expect("the second device opened");
    let left_a = events
        .iter()
        .position(|e| e == "close a" || e == "drop a")
        .expect("the first device was let go of");
    assert!(
        left_a < opened_b,
        "the first device has to be released before the second is opened, and was not: {events:?}",
    );
}

#[test]
fn a_relaunch_can_start_the_track_somewhere_other_than_its_beginning() {
    let track = tone("place", 30);
    let log: Log = Arc::default();
    let player = player_with(Arc::clone(&log));

    player.play_all(&[track.clone()], "a");
    assert!(
        until(&player, Duration::from_secs(5), |e| {
            matches!(e, PlayerEvent::Progress { frames, .. } if *frames > 0)
        }),
        "it was playing",
    );

    player.stop();
    player.play_all_from(&[track], "b", Some(Duration::from_secs(20)));
    // The position counts from the point asked for, so the first progress of
    // the new play is already past it. Nineteen and not twenty: the
    // container lands on the frame it can and the discarded frames are
    // counted in, which is what keeps the number from running short.
    let kept_its_place = until(&player, Duration::from_secs(10), |e| {
        matches!(e, PlayerEvent::Progress { frames, .. } if *frames > 44_100 * 19)
    });
    assert!(kept_its_place, "it picked up where it was asked to, not at the beginning");
}

#[test]
fn a_seek_left_over_from_the_last_play_does_not_move_the_next_one() {
    // The trap behind carrying the position in the order instead of asking
    // for a seek after the fact: the seek signal is one shared slot, and a
    // request made while the old source is still alive is read by whichever
    // gets there first. One that nobody served has to be thrown away, or the
    // next track starts in the middle for no reason anybody asked for.
    let track = tone("leftover", 30);
    let log: Log = Arc::default();
    let player = player_with(Arc::clone(&log));

    player.play_all(&[track.clone()], "a");
    assert!(
        until(&player, Duration::from_secs(5), |e| {
            matches!(e, PlayerEvent::Progress { frames, .. } if *frames > 0)
        }),
        "it was playing",
    );

    // Asked for and never served, because the stop arrives first.
    player.seek(Duration::from_secs(25));
    player.stop();
    player.play_all_from(&[track], "b", None);
    assert!(
        until(&player, Duration::from_secs(5), |e| matches!(e, PlayerEvent::Started { .. })),
        "the new play started",
    );
    // Whatever it reports, it is near the beginning and not near the end.
    let jumped = until(&player, Duration::from_secs(3), |e| {
        matches!(e, PlayerEvent::Progress { frames, .. } if *frames > 44_100 * 20)
    });
    assert!(!jumped, "a seek nobody served was carried into the next play");
}
