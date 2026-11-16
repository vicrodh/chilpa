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
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError, SkipSignal};
use chilpa_core::player::{Player, PlayerEvent};
use chilpa_core::queue_playback::{play_queue_told, Controls, SharedQueue};
use chilpa_core::remote::{Described, Located, Media, RemoteOpener};
use chilpa_models::{RemoteEntry, SourceId};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
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

/// A player whose device keeps what it is given in `kept`, and whose probe
/// says it was entered and waits to be let go when `held` is given.
fn keeping_player(kept: &Arc<Mutex<Vec<u8>>>, held: Option<(Sender<()>, Arc<AtomicBool>)>) -> Player {
    let device = Arc::clone(kept);
    Player::spawn(
        Box::new(move |_| {
            if let Some((entered, go)) = &held {
                let _ = entered.send(());
                let limit = Instant::now() + Duration::from_secs(20);
                while !go.load(Ordering::Acquire) && Instant::now() < limit {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            Ok(caps_16())
        }),
        Box::new(move |_, format, _| Ok(Box::new(Keep { format, kept: Arc::clone(&device) }) as Box<dyn AudioSink + Send>)),
    )
}

#[test]
fn a_seek_asked_after_the_play_was_taken_wins_over_its_start_position() {
    let dir = folder("start-then-seek");
    let kept = Arc::new(Mutex::new(Vec::new()));
    let (entered, entered_seen) = channel();
    let go = Arc::new(AtomicBool::new(false));
    let player = keeping_player(&kept, Some((entered, Arc::clone(&go))));
    let play = player.play_from_index(&[wave(&dir, "01.wav")], "fake", Some(Duration::from_millis(30)), false, Some(0), &[], None);
    entered_seen.recv_timeout(Duration::from_secs(10)).expect("the play was taken and is measuring the device");
    player.seek(Duration::from_millis(60));
    go.store(true, Ordering::Release);
    let events = events_until_over(&player, &[play]);
    assert!(matches!(ending_of(&events, play), PlayerEvent::Finished { .. }), "{events:?}");
    // Sixty milliseconds at 44.1 kHz is frame 2,646.
    assert_eq!(samples(&kept.lock().unwrap(), 4), [2_646, 2_646, 2_647, 2_647], "the later seek is where it starts");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_seek_asked_for_a_start_that_was_left_out_does_not_reach_the_track_after_it() {
    let dir = folder("start-left-out-then-seek");
    let kept = Arc::new(Mutex::new(Vec::new()));
    let (entered, entered_seen) = channel();
    let go = Arc::new(AtomicBool::new(false));
    let player = keeping_player(&kept, Some((entered, Arc::clone(&go))));
    let tracks = [wave(&dir, "01.wav"), wave(&dir, "02.wav")];
    let play = player.play_from_index(&tracks, "fake", Some(Duration::from_millis(30)), false, Some(0), &[true, false], None);
    entered_seen.recv_timeout(Duration::from_secs(10)).expect("the play was taken and is measuring the device");
    player.seek(Duration::from_millis(60));
    go.store(true, Ordering::Release);
    let events = events_until_over(&player, &[play]);
    assert!(matches!(ending_of(&events, play), PlayerEvent::Finished { .. }), "{events:?}");
    let kept = kept.lock().unwrap();
    assert_eq!(samples(&kept, 4), [0, 0, 1, 1], "the second track plays from its beginning");
    assert_eq!(kept.len(), 4_410 * 4, "and whole");
    let _ = std::fs::remove_dir_all(dir);
}

/// Asks for a play while the player is held by another that is stuck opening
/// its track, lets `press` act on the player before the new play is taken,
/// and gives back what the device was handed.
#[cfg(unix)]
fn pressed_before_the_play_was_taken(name: &str, press: impl FnOnce(&Player)) -> Vec<u8> {
    let dir = folder(name);
    let kept = Arc::new(Mutex::new(Vec::new()));
    let (stuck, opened, go) = pipe(&dir, "01.flac");
    let player = keeping_player(&kept, None);
    let old = player.play_from_index(&[stuck], "fake", None, false, None, &[], None);
    opened.recv_timeout(Duration::from_secs(10)).expect("the old play is opening its track");
    player.stop_for_handoff();
    let new = player.play_from_index(&[wave(&dir, "02.wav")], "fake", None, false, None, &[], None);
    press(&player);
    go.send(()).unwrap();
    let events = events_until_over(&player, &[old, new]);
    assert!(matches!(ending_of(&events, new), PlayerEvent::Finished { .. }), "{events:?}");
    let _ = std::fs::remove_dir_all(dir);
    let kept = kept.lock().unwrap().clone();
    kept
}

#[cfg(unix)]
#[test]
fn a_seek_asked_before_the_play_was_taken_reaches_nobody() {
    let kept = pressed_before_the_play_was_taken("seek-before-taken", |player| player.seek(Duration::from_millis(50)));
    assert_eq!(samples(&kept, 4), [0, 0, 1, 1], "the track plays from its beginning");
    assert_eq!(kept.len(), 4_410 * 4);
}

#[cfg(unix)]
#[test]
fn a_next_asked_before_the_play_was_taken_reaches_nobody() {
    let kept = pressed_before_the_play_was_taken("next-before-taken", |player| player.next());
    assert_eq!(kept.len(), 4_410 * 4, "the track plays whole");
}

/// The fake of the facade's opener: an entry's `locate` says it was entered,
/// waits to be let go, and then answers with where its file is served.
#[derive(Default)]
struct Opener {
    gated: Mutex<HashMap<String, (Sender<()>, Arc<AtomicBool>, String)>>,
}

impl RemoteOpener for Opener {
    fn locate(&self, entry: &RemoteEntry) -> Result<Located, String> {
        let (entered, go, url) = self.gated.lock().unwrap().get(&entry.external_id).cloned().ok_or("nobody serves this entry")?;
        let _ = entered.send(());
        let limit = Instant::now() + Duration::from_secs(20);
        while !go.load(Ordering::Acquire) && Instant::now() < limit {
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(Located {
            normalization: None,
            playback: None,
            media: Media::Remote(chilpa_core::RangedRequest { url, headers: Vec::new() }),
            extension: Some("wav".into()),
            server: "Lab".into(),
        })
    }
    fn describe(&self, _: &RemoteEntry) -> Described {
        Described::default()
    }
}

fn opener() -> &'static Opener {
    static OPENER: OnceLock<&'static Opener> = OnceLock::new();
    OPENER.get_or_init(|| {
        let opener: &'static Opener = Box::leak(Box::default());
        struct Shared(&'static Opener);
        impl RemoteOpener for Shared {
            fn locate(&self, e: &RemoteEntry) -> Result<Located, String> { self.0.locate(e) }
            fn describe(&self, e: &RemoteEntry) -> Described { self.0.describe(e) }
        }
        chilpa_core::remote::install(Arc::new(Shared(opener)));
        opener
    })
}

fn entry(id: &str) -> PathBuf {
    PathBuf::from(RemoteEntry::new(SourceId::new("example-server", "one"), id).to_entry())
}

/// Serves `file` by `Range`.
fn serve(file: Vec<u8>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/audio/file", listener.local_addr().unwrap().port());
    let file = Arc::new(file);
    std::thread::spawn(move || {
        for peer in listener.incoming() {
            let Ok(mut peer) = peer else { break };
            let file = Arc::clone(&file);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(peer.try_clone().unwrap());
                let mut range = (0usize, file.len() - 1);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" { break; }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        let (a, b) = v.trim().split_once('-').unwrap();
                        range = (a.parse().unwrap(), b.parse::<usize>().unwrap().min(file.len() - 1));
                    }
                }
                let (a, b) = range;
                let mut reply = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Type: audio/wav\r\nContent-Range: bytes {a}-{b}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    file.len(), b + 1 - a
                ).into_bytes();
                reply.extend_from_slice(&file[a..=b]);
                let _ = peer.write_all(&reply);
            });
        }
    });
    url
}

/// A device that keeps what it is given, and whose drain waits a little for
/// a reason to give up and writes down whether it got one. Once `press_at`
/// frames have gone in, it presses Next.
struct Draining {
    format: StreamFormat,
    kept: Arc<Mutex<Vec<u8>>>,
    drains: Arc<Mutex<Vec<bool>>>,
    press_at: Option<(usize, SkipSignal)>,
}

impl AudioSink for Draining {
    fn format(&self) -> StreamFormat {
        self.format
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let mut kept = self.kept.lock().unwrap();
        kept.extend_from_slice(frames);
        if self.press_at.as_ref().is_some_and(|(at, _)| kept.len() >= at * self.format.frame_bytes()) {
            if let Some((_, next)) = self.press_at.take() {
                // Long enough for the source, read ahead, to have ended.
                std::thread::sleep(Duration::from_millis(60));
                next.skip();
            }
        }
        Ok(frames.len() / self.format.frame_bytes())
    }
    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        let gave_up = (0..10).any(|_| {
            std::thread::sleep(Duration::from_millis(2));
            give_up()
        });
        self.drains.lock().unwrap().push(gave_up);
        Ok(())
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// What a queue left behind: the bytes the device was given, whether each
/// drain was given up, and the frames of each track that played.
struct Ran {
    kept: Vec<u8>,
    drains: Vec<bool>,
    frames: Vec<u64>,
}

/// Plays `tracks`, gapless or with every track drained before the next,
/// pressing Next once `press_at` frames went in, and calling `during` with
/// the controls once the queue is running.
fn run_queue(tracks: Vec<PathBuf>, gapless: bool, press_at: Option<usize>, feed: Option<Arc<dyn chilpa_audio::seek_waveform::Feed>>, during: impl FnOnce(&Controls)) -> Ran {
    let controls = Controls::new();
    controls.gapless.set(gapless);
    *controls.waveform.lock().unwrap() = feed;
    let (kept, drains) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
    let (device, drained, driven) = (Arc::clone(&kept), Arc::clone(&drains), controls.clone());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut press = press_at.map(|at| (at, driven.skip.clone()));
        let result = play_queue_told(
            &SharedQueue::new(tracks),
            &caps_16(),
            Policy::AllowAnything,
            &driven,
            |format| Ok(Draining { format, kept: Arc::clone(&device), drains: Arc::clone(&drained), press_at: press.take() }),
            &mut |_, _, _, _| {},
            &mut |_, _| {},
            &mut |_, _, _, _| {},
        );
        let _ = tx.send(result.map(|results| results.iter().map(|result| result.frames()).collect::<Vec<u64>>()));
    });
    during(&controls);
    let frames = rx.recv_timeout(Duration::from_secs(20)).expect("the queue ended").expect("the queue ran");
    let kept = kept.lock().unwrap().clone();
    let drains = drains.lock().unwrap().clone();
    Ran { kept, drains, frames }
}

const TRACK: usize = 4_410 * 4;

#[test]
fn a_next_asked_while_the_next_track_opens_is_not_taken_by_the_drain_of_the_one_before() {
    let dir = folder("next-while-opening");
    let (entered, entered_seen) = channel();
    let go = Arc::new(AtomicBool::new(false));
    opener().gated.lock().unwrap().insert("slow-to-open".into(), (entered, Arc::clone(&go), serve(wave_bytes(4_410))));
    let ran = run_queue(vec![wave(&dir, "01.wav"), entry("slow-to-open")], false, None, None, |controls| {
        entered_seen.recv_timeout(Duration::from_secs(10)).expect("the second track is being opened");
        controls.skip.skip();
        go.store(true, Ordering::Release);
    });
    assert_eq!(ran.drains.first(), Some(&false), "what was left of the first track played out: {:?}", ran.drains);
    assert_eq!(ran.kept.len(), TRACK, "the second track was skipped: it was the one in front when Next was asked");
    assert_eq!(ran.frames.len(), 2, "{:?}", ran.frames);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_next_asked_in_a_track_s_last_moments_skips_the_one_that_follows_gapless() {
    let dir = folder("next-read-ahead-gapless");
    // Pressed as the last frames of the first track go in: its source ended
    // long before, read ahead, and never sees it.
    let tracks = vec![wave(&dir, "01.wav"), wave(&dir, "02.wav"), wave(&dir, "03.wav")];
    let ran = run_queue(tracks, true, Some(4_410), None, |_| {});
    assert_eq!(ran.frames, [4_410, 0, 4_410], "the second track was skipped, the third played");
    assert_eq!(ran.kept.len(), 2 * TRACK);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_next_asked_in_a_track_s_last_moments_cuts_its_drain_when_there_is_one() {
    let dir = folder("next-read-ahead-drained");
    let ran = run_queue(vec![wave(&dir, "01.wav"), wave(&dir, "02.wav")], false, Some(4_410), None, |_| {});
    assert_eq!(ran.drains, [true, false], "the first track's drain was given up, the second's was not");
    assert_eq!(ran.kept.len(), 2 * TRACK, "the second track played whole");
    assert_eq!(samples(&ran.kept[TRACK..], 4), [0, 0, 1, 1], "from its beginning");
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn a_next_asked_while_a_track_fails_to_open_cuts_nothing_and_skips_nothing() {
    let dir = folder("next-while-failing");
    let (stuck, opened, go) = pipe(&dir, "02.flac");
    let ran = run_queue(vec![wave(&dir, "01.wav"), stuck, wave(&dir, "03.wav")], false, None, None, |controls| {
        opened.recv_timeout(Duration::from_secs(10)).expect("the second track is being opened");
        controls.skip.skip();
        go.send(()).unwrap();
    });
    assert_eq!(ran.drains, [false, false], "what was left of the first track played out");
    assert_eq!(ran.kept.len(), 2 * TRACK, "the third track played whole");
    assert_eq!(samples(&ran.kept[TRACK..], 4), [0, 0, 1, 1], "from its beginning");
    assert_eq!(ran.frames.len(), 2, "the second never played: {:?}", ran.frames);
    let _ = std::fs::remove_dir_all(dir);
}

/// A waveform feed that asks for a seek the first time a track is over: the
/// queue says so right after the track's render has returned, when the
/// thread that read it is gone and nothing is left to serve the seek.
struct SeeksWhenATrackIsOver {
    seek: chilpa_audio::seek::SeekSignal,
    pressed: AtomicBool,
}

impl chilpa_audio::seek_waveform::Feed for SeeksWhenATrackIsOver {
    fn begin(&self, _: &Path, _: u16, _: Option<u64>) {}
    fn feed_float(&self, _: u64, _: &[f32]) {}
    fn feed_int(&self, _: u64, _: &[i32], _: u8) {}
    fn end(&self, _: bool) {
        if !self.pressed.swap(true, Ordering::AcqRel) {
            self.seek.request(Duration::from_millis(50));
        }
    }
}

#[test]
fn a_seek_left_pending_when_a_track_ends_does_not_move_the_next_one() {
    let dir = folder("seek-pending");
    let controls_seek = chilpa_audio::seek::SeekSignal::default();
    let feed = Arc::new(SeeksWhenATrackIsOver { seek: controls_seek.clone(), pressed: AtomicBool::new(false) });
    let tracks = vec![wave(&dir, "01.wav"), wave(&dir, "02.wav")];
    // The queue's own seek slot is the one the feed presses.
    let controls = Controls { seek: controls_seek, ..Controls::new() };
    *controls.waveform.lock().unwrap() = Some(feed.clone());
    let kept = Arc::new(Mutex::new(Vec::new()));
    let device = Arc::clone(&kept);
    let results = play_queue_told(
        &SharedQueue::new(tracks),
        &caps_16(),
        Policy::AllowAnything,
        &controls,
        |format| Ok(Keep { format, kept: Arc::clone(&device) }),
        &mut |_, _, _, _| {},
        &mut |_, _| {},
        &mut |_, _, _, _| {},
    )
    .expect("the queue ran");
    assert!(feed.pressed.load(Ordering::Acquire), "the seek was asked for");
    assert_eq!(results[0].frames(), 4_410, "the first track's own reader did not serve it: it was left pending");
    let kept = kept.lock().unwrap();
    assert_eq!(kept.len(), 2 * TRACK, "the second track played whole");
    assert_eq!(samples(&kept[TRACK..], 4), [0, 0, 1, 1], "from its beginning");
    let _ = std::fs::remove_dir_all(dir);
}

fn spec(paths: &[PathBuf]) -> chilpa_core::launch::LaunchSpec {
    chilpa_core::launch::LaunchSpec { paths: paths.to_vec(), device: "fake".into(), ..Default::default() }
}

/// The positions the play started, in the order it said them.
fn started_of(events: &[(u64, PlayerEvent)], play: u64) -> Vec<usize> {
    events.iter().filter_map(|(of, event)| match event {
        PlayerEvent::Started { index, .. } if *of == play => Some(*index),
        _ => None,
    }).collect()
}

#[test]
fn a_launch_walks_the_order_it_was_given_from_its_first() {
    let dir = folder("own-order");
    let tracks = vec![wave(&dir, "01.wav"), wave(&dir, "02.wav"), wave(&dir, "03.wav")];
    let player = player();
    let launch = player.launch(chilpa_core::launch::LaunchSpec { order: vec![2, 0, 1], ..spec(&tracks) });
    assert_eq!(launch.order(), [2, 0, 1]);
    let play = launch.id().get();
    let events = events_until_over(&player, &[play]);
    assert_one_ending_each(&events, &[play]);
    assert_eq!(started_of(&events, play), [2, 0, 1], "{events:?}");
    assert!(launch.ended());
    // An order that is no permutation of its queue is none.
    let refused = player.launch(chilpa_core::launch::LaunchSpec { order: vec![0, 0, 1], ..spec(&tracks) });
    assert!(refused.order().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

/// The order was the player's, one for every play: setting it for the play
/// about to go out changed how the one on its way out walked what it had
/// left, and what was set on the old one afterwards changed the new one.
#[cfg(unix)]
#[test]
fn an_order_set_on_one_launch_is_not_the_order_of_another() {
    let dir = folder("order-apart");
    let (stuck, opened, go) = pipe(&dir, "01.flac");
    let tracks = vec![wave(&dir, "02.wav"), wave(&dir, "03.wav")];
    let player = player();
    let old = player.launch(chilpa_core::launch::LaunchSpec { order: vec![0, 1], ..spec(&[stuck, tracks[0].clone()]) });
    opened.recv_timeout(Duration::from_secs(10)).expect("the old play is opening its track");
    let new = player.launch(chilpa_core::launch::LaunchSpec { order: vec![1, 0], ..spec(&tracks) });
    assert_eq!(old.order(), [0, 1], "the launch after it did not write its order");
    // What is asked of the old one from here on is not the new one's.
    assert!(old.set_order(vec![0, 1], 2));
    old.clear_order();
    assert_eq!(new.order(), [1, 0]);
    go.send(()).unwrap();
    let plays = [old.id().get(), new.id().get()];
    let events = events_until_over(&player, &plays);
    assert_one_ending_each(&events, &plays);
    assert_eq!(started_of(&events, plays[1]), [1, 0], "it walked its own order: {events:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_launch_stops_the_play_before_it_without_being_asked() {
    let dir = folder("launch-replaces");
    let player = slow_player(&Arc::default());
    let long = long_wave(&dir, "long.wav", 400_000);
    let old = player.launch(spec(&[long]));
    let mut seen = Vec::new();
    wait_started(&player, old.id().get(), &mut seen);
    let new = player.launch(spec(&[wave(&dir, "short.wav")]));
    let plays = [old.id().get(), new.id().get()];
    seen.extend(events_until_over(&player, &plays));
    assert_one_ending_each(&seen, &plays);
    assert!(matches!(ending_of(&seen, plays[0]), PlayerEvent::Stopped { .. }), "the old one was stopped for it: {seen:?}");
    assert!(matches!(ending_of(&seen, plays[1]), PlayerEvent::Finished { .. }), "{seen:?}");
    let _ = std::fs::remove_dir_all(dir);
}

/// A Next, a seek, a stop mark and a removed row asked through a launch
/// reach that launch: the one replacing it plays whole, in its own order.
#[cfg(unix)]
#[test]
fn what_is_asked_of_a_launch_reaches_no_other() {
    let dir = folder("handle-apart");
    let (stuck, opened, go) = pipe(&dir, "01.flac");
    let tracks = vec![wave(&dir, "02.wav"), wave(&dir, "03.wav")];
    let player = player();
    let old = player.launch(spec(&[stuck, tracks[0].clone()]));
    opened.recv_timeout(Duration::from_secs(10)).expect("the old play is opening its track");
    let new = player.launch(spec(&tracks));
    let plays = [old.id().get(), new.id().get()];
    let mut seen = Vec::new();
    // The new one waits behind the old open. Once it has been taken, what
    // is asked of the old one must not move it.
    go.send(()).unwrap();
    wait_started(&player, plays[1], &mut seen);
    old.next();
    old.seek(Duration::from_millis(50));
    old.set_stop_after(Some(0));
    assert!(old.remove(1));
    assert_eq!(old.enqueue(&[tracks[1].clone()]), 3);
    assert_eq!(new.queue_len(), 2, "its queue is its own");
    // The old one ended before the new one started: that is already read.
    seen.extend(events_until_over(&player, &plays[1..]));
    assert_one_ending_each(&seen, &plays);
    assert_eq!(started_of(&seen, plays[1]), [0, 1], "{seen:?}");
    assert_eq!(ending_of(&seen, plays[1]), PlayerEvent::Finished { frames: 4_410 }, "both tracks whole: {seen:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_stop_mark_and_a_removed_row_asked_of_a_launch_are_kept_by_it() {
    let dir = folder("handle-own");
    let player = slow_player(&Arc::default());
    let tracks = vec![long_wave(&dir, "01.wav", 20_000), wave(&dir, "02.wav"), wave(&dir, "03.wav"), wave(&dir, "04.wav")];
    let launch = player.launch(spec(&tracks));
    let play = launch.id().get();
    let mut seen = Vec::new();
    wait_started(&player, play, &mut seen);
    assert!(launch.remove(1));
    launch.set_stop_after(Some(2));
    seen.extend(events_until_over(&player, &[play]));
    assert_eq!(started_of(&seen, play), [0, 2], "the removed row is passed and the mark ends it: {seen:?}");
    let _ = std::fs::remove_dir_all(dir);
}
