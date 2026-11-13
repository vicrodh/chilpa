//! A play as one launch: its number, the single event it ends with, and the
//! start position that is its own.
//!
//! What these pin down is the hand-off between a play and the one replacing
//! it. A replacement stopped while it still waited behind a stuck play used
//! to vanish without a word, and the window stayed on "playing"; a player
//! made again counted its plays from one, so what the old one said late was
//! taken for the new one's; a start position was served to whatever track
//! came up when the one it was for had been left out.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::DeviceCaps;
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::player::{Player, PlayerEvent};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chilpa-launch-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Sixteen bit stereo at 44.1 kHz, each frame carrying its own number.
fn wave_bytes(frames: u32) -> Vec<u8> {
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
    for i in 0..frames {
        let v = (i % 3_000) as i16;
        out.extend_from_slice(&v.to_le_bytes());
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// A tenth of a second of it, as a file.
fn wave(dir: &Path, name: &str) -> PathBuf {
    long_wave(dir, name, 4_410)
}

fn long_wave(dir: &Path, name: &str, frames: u32) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, wave_bytes(frames)).unwrap();
    path
}

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

/// A device that takes a little at a time, so a long track stays playing,
/// and panics on its next write once `panic` is set.
struct Slow {
    format: StreamFormat,
    panic: Arc<AtomicBool>,
}

impl AudioSink for Slow {
    fn format(&self) -> StreamFormat {
        self.format
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        if self.panic.load(Ordering::Acquire) {
            panic!("the device panicked");
        }
        std::thread::sleep(Duration::from_millis(2));
        Ok((frames.len() / self.format.frame_bytes()).min(64))
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

fn player() -> Player {
    Player::spawn(
        Box::new(|_| Ok(caps_16())),
        Box::new(|_, format, _| Ok(Box::new(Keep { format, kept: Arc::default() }) as Box<dyn AudioSink + Send>)),
    )
}

fn slow_player(panic: &Arc<AtomicBool>) -> Player {
    let panic = Arc::clone(panic);
    Player::spawn(
        Box::new(|_| Ok(caps_16())),
        Box::new(move |_, format, _| Ok(Box::new(Slow { format, panic: Arc::clone(&panic) }) as Box<dyn AudioSink + Send>)),
    )
}

fn terminal(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. } | PlayerEvent::Stopped { .. })
}

/// What the player said, each event with its play, until every play of
/// `plays` was over; then a little longer, for anything said after.
fn events_until_over(player: &Player, plays: &[u64]) -> Vec<(u64, PlayerEvent)> {
    let limit = Instant::now() + Duration::from_secs(20);
    let mut seen: Vec<(u64, PlayerEvent)> = Vec::new();
    loop {
        seen.extend(player.poll_tagged());
        if plays.iter().all(|play| seen.iter().any(|(of, event)| of == play && terminal(event))) {
            break;
        }
        assert!(Instant::now() < limit, "a play never ended: {plays:?} in {seen:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(150));
    seen.extend(player.poll_tagged());
    seen
}

fn wait_started(player: &Player, play: u64, seen: &mut Vec<(u64, PlayerEvent)>) {
    let limit = Instant::now() + Duration::from_secs(20);
    while !seen.iter().any(|(of, event)| *of == play && matches!(event, PlayerEvent::Started { .. })) {
        assert!(Instant::now() < limit, "the play never started: {seen:?}");
        seen.extend(player.poll_tagged());
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Each play of `plays` ended exactly once, said nothing after that, and its
/// events stand together, plays in the order they were asked for.
fn assert_one_ending_each(events: &[(u64, PlayerEvent)], plays: &[u64]) {
    for play in plays {
        let own: Vec<&PlayerEvent> = events.iter().filter(|(of, _)| of == play).map(|(_, event)| event).collect();
        assert_eq!(own.iter().filter(|event| terminal(event)).count(), 1, "play {play} ended once: {own:?}");
        assert!(own.last().is_some_and(|event| terminal(event)), "play {play} said nothing after it ended: {own:?}");
    }
    let order: Vec<u64> = events.iter().map(|(of, _)| *of).filter(|of| plays.contains(of)).collect();
    assert!(order.windows(2).all(|pair| pair[0] <= pair[1]), "each play's events stand together, in order: {order:?}");
}

fn ending_of(events: &[(u64, PlayerEvent)], play: u64) -> PlayerEvent {
    events.iter().find(|(of, event)| *of == play && terminal(event)).map(|(_, event)| event.clone()).expect("the play ended")
}

/// A named pipe with an audio name. Opening it waits until something writes
/// to it: the first receiver says the player is in that open, and what is
/// sent on the sender lets it read bytes that are no audio and fail.
#[cfg(unix)]
fn pipe(dir: &Path, name: &str) -> (PathBuf, Receiver<()>, Sender<()>) {
    let path = dir.join(name);
    assert!(std::process::Command::new("mkfifo").arg(&path).status().unwrap().success());
    let (opened, opened_seen) = channel();
    let (go, go_seen) = channel::<()>();
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
fn a_replacement_stopped_while_it_waits_behind_a_stuck_play_still_ends() {
    let dir = folder("stopped-waiting");
    let (stuck, opened, go) = pipe(&dir, "01.flac");
    let player = player();
    let old = player.play_from_index(&[stuck], "fake", None, false, None, &[], None);
    opened.recv_timeout(Duration::from_secs(10)).expect("the old play is opening its track");
    player.stop_for_handoff();
    let new = player.play_from_index(&[wave(&dir, "02.wav")], "fake", None, false, None, &[], None);
    player.stop();
    // Only now does the old open fail.
    go.send(()).unwrap();
    let events = events_until_over(&player, &[old, new]);
    assert_one_ending_each(&events, &[old, new]);
    assert_eq!(ending_of(&events, new), PlayerEvent::Stopped { frames: 0 }, "it never started, and says so: {events:?}");
    assert!(!events.iter().any(|(of, event)| *of == new && matches!(event, PlayerEvent::Started { .. })), "{events:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shutting_down_ends_every_play_still_waiting_and_any_asked_for_after() {
    let dir = folder("shutdown");
    let mut player = slow_player(&Arc::default());
    let long = long_wave(&dir, "long.wav", 400_000);
    let short = wave(&dir, "short.wav");
    let playing = player.play_from_index(&[long], "fake", None, false, None, &[], None);
    let mut seen = Vec::new();
    wait_started(&player, playing, &mut seen);
    let second = player.play_from_index(&[short.clone()], "fake", None, false, None, &[], None);
    let third = player.play_from_index(&[short.clone()], "fake", None, false, None, &[], None);
    player.shutdown();
    let late = player.play_from_index(&[short], "fake", None, false, None, &[], None);
    seen.extend(events_until_over(&player, &[playing, second, third, late]));
    assert_one_ending_each(&seen, &[playing, second, third, late]);
    assert!(matches!(ending_of(&seen, playing), PlayerEvent::Stopped { .. }), "{seen:?}");
    for waiting in [second, third, late] {
        assert_eq!(ending_of(&seen, waiting), PlayerEvent::Stopped { frames: 0 }, "play {waiting}: {seen:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_panic_while_playing_ends_that_play_as_failed_and_the_waiting_ones_as_stopped() {
    let dir = folder("panic");
    let panic = Arc::new(AtomicBool::new(false));
    let player = slow_player(&panic);
    let long = long_wave(&dir, "long.wav", 400_000);
    let short = wave(&dir, "short.wav");
    let playing = player.play_from_index(&[long], "fake", None, false, None, &[], None);
    let mut seen = Vec::new();
    wait_started(&player, playing, &mut seen);
    let second = player.play_from_index(&[short.clone()], "fake", None, false, None, &[], None);
    let third = player.play_from_index(&[short.clone()], "fake", None, false, None, &[], None);
    panic.store(true, Ordering::Release);
    seen.extend(events_until_over(&player, &[playing, second, third]));
    // The player closed with the panic: what is asked of it now ends at once.
    let late = player.play_from_index(&[short], "fake", None, false, None, &[], None);
    seen.extend(events_until_over(&player, &[late]));
    assert_one_ending_each(&seen, &[playing, second, third, late]);
    assert!(matches!(ending_of(&seen, playing), PlayerEvent::Failed { ref message } if message.contains("the device panicked")), "{seen:?}");
    for waiting in [second, third, late] {
        assert_eq!(ending_of(&seen, waiting), PlayerEvent::Stopped { frames: 0 }, "play {waiting}: {seen:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_second_player_does_not_count_over_the_numbers_of_the_first() {
    let dir = folder("numbers");
    let track = wave(&dir, "01.wav");
    let first = player();
    let a = first.play_from_index(&[track.clone()], "fake", None, false, None, &[], None);
    events_until_over(&first, &[a]);
    drop(first);
    let second = player();
    let b = second.play_from_index(&[track.clone()], "fake", None, false, None, &[], None);
    let c = second.play_from_index(&[track], "fake", None, false, None, &[], None);
    assert!(a < b && b < c, "{a}, {b}, {c}");
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn a_storm_of_replacements_leaves_one_ending_per_play_in_order() {
    let dir = folder("storm");
    let (stuck, opened, go) = pipe(&dir, "01.flac");
    let track = wave(&dir, "02.wav");
    let player = player();
    let mut plays = vec![player.play_from_index(&[stuck], "fake", None, false, None, &[], None)];
    opened.recv_timeout(Duration::from_secs(10)).expect("the first play is opening its track");
    for _ in 0..10 {
        player.stop_for_handoff();
        plays.push(player.play_from_index(&[track.clone()], "fake", None, false, None, &[], None));
    }
    go.send(()).unwrap();
    let events = events_until_over(&player, &plays);
    assert_one_ending_each(&events, &plays);
    let last = *plays.last().unwrap();
    assert!(matches!(ending_of(&events, last), PlayerEvent::Finished { .. }), "the newest one played: {events:?}");
    for replaced in &plays[1..plays.len() - 1] {
        assert_eq!(ending_of(&events, *replaced), PlayerEvent::Stopped { frames: 0 }, "play {replaced}: {events:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

fn samples(bytes: &[u8], count: usize) -> Vec<i16> {
    bytes.chunks(2).take(count).map(|pair| i16::from_le_bytes([pair[0], pair[1]])).collect()
}

#[test]
fn a_start_position_is_not_served_to_the_track_after_a_start_that_was_left_out() {
    let dir = folder("start-left-out");
    let kept = Arc::new(Mutex::new(Vec::new()));
    let device = Arc::clone(&kept);
    let player = Player::spawn(
        Box::new(|_| Ok(caps_16())),
        Box::new(move |_, format, _| Ok(Box::new(Keep { format, kept: Arc::clone(&device) }) as Box<dyn AudioSink + Send>)),
    );
    let tracks = [wave(&dir, "01.wav"), wave(&dir, "02.wav")];
    // It starts on the first track, fifty milliseconds in; the first track
    // was left out of the walk.
    let play = player.play_from_index(&tracks, "fake", Some(Duration::from_millis(50)), false, Some(0), &[true, false], None);
    let events = events_until_over(&player, &[play]);
    assert!(matches!(ending_of(&events, play), PlayerEvent::Finished { .. }), "{events:?}");
    let kept = kept.lock().unwrap();
    assert_eq!(samples(&kept, 4), [0, 0, 1, 1], "the second track plays from its beginning");
    assert_eq!(kept.len(), 4_410 * 4, "and whole");
    let _ = std::fs::remove_dir_all(dir);
}
