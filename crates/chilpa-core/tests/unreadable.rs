//! A track that cannot be played is passed over, and the queue goes on.
//!
//! A file the decoder refuses, a one-bit file that will not open, a file
//! that breaks partway: each one used to end the whole queue. The output is
//! another matter: a rate it refuses or a device that is lost still stops
//! everything, and a lap where nothing plays ends the queue instead of
//! spinning.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::render::Outcome;
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::playback::PlayError;
use chilpa_core::player::{Player, PlayerEvent};
use chilpa_core::queue_playback::{play_queue_told, play_queue_with, Controls, Repeat, SharedQueue, TrackResult};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DSD64: u64 = 2_822_400;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chilpa-unreadable-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A tenth of a second of 16 bit stereo at 44.1 kHz.
fn wave(dir: &Path, name: &str) -> PathBuf {
    wave_at(dir, name, 4_410, 44_100)
}

fn wave_at(dir: &Path, name: &str, frames: u32, rate: u32) -> PathBuf {
    let path = dir.join(name);
    let data = frames * 4;
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
    for i in 0..frames {
        let v = (i % 3_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    path
}

/// A file with an audio name and nothing in it.
fn empty(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"").unwrap();
    path
}

/// A file with an audio name and bytes that are no audio at all.
fn garbage(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"this is text and no audio at all. ".repeat(256)).unwrap();
    path
}

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

fn caps() -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![44_100, 88_200],
        formats: vec![SampleFormat::S16, SampleFormat::S24In32],
        channels: vec![2],
        probed: true,
    }
}

/// A device that takes what it is given at once, and is lost after
/// `lost_after` frames when that is set.
struct Fake {
    format: StreamFormat,
    written: u64,
    lost_after: Option<u64>,
}

impl AudioSink for Fake {
    fn format(&self) -> StreamFormat {
        self.format
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        if self.lost_after.is_some_and(|limit| self.written >= limit) {
            return Err(SinkError::DeviceLost);
        }
        let n = (frames.len() / self.format.frame_bytes()).min(256);
        self.written += n as u64;
        Ok(n)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

type Ran = (Vec<usize>, Result<Vec<TrackResult>, PlayError>);

/// Plays `tracks` on a thread of its own and says which positions started
/// and how the queue ended. A queue that does not end in time is a spin.
fn run_with(tracks: &[PathBuf], controls: &Controls, lost_after: Option<u64>) -> Ran {
    let (tracks, controls) = (tracks.to_vec(), controls.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut started = Vec::new();
        let result = play_queue_with(
            &SharedQueue::new(tracks),
            &caps(),
            Policy::AllowAnything,
            &controls,
            |format| Ok(Fake { format, written: 0, lost_after }),
            &mut |index, _, _| started.push(index),
            &mut |_, _| {},
        );
        let _ = tx.send((started, result));
    });
    rx.recv_timeout(Duration::from_secs(20)).expect("the queue ended instead of spinning")
}

fn run(tracks: &[PathBuf], controls: &Controls) -> Ran {
    run_with(tracks, controls, None)
}

#[test]
fn a_file_the_decoder_refuses_in_the_middle_is_passed_over() {
    let dir = folder("middle");
    let tracks = [wave(&dir, "01.wav"), empty(&dir, "02.wav"), wave(&dir, "03.wav")];
    let (started, result) = run(&tracks, &Controls::new());
    let results = result.expect("the queue goes on past the empty file");
    assert_eq!(started, [0, 2]);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.outcome.may_advance_queue()), "both played whole");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_refused_first_track_starts_the_next_one() {
    let dir = folder("first");
    let tracks = [garbage(&dir, "01.flac"), wave(&dir, "02.wav"), wave(&dir, "03.wav")];
    let (started, result) = run(&tracks, &Controls::new());
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(started, [1, 2]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_refused_last_track_ends_the_queue_as_one_that_ran_out() {
    let dir = folder("last");
    let tracks = [wave(&dir, "01.wav"), wave(&dir, "02.wav"), garbage(&dir, "03.flac")];
    let (started, result) = run(&tracks, &Controls::new());
    let results = result.expect("the queue ran out");
    assert_eq!(started, [0, 1]);
    assert!(matches!(results.last().map(|r| &r.outcome), Some(Outcome::Finished { .. })));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_one_bit_file_that_will_not_open_is_passed_over() {
    let dir = folder("one-bit");
    let tracks = [garbage(&dir, "01.dsf"), wave(&dir, "02.wav")];
    let (started, result) = run(&tracks, &Controls::new());
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(started, [1]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_that_breaks_partway_is_passed_like_one_that_ended() {
    let dir = folder("partway");
    let tracks = [wave(&dir, "01.wav"), broken_dsf(&dir, "02.dsf"), wave(&dir, "03.wav")];
    let (started, result) = run(&tracks, &Controls::new());
    let results = result.expect("the queue goes on");
    assert_eq!(started, [0, 1, 2], "the track after the broken one played");
    assert!(
        matches!(&results[1].outcome, Outcome::Unreadable { frames, .. } if *frames > 0),
        "the broken track played its first part and says it broke: {:?}",
        results[1].outcome
    );
    assert!(matches!(results[2].outcome, Outcome::Finished { .. }));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_lap_where_nothing_opens_ends_the_queue_under_repeat_all() {
    let dir = folder("nothing-opens");
    let tracks = [empty(&dir, "01.wav"), garbage(&dir, "02.flac")];
    let controls = Controls::new();
    controls.repeat.set(Repeat::All);
    let (started, result) = run(&tracks, &controls);
    assert!(started.is_empty());
    assert!(matches!(result, Err(PlayError::Decode(_))), "it says why nothing played: {:?}", result.as_ref().err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_lap_where_every_track_breaks_ends_the_queue_under_repeat_all() {
    let dir = folder("all-break");
    let tracks = [broken_dsf(&dir, "01.dsf"), broken_dsf(&dir, "02.dsf")];
    let controls = Controls::new();
    controls.repeat.set(Repeat::All);
    let (started, result) = run(&tracks, &controls);
    let results = result.expect("the queue ends");
    assert_eq!(results.len(), tracks.len(), "each broken track is heard once, one lap and no more: {started:?}");
    assert!(results.iter().all(|r| matches!(r.outcome, Outcome::Unreadable { .. })));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn repeat_one_on_a_track_that_breaks_moves_on_to_the_next() {
    let dir = folder("repeat-one-breaks");
    let tracks = [broken_dsf(&dir, "01.dsf"), wave(&dir, "02.wav")];
    let controls = Controls::new();
    controls.repeat.set(Repeat::One);
    // The mark ends the repeat of the second track, so the test ends.
    controls.stop_after.set(Some(1));
    let (started, result) = run(&tracks, &controls);
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(started, [0, 1], "the broken track is not repeated");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn repeat_one_on_a_refused_track_moves_on_to_the_next() {
    let dir = folder("repeat-one-refused");
    let tracks = [empty(&dir, "01.wav"), wave(&dir, "02.wav")];
    let controls = Controls::new();
    controls.repeat.set(Repeat::One);
    controls.stop_after.set(Some(1));
    let (started, result) = run(&tracks, &controls);
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(started, [1]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_stop_mark_on_a_refused_track_stops_before_what_follows() {
    let dir = folder("mark-refused");
    let tracks = [wave(&dir, "01.wav"), garbage(&dir, "02.flac"), wave(&dir, "03.wav")];
    let controls = Controls::new();
    controls.stop_after.set(Some(1));
    let (started, result) = run(&tracks, &controls);
    let results = result.expect("the mark ends the queue");
    assert_eq!(started, [0], "what follows the mark never plays");
    assert_eq!(results.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_stop_mark_on_a_track_that_breaks_stops_after_it() {
    let dir = folder("mark-breaks");
    let tracks = [broken_dsf(&dir, "01.dsf"), wave(&dir, "02.wav")];
    let controls = Controls::new();
    controls.stop_after.set(Some(0));
    let (started, result) = run(&tracks, &controls);
    let results = result.expect("the queue ends at the mark");
    assert_eq!(started, [0]);
    assert!(matches!(results[0].outcome, Outcome::Unreadable { .. }));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_output_that_refuses_the_rate_still_stops_the_queue() {
    let dir = folder("rate");
    // No 7 kHz on this device and no resampler: the output refuses it,
    // which says nothing about the file.
    let tracks = [garbage(&dir, "01.flac"), wave_at(&dir, "02.wav", 700, 7_000), wave(&dir, "03.wav")];
    let (started, result) = run(&tracks, &Controls::new());
    assert!(started.is_empty(), "the track after the refused rate never started: {started:?}");
    assert!(matches!(result, Err(PlayError::Negotiation(_))), "the output's own error: {:?}", result.as_ref().err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_device_lost_after_a_refused_track_still_stops_the_queue() {
    let dir = folder("device-lost");
    let tracks = [garbage(&dir, "01.flac"), wave(&dir, "02.wav"), wave(&dir, "03.wav")];
    let (started, result) = run_with(&tracks, &Controls::new(), Some(500));
    let results = result.expect("the queue reports the track the device was lost on");
    assert_eq!(started, [1], "nothing after the lost device started");
    assert!(matches!(results[0].outcome, Outcome::Failed { .. }), "{:?}", results[0].outcome);
    assert_eq!(results.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

fn player() -> Player {
    Player::spawn(
        Box::new(|_| Ok(caps())),
        Box::new(|_, format, _| Ok(Box::new(Fake { format, written: 0, lost_after: None }) as Box<dyn AudioSink + Send>)),
    )
}

/// What the player said until its queue was over.
fn events_until_over(player: &Player) -> Vec<PlayerEvent> {
    let limit = Instant::now() + Duration::from_secs(20);
    let mut seen = Vec::new();
    while Instant::now() < limit {
        for event in player.poll() {
            let over = matches!(event, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. } | PlayerEvent::Stopped { .. });
            seen.push(event);
            if over {
                return seen;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the player never finished: {seen:?}");
}

fn started(events: &[PlayerEvent]) -> Vec<usize> {
    events.iter().filter_map(|event| match event {
        PlayerEvent::Started { index, .. } => Some(*index),
        _ => None,
    }).collect()
}

#[test]
fn the_player_goes_on_past_a_refused_track_and_ends_as_finished() {
    let dir = folder("player-refused");
    let tracks = [wave(&dir, "01.wav"), garbage(&dir, "02.flac"), wave(&dir, "03.wav")];
    let player = player();
    player.play_all(&tracks, "fake");
    let events = events_until_over(&player);
    assert_eq!(started(&events), [0, 2]);
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })), "{events:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_queue_whose_last_track_breaks_partway_ends_as_finished() {
    let dir = folder("player-last-breaks");
    let tracks = [wave(&dir, "01.wav"), broken_dsf(&dir, "02.dsf")];
    let player = player();
    player.play_all(&tracks, "fake");
    let events = events_until_over(&player);
    assert_eq!(started(&events), [0, 1]);
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })), "the queue ran out: {events:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_queue_where_nothing_opens_fails_and_says_why() {
    let dir = folder("player-nothing");
    let tracks = [garbage(&dir, "01.flac")];
    let player = player();
    player.play_all(&tracks, "fake");
    let events = events_until_over(&player);
    assert!(started(&events).is_empty());
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { message }) if !message.is_empty()), "{events:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_player_says_which_entry_it_passed_over_and_why() {
    let dir = folder("player-says");
    let tracks = [wave(&dir, "01.wav"), garbage(&dir, "02.flac"), broken_dsf(&dir, "03.dsf"), wave(&dir, "04.wav")];
    let player = player();
    player.play_all(&tracks, "fake");
    let events = events_until_over(&player);
    let said: Vec<(usize, PathBuf, String)> = events.iter().filter_map(|event| match event {
        PlayerEvent::Skipped { index, path, message, .. } => Some((*index, path.clone(), message.clone())),
        _ => None,
    }).collect();
    assert_eq!(said.iter().map(|(index, path, _)| (*index, path.clone())).collect::<Vec<_>>(), [(1, tracks[1].clone()), (2, tracks[2].clone())]);
    assert!(said.iter().all(|(_, _, message)| !message.is_empty()), "each one says why: {said:?}");
    // How much of each went out is said as it is closed: the refused one
    // never started, so nothing closes it; the broken one closes as broken.
    let closed: Vec<(usize, u64, chilpa_core::attempts::How)> = events.iter().filter_map(|event| match event {
        PlayerEvent::Closed { index, heard, how, .. } => Some((*index, *heard, *how)),
        _ => None,
    }).collect();
    assert!(!closed.iter().any(|(index, _, _)| *index == 1), "the refused track never opened: {closed:?}");
    assert!(closed.iter().any(|(index, heard, how)| *index == 2 && *heard > 0 && *how == chilpa_core::attempts::How::Broken), "the broken one says how much of it went out: {closed:?}");
    assert_eq!(started(&events), [0, 2, 3], "the broken track started, the refused one did not");
    // Said before what follows it starts, so whoever counts them is never late.
    let position = |wanted: &dyn Fn(&PlayerEvent) -> bool| events.iter().position(wanted).unwrap();
    assert!(position(&|e| matches!(e, PlayerEvent::Skipped { index: 1, .. })) < position(&|e| matches!(e, PlayerEvent::Started { index: 2, .. })));
    assert!(position(&|e| matches!(e, PlayerEvent::Skipped { index: 2, .. })) < position(&|e| matches!(e, PlayerEvent::Started { index: 3, .. })));
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })), "{events:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn each_entry_passed_over_says_which_play_it_belongs_to() {
    let dir = folder("player-generation");
    let tracks = [garbage(&dir, "01.flac"), wave(&dir, "02.wav")];
    let player = player();
    // The word carries no play of its own: it comes under the number of the
    // play that said it, as every event does.
    let said = |play: u64| -> Vec<u64> {
        let limit = Instant::now() + Duration::from_secs(20);
        let mut under = Vec::new();
        loop {
            for (of, event) in player.poll_tagged() {
                if matches!(event, PlayerEvent::Skipped { .. }) {
                    under.push(of);
                }
                if of == play && matches!(event, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. } | PlayerEvent::Stopped { .. }) {
                    return under;
                }
            }
            assert!(Instant::now() < limit, "the play never ended");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let first = player.play_from_index(&tracks, "fake", None, false, None, &[], None);
    let first_said = said(first);
    let second = player.play_from_index(&tracks, "fake", None, false, None, &[], None);
    let second_said = said(second);
    assert_ne!(first, second, "each play has its own");
    assert_eq!(first_said, [first]);
    assert_eq!(second_said, [second]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_lap_where_nothing_opens_says_each_once_and_fails_with_the_last_reason() {
    let dir = folder("player-lap");
    let tracks = [empty(&dir, "01.wav"), garbage(&dir, "02.flac")];
    let player = player();
    player.set_repeat(Repeat::All);
    player.play_all(&tracks, "fake");
    let events = events_until_over(&player);
    let passed = events.iter().filter(|event| matches!(event, PlayerEvent::Skipped { .. })).count();
    assert_eq!(passed, tracks.len(), "each one once, one lap and no more");
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })), "{events:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_device_error_is_never_taken_for_the_track() {
    use chilpa_audio::negotiate::NegotiationError;
    use chilpa_audio::pack::PackError;
    use chilpa_decode::DecodeError;
    assert!(PlayError::Decode(DecodeError::Unsupported("no reader".into())).is_track());
    assert!(PlayError::Decode(DecodeError::Open(std::io::ErrorKind::NotFound.into())).is_track());
    assert!(PlayError::Unreadable("the image is short".into()).is_track());
    assert!(!PlayError::Negotiation(NegotiationError::CapsNotProbed).is_track());
    assert!(!PlayError::Negotiation(NegotiationError::Rate { requested_hz: 7_000, offered: vec![44_100] }).is_track());
    assert!(!PlayError::Sink(SinkError::DeviceLost).is_track());
    assert!(!PlayError::Sink(SinkError::Backend("busy".into())).is_track());
    assert!(!PlayError::Pack(PackError::NativeDsdNeedsDsdSource).is_track());
}

/// Sixteen bit stereo at 44.1 kHz, and nothing else.
fn caps_16() -> DeviceCaps {
    DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true }
}

/// A device that keeps every byte it is given.
struct Keep {
    format: StreamFormat,
    kept: Arc<Mutex<Vec<u8>>>,
}

impl AudioSink for Keep {
    fn format(&self) -> StreamFormat {
        self.format
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        self.kept.lock().unwrap().extend_from_slice(frames);
        Ok(frames.len() / self.format.frame_bytes())
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

#[test]
fn a_track_refused_at_open_leaves_no_pending_seek_for_the_next() {
    let dir = folder("seek-refused");
    let tracks = vec![garbage(&dir, "01.flac"), wave(&dir, "02.wav")];
    let controls = Controls::new();
    // Where the refused track was to resume, as a launch's `start_at` asks.
    controls.seek.request(Duration::from_millis(50));
    let kept = Arc::new(Mutex::new(Vec::new()));
    let device = Arc::clone(&kept);
    let results = play_queue_with(
        &SharedQueue::new(tracks),
        &caps_16(),
        Policy::AllowAnything,
        &controls,
        |format| Ok(Keep { format, kept: Arc::clone(&device) }),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("the queue went on past the refused track");
    assert_eq!(results.len(), 1);
    let bytes = kept.lock().unwrap();
    let samples: Vec<i16> = bytes[..8].chunks(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]])).collect();
    assert_eq!(samples, [0, 0, 1, 1], "the next track starts at its beginning, not 50 ms in");
    assert_eq!(bytes.len(), 4_410 * 4, "and plays whole");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_next_asked_while_a_track_is_refused_is_spent_on_passing_it() {
    let dir = folder("next-refused");
    let tracks = [garbage(&dir, "01.flac"), wave(&dir, "02.wav"), wave(&dir, "03.wav")];
    let controls = Controls::new();
    // Next, pressed while the first track was being opened.
    controls.skip.skip();
    let (started, result) = run(&tracks, &controls);
    let results = result.expect("the queue went on");
    assert_eq!(started, [1, 2]);
    assert_eq!(results[0].outcome, Outcome::Finished { frames: 4_410 }, "the track after the refused one played whole");
    let _ = std::fs::remove_dir_all(dir);
}


#[test]
fn a_stop_while_a_track_is_refused_is_no_failure_of_the_queue() {
    let dir = folder("stop-refused");
    let tracks = vec![garbage(&dir, "01.flac"), wave(&dir, "02.wav")];
    let controls = Controls::new();
    let stop = controls.stop.clone();
    let mut started = Vec::new();
    // The stop arrives while the refused track is being opened.
    let result = play_queue_told(
        &SharedQueue::new(tracks),
        &caps(),
        Policy::AllowAnything,
        &controls,
        |format| Ok(Fake { format, written: 0, lost_after: None }),
        &mut |index, _, _, _| started.push(index),
        &mut |_, _| {},
        &mut |_, _, _, _| stop.stop(),
    );
    let results = result.expect("a stop is not the refused track's error");
    assert!(results.is_empty() && started.is_empty(), "nothing started after the stop: {started:?}");
    let _ = std::fs::remove_dir_all(dir);
}

/// A named pipe with an audio name. Opening it waits until something writes
/// to it: the first receiver says the player is in that open, and what is
/// sent on the sender lets it read bytes that are no audio and fail.
#[cfg(unix)]
fn pipe(dir: &Path, name: &str) -> (PathBuf, std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
    let path = dir.join(name);
    assert!(std::process::Command::new("mkfifo").arg(&path).status().unwrap().success());
    let (opened, opened_seen) = std::sync::mpsc::channel();
    let (go, go_seen) = std::sync::mpsc::channel::<()>();
    let writer = path.clone();
    std::thread::spawn(move || {
        let mut pipe = std::fs::OpenOptions::new().write(true).open(writer).unwrap();
        let _ = opened.send(());
        if go_seen.recv().is_ok() {
            let _ = pipe.write_all(&b"this is text and no audio at all. ".repeat(256));
        }
    });
    (path, opened_seen, go)
}

#[cfg(unix)]
#[test]
fn a_stop_while_the_first_track_fails_to_open_ends_the_play_as_stopped() {
    let dir = folder("stop-refusing-first");
    let (pipe, opened, go) = pipe(&dir, "01.flac");
    let tracks = [pipe, wave(&dir, "02.wav")];
    let player = player();
    player.play_all(&tracks, "fake");
    opened.recv_timeout(Duration::from_secs(10)).expect("the player is opening the first track");
    player.stop();
    go.send(()).unwrap();
    let events = events_until_over(&player);
    assert!(started(&events).is_empty(), "nothing started after the stop: {events:?}");
    assert!(matches!(events.last(), Some(PlayerEvent::Stopped { .. })), "a stop, not the track's error: {events:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn a_stop_while_a_later_track_fails_to_open_ends_the_play_as_stopped() {
    let dir = folder("stop-refusing-later");
    let (pipe, opened, go) = pipe(&dir, "02.flac");
    let tracks = [wave(&dir, "01.wav"), pipe];
    let player = player();
    // Without gapless nothing is opened ahead: the pipe is opened once the
    // first track is over.
    player.set_gapless(false);
    player.play_all(&tracks, "fake");
    opened.recv_timeout(Duration::from_secs(10)).expect("the player is opening the second track");
    player.stop();
    go.send(()).unwrap();
    let events = events_until_over(&player);
    assert_eq!(started(&events), [0]);
    assert!(matches!(events.last(), Some(PlayerEvent::Stopped { .. })), "a stop, not a queue that ran out: {events:?}");
    let _ = std::fs::remove_dir_all(dir);
}


#[test]
fn under_repeat_all_a_track_passed_over_comes_up_once_and_not_every_lap() {
    let dir = folder("repeat-all-laps");
    let tracks = vec![wave(&dir, "01.wav"), garbage(&dir, "02.flac"), broken_dsf(&dir, "03.dsf"), wave(&dir, "04.wav")];
    let controls = Controls::new();
    controls.repeat.set(Repeat::All);
    // Nobody drives the queue from outside, as when the window does not read
    // the player's events: after a few laps the test stops it.
    let stop = controls.stop.clone();
    let (mut started, mut passed) = (Vec::new(), Vec::new());
    let result = play_queue_told(
        &SharedQueue::new(tracks),
        &caps(),
        Policy::AllowAnything,
        &controls,
        |format| Ok(Fake { format, written: 0, lost_after: None }),
        &mut |index, _, _, _| {
            started.push(index);
            if started.len() >= 9 {
                stop.stop();
            }
        },
        &mut |_, _| {},
        &mut |index, _, _, _| passed.push(index),
    );
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(passed, [1, 2], "each one is said once: {passed:?}");
    assert_eq!(started.iter().filter(|&&index| index == 2).count(), 1, "the broken track is not heard again each lap: {started:?}");
    assert!(started.iter().filter(|&&index| index == 0).count() >= 3, "it did go round: {started:?}");
    let _ = std::fs::remove_dir_all(dir);
}


#[test]
fn a_stop_mark_on_a_position_left_out_ends_the_queue_there() {
    let dir = folder("mark-left-out");
    let queue = SharedQueue::new(vec![wave(&dir, "01.wav"), wave(&dir, "02.wav"), wave(&dir, "03.wav")]);
    // Left out by whoever drives the queue, with the stop mark on it.
    queue.remove(1);
    let controls = Controls::new();
    controls.stop_after.set(Some(1));
    let mut started = Vec::new();
    let results = play_queue_with(
        &queue,
        &caps(),
        Policy::AllowAnything,
        &controls,
        |format| Ok(Fake { format, written: 0, lost_after: None }),
        &mut |index, _, _| started.push(index),
        &mut |_, _| {},
    )
    .expect("the queue ends at the mark");
    assert_eq!(started, [0], "what follows the mark never plays");
    assert_eq!(results.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

/// A tenth of a second of 24 bit stereo at 44.1 kHz.
fn wave_24(dir: &Path, name: &str) -> PathBuf {
    let (frames, rate) = (4_410u32, 44_100u32);
    let path = dir.join(name);
    let data = frames * 6;
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 6).to_le_bytes()).unwrap();
    f.write_all(&6u16.to_le_bytes()).unwrap();
    f.write_all(&24u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    for i in 0..frames {
        let v = (i % 3_000) * 256 + 1;
        f.write_all(&v.to_le_bytes()[..3]).unwrap();
        f.write_all(&v.to_le_bytes()[..3]).unwrap();
    }
    path
}

#[test]
fn a_track_the_output_cannot_pack_stops_the_queue_as_the_output_s_failure() {
    let dir = folder("pack");
    // Twenty-four bits do not fit in the sixteen this output takes, and the
    // packer refuses to cut them: that is the output's limit, not the file's.
    let tracks = vec![wave_24(&dir, "01.wav"), wave(&dir, "02.wav")];
    let (mut started, mut passed) = (Vec::new(), Vec::new());
    let result = play_queue_told(
        &SharedQueue::new(tracks),
        &caps_16(),
        Policy::AllowAnything,
        &Controls::new(),
        |format| Ok(Fake { format, written: 0, lost_after: None }),
        &mut |index, _, _, _| started.push(index),
        &mut |_, _| {},
        &mut |index, _, _, _| passed.push(index),
    );
    let results = result.expect("the queue reports how the track ended");
    assert!(passed.is_empty(), "the output's failure is not passed over as the track's: {passed:?}");
    assert_eq!(started, [0], "nothing after it started");
    assert!(
        matches!(results.last().map(|r| &r.outcome), Some(Outcome::Failed { .. })),
        "it ends as a failure of the output: {:?}",
        results.last().map(|r| &r.outcome)
    );
    let _ = std::fs::remove_dir_all(dir);
}
