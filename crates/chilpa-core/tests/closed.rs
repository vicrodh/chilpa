//! Every track that started is closed, once, with how it ended and what of
//! it left the device.
//!
//! The listen of a track used to be settled from three numbers that did not
//! mean the same thing, read at different moments. The player now says one,
//! per track, when the audio the track left in the device is accounted for.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::DeviceCaps;
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::attempts::How;
use chilpa_core::player::{Player, PlayerEvent};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chilpa-closed-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Sixteen bit stereo at 44.1 kHz, `frames` long.
fn wave(dir: &Path, name: &str, frames: u32) -> PathBuf {
    let data = frames * 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&176_400u32.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    out.resize(out.len() + data as usize, 0);
    let path = dir.join(name);
    std::fs::write(&path, out).unwrap();
    path
}

const DSD64: u64 = 2_822_400;

/// A second of two channel DSD64 silence as a DSF, cut halfway through its
/// audio: it opens, plays its first half and breaks.
fn broken_dsf(dir: &Path, name: &str) -> PathBuf {
    let (channels, block) = (2u32, 4096u32);
    let per_channel = ((DSD64 / 8) as usize).div_ceil(block as usize) * block as usize;
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
    out.extend_from_slice(&(DSD64 as u32).to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&DSD64.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_chunk.to_le_bytes());
    out.resize(out.len() + data_len / 2, 0x69);
    let path = dir.join(name);
    std::fs::write(&path, out).unwrap();
    path
}

fn caps_16() -> DeviceCaps {
    DeviceCaps { sample_rates: vec![44_100, 88_200], formats: vec![SampleFormat::S16, SampleFormat::S24In32], channels: vec![2], probed: true }
}

/// What the test's device does, set as it goes.
#[derive(Default)]
struct Device {
    /// Takes a little at a time, so a long track stays playing.
    slow: AtomicBool,
    /// Fails on its next write.
    lost: AtomicBool,
    /// Panics on its next write.
    panics: AtomicBool,
    /// Openings so far, and the one that fails.
    opened: AtomicUsize,
    refuses_opening: AtomicUsize,
}

struct Sink {
    format: StreamFormat,
    device: Arc<Device>,
}

impl AudioSink for Sink {
    fn format(&self) -> StreamFormat {
        self.format
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        if self.device.panics.load(Ordering::Acquire) {
            panic!("the device panicked");
        }
        if self.device.lost.load(Ordering::Acquire) {
            return Err(SinkError::DeviceLost);
        }
        let all = frames.len() / self.format.frame_bytes();
        if self.device.slow.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(2));
            return Ok(all.min(64));
        }
        Ok(all)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

fn player(device: &Arc<Device>) -> Player {
    let device = Arc::clone(device);
    Player::spawn(
        Box::new(|_| Ok(caps_16())),
        Box::new(move |_, format, _| {
            let opening = device.opened.fetch_add(1, Ordering::AcqRel) + 1;
            if device.refuses_opening.load(Ordering::Acquire) == opening {
                return Err(SinkError::Backend("the device would not open again".into()));
            }
            Ok(Box::new(Sink { format, device: Arc::clone(&device) }) as Box<dyn AudioSink + Send>)
        }),
    )
}

fn terminal(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. } | PlayerEvent::Stopped { .. })
}

/// What the player said until the play ended, calling `when_started` once a
/// track has started.
fn play(player: &Player, tracks: &[PathBuf], gapless: bool, mut when_started: impl FnMut(&Player)) -> Vec<PlayerEvent> {
    player.set_gapless(gapless);
    player.play_all(tracks, "fake");
    let limit = Instant::now() + Duration::from_secs(20);
    let (mut seen, mut told) = (Vec::new(), false);
    loop {
        seen.extend(player.poll());
        if !told && seen.iter().any(|event| matches!(event, PlayerEvent::Started { .. })) {
            told = true;
            when_started(player);
        }
        if seen.iter().any(terminal) {
            return seen;
        }
        assert!(Instant::now() < limit, "the play never ended: {seen:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Every track that started was closed once, after it started and before
/// the play ended, in the order they started. Gives back each closing: the
/// queue position, what was heard and how it ended.
fn closings(events: &[PlayerEvent]) -> Vec<(usize, u64, How)> {
    let started: Vec<u64> = events.iter().filter_map(|event| match event {
        PlayerEvent::Started { attempt, .. } => Some(*attempt),
        _ => None,
    }).collect();
    let closed: Vec<u64> = events.iter().filter_map(|event| match event {
        PlayerEvent::Closed { attempt, .. } => Some(*attempt),
        _ => None,
    }).collect();
    assert_eq!(closed, started, "one closing for every start, in order: {events:?}");
    for attempt in &started {
        let at = |wanted: &dyn Fn(&PlayerEvent) -> bool| events.iter().position(|event| wanted(event)).unwrap();
        let start = at(&|event| matches!(event, PlayerEvent::Started { attempt: of, .. } if of == attempt));
        let close = at(&|event| matches!(event, PlayerEvent::Closed { attempt: of, .. } if of == attempt));
        assert!(start < close, "attempt {attempt} closed after it started: {events:?}");
    }
    assert!(events.last().is_some_and(terminal), "the play's ending is the last word: {events:?}");
    assert_eq!(events.iter().filter(|event| terminal(event)).count(), 1, "{events:?}");
    events.iter().filter_map(|event| match event {
        PlayerEvent::Closed { index, heard, how, .. } => Some((*index, *heard, *how)),
        _ => None,
    }).collect()
}

#[test]
fn tracks_played_to_their_end_are_closed_as_completed_with_all_of_them_heard() {
    let dir = folder("completed");
    let tracks = [wave(&dir, "01.wav", 4_410), wave(&dir, "02.wav", 2_205)];
    for gapless in [true, false] {
        let events = play(&player(&Arc::default()), &tracks, gapless, |_| {});
        assert_eq!(closings(&events), [(0, 4_410, How::Completed), (1, 2_205, How::Completed)], "gapless: {gapless}");
        assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })));
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_the_listener_left_is_closed_as_advanced() {
    let dir = folder("advanced");
    let device = Arc::new(Device::default());
    device.slow.store(true, Ordering::Release);
    let tracks = [wave(&dir, "01.wav", 400_000), wave(&dir, "02.wav", 256)];
    let events = play(&player(&device), &tracks, true, |player| player.next());
    let closed = closings(&events);
    assert_eq!(closed.len(), 2, "{closed:?}");
    assert_eq!((closed[0].0, closed[0].2), (0, How::Advanced));
    assert!(closed[0].1 < 400_000, "not all of it was heard: {closed:?}");
    assert_eq!(closed[1], (1, 256, How::Completed));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_that_breaks_partway_is_closed_as_broken_and_the_next_as_completed() {
    let dir = folder("broken");
    // One-bit audio, converted on its way to this device: the same road
    // with another source.
    let tracks = [broken_dsf(&dir, "01.dsf"), wave(&dir, "02.wav", 2_205)];
    let events = play(&player(&Arc::default()), &tracks, true, |_| {});
    let closed = closings(&events);
    assert_eq!(closed.len(), 2, "{events:?}");
    assert_eq!((closed[0].0, closed[0].2), (0, How::Broken));
    assert!(closed[0].1 > 0, "what was heard of it before it broke: {closed:?}");
    assert_eq!(closed[1], (1, 2_205, How::Completed));
    assert!(events.iter().any(|event| matches!(event, PlayerEvent::Skipped { attempt: 1, index: 0, .. })), "and it is said at once, against its turn: {events:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_stopped_partway_is_closed_as_stopped() {
    let dir = folder("stopped");
    let device = Arc::new(Device::default());
    device.slow.store(true, Ordering::Release);
    let events = play(&player(&device), &[wave(&dir, "01.wav", 400_000)], true, |player| player.stop());
    let closed = closings(&events);
    assert_eq!(closed.len(), 1);
    assert_eq!((closed[0].0, closed[0].2), (0, How::Stopped));
    assert!(matches!(events.last(), Some(PlayerEvent::Stopped { .. })));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_the_device_was_lost_under_is_closed_as_failed() {
    let dir = folder("lost");
    let device = Arc::new(Device::default());
    device.slow.store(true, Ordering::Release);
    let lost = Arc::clone(&device);
    let events = play(&player(&device), &[wave(&dir, "01.wav", 400_000)], true, move |_| lost.lost.store(true, Ordering::Release));
    let closed = closings(&events);
    assert_eq!(closed.len(), 1);
    assert_eq!((closed[0].0, closed[0].2), (0, How::Failed));
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_device_that_will_not_open_again_leaves_the_track_before_it_closed_as_it_ended() {
    let dir = folder("reopen");
    let device = Arc::new(Device::default());
    device.refuses_opening.store(2, Ordering::Release);
    let tracks = [wave(&dir, "01.wav", 4_410), wave(&dir, "02.wav", 2_205)];
    // Without gapless the device is opened again for the second track.
    let events = play(&player(&device), &tracks, false, |_| {});
    assert_eq!(closings(&events), [(0, 4_410, How::Completed)], "the second never started and is owed nothing");
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_panic_under_a_track_still_closes_it_before_the_play_ends_as_failed() {
    let dir = folder("panic");
    let device = Arc::new(Device::default());
    device.slow.store(true, Ordering::Release);
    let panics = Arc::clone(&device);
    let events = play(&player(&device), &[wave(&dir, "01.wav", 400_000)], true, move |_| panics.panics.store(true, Ordering::Release));
    let closed = closings(&events);
    assert_eq!(closed.len(), 1);
    assert_eq!((closed[0].0, closed[0].2), (0, How::Failed));
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })));
    let _ = std::fs::remove_dir_all(dir);
}
