//! The player's own tests: what it says while it plays and how it ends.

use super::*;

#[test]
fn a_station_title_is_published_once_and_every_progress_still_is() {
    let (to, heard) = channel();
    let events = Events { to, generation: 1 };
    let now_playing = chilpa_stream::NowPlaying::new();
    now_playing.set(Some("A - One".into()));
    let mut titles = now_playing.changes();
    for frames in 0..1_000 {
        publish_progress(&events, &mut titles, frames);
    }
    now_playing.set(Some("B - Two".into()));
    publish_progress(&events, &mut titles, 1_000);
    let heard: Vec<PlayerEvent> = heard.try_iter().map(|(_, event)| event).collect();
    let songs: Vec<&str> = heard.iter().filter_map(|event| match event {
        PlayerEvent::StreamTitle { title } => Some(title.as_str()),
        _ => None,
    }).collect();
    assert_eq!(songs, ["A - One", "B - Two"]);
    assert_eq!(heard.iter().filter(|event| matches!(event, PlayerEvent::Progress { .. })).count(), 1_001);
}

#[test]
fn a_stop_asked_for_is_how_the_play_ended_unless_the_device_failed() {
    let broken = Outcome::Unreadable { frames: 7, reason: "cut".into() };
    assert_eq!(ending(Some(&broken), false), Outcome::Finished { frames: 7 }, "a queue whose last track broke ran out");
    assert_eq!(ending(Some(&broken), true), Outcome::Stopped { frames: 7 });
    assert_eq!(ending(Some(&Outcome::Finished { frames: 9 }), true), Outcome::Stopped { frames: 9 });
    assert_eq!(ending(None, true), Outcome::Stopped { frames: 0 }, "stopped before anything started");
    assert_eq!(ending(None, false), Outcome::Finished { frames: 0 });
    let failed = Outcome::Failed { frames: 3, error: SinkError::DeviceLost };
    assert_eq!(ending(Some(&failed), true), failed, "a device failure is said as it is");
}

#[test]
fn a_cd_path_is_recognized_by_its_prefix() {
    assert!(is_cd(Path::new("cdda:/dev/sr0")));
    // A bare device node is NOT a CD: opening the drive by mistake
    // startles anyone.
    assert!(!is_cd(Path::new("/dev/sr0")));
    assert!(!is_cd(Path::new("/music/one.flac")));
}

#[test]
fn a_cd_path_yields_its_drive() {
    assert_eq!(cd_device_of(Path::new("cdda:/dev/sr0")), Some(PathBuf::from("/dev/sr0")));
    assert_eq!(cd_device_of(Path::new("cdda:")), None, "with no drive there is nothing to open");
    assert_eq!(cd_device_of(Path::new("/dev/sr0")), None);
}

use chilpa_audio::format::SampleFormat;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct NullSink {
    stream: StreamFormat,
    written: Arc<AtomicU64>,
}

impl AudioSink for NullSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let n = frames.len() / self.stream.frame_bytes();
        self.written.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

struct ClosingGateSink {
    stream: StreamFormat,
    closing: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
}

impl AudioSink for ClosingGateSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        Ok(frames.len() / self.stream.frame_bytes())
    }
    fn close(&mut self) -> Result<(), SinkError> {
        self.closing.store(true, Ordering::Release);
        while !self.release.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        Ok(())
    }
}

fn wav(name: &str, frames: u32) -> PathBuf {
    wav_at(name, frames, 44_100)
}

fn wav_at(name: &str, frames: u32, rate: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-player-{name}.wav"));
    let data = frames * 4;
    let mut f = std::fs::File::create(&p).unwrap();
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
        let v = (i % 1000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&(-v).to_le_bytes()).unwrap();
    }
    p
}

fn player(written: Arc<AtomicU64>) -> Player {
    Player::spawn(
        Box::new(|_| {
            Ok(DeviceCaps {
                sample_rates: vec![44_100],
                formats: vec![SampleFormat::S16],
                channels: vec![2],
                probed: true,
            })
        }),
        Box::new(move |_, stream, _exclusive| {
            Ok(Box::new(NullSink { stream, written: Arc::clone(&written) }) as Box<dyn AudioSink + Send>)
        }),
    )
}

fn wait<F: Fn(&[PlayerEvent]) -> bool>(p: &Player, ready: F) -> Vec<PlayerEvent> {
    let limit = Instant::now() + Duration::from_secs(5);
    let mut all = Vec::new();
    while Instant::now() < limit {
        all.extend(p.poll());
        if ready(&all) {
            return all;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    all
}

#[test]
fn playing_does_not_block_and_reports_progress_and_end() {
    let written = Arc::new(AtomicU64::new(0));
    let p = player(Arc::clone(&written));
    let file = wav("basic", 5_000);

    let started = Instant::now();
    p.play(&file, "fake");
    assert!(started.elapsed() < Duration::from_millis(100), "play() returns immediately");

    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
    assert!(events.iter().any(|e| matches!(e, PlayerEvent::Started { .. })));
    assert!(events.iter().any(|e| matches!(e, PlayerEvent::Progress { .. })));
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { frames: 5_000 })));
    assert_eq!(written.load(Ordering::Relaxed), 5_000);
    let _ = std::fs::remove_file(file);
}

#[test]
fn a_track_moved_while_paused_stays_paused_there() {
    let written = Arc::new(AtomicU64::new(0));
    let p = player(Arc::clone(&written));
    let file = wav("moved-paused", 5_000);
    p.set_paused(true);
    p.play_all_from_paused(&[file.clone()], "fake", None, true);
    std::thread::sleep(Duration::from_millis(300));
    assert!(p.is_paused(), "the pause went with the track");
    assert_eq!(written.load(Ordering::Relaxed), 0, "nothing played while paused");
    p.set_paused(false);
    wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
    assert_eq!(written.load(Ordering::Relaxed), 5_000);
    let _ = std::fs::remove_file(file);
}

#[test]
fn a_new_play_does_not_inherit_an_old_pause() {
    let written = Arc::new(AtomicU64::new(0));
    let p = player(Arc::clone(&written));
    let file = wav("fresh-play", 2_000);
    p.set_paused(true);
    p.play(&file, "fake");
    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { frames: 2_000 })));
    let _ = std::fs::remove_file(file);
}

#[test]
fn rapid_replacements_discard_stale_play_orders_and_their_queues() {
    let closing = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let close_seen = Arc::clone(&closing);
    let close_release = Arc::clone(&release);
    let p = Player::spawn(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
        Box::new(move |_, stream, _| Ok(Box::new(ClosingGateSink { stream, closing: Arc::clone(&close_seen), release: Arc::clone(&close_release) }) as Box<dyn AudioSink + Send>)),
    );
    let a = wav("replacement-a", 400_000);
    let stale = wav("replacement-stale", 1_000);
    let latest = wav("replacement-latest", 1_000);

    p.play(&a, "fake");
    let first = wait(&p, |events| events.iter().any(|event| matches!(event, PlayerEvent::Started { path, .. } if path == &a)));
    p.stop_for_handoff();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !closing.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "the first stream never began closing");
        std::thread::yield_now();
    }
    p.play(&stale, "fake");
    p.stop_for_handoff();
    p.play(&latest, "fake");
    release.store(true, Ordering::Release);

    let mut events = first;
    events.extend(wait(&p, |events| events.iter().any(|event| matches!(event, PlayerEvent::Finished { .. }))));
    let starts: Vec<PathBuf> = events.iter().filter_map(|event| match event {
        PlayerEvent::Started { path, .. } => Some(path.clone()),
        _ => None,
    }).collect();
    assert_eq!(starts, vec![a.clone(), latest.clone()]);

    for path in [a, stale, latest] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn the_exclusive_switch_asks_for_the_output_alone() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&asked);
    let p = Player::spawn(
        Box::new(|_| {
            Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })
        }),
        Box::new(move |_, stream, exclusive| {
            seen.lock().unwrap().push(exclusive);
            Ok(Box::new(NullSink { stream, written: Arc::new(AtomicU64::new(0)) }) as Box<dyn AudioSink + Send>)
        }),
    );
    let file = wav("exclusive", 1_000);
    p.play(&file, "fake");
    wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
    p.set_exclusive(true);
    p.play(&file, "fake");
    wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
    assert_eq!(*asked.lock().unwrap(), vec![false, true], "shared first, alone once asked");
    let _ = std::fs::remove_file(file);
}

#[test]
fn a_queue_plays_in_order_and_reports_each_track_position() {
    let written = Arc::new(AtomicU64::new(0));
    let p = player(Arc::clone(&written));
    let a = wav("cola-a", 1_000);
    let b = wav("cola-b", 2_000);
    p.play_all(&[a.clone(), b.clone()], "fake");

    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
    let startups: Vec<(usize, usize)> = events
        .iter()
        .filter_map(|e| match e {
            PlayerEvent::Started { index, queue_len, .. } => Some((*index, *queue_len)),
            _ => None,
        })
        .collect();
    assert_eq!(startups, vec![(0, 2), (1, 2)], "both tracks, in order");
    assert_eq!(written.load(Ordering::Relaxed), 3_000, "every frame went out");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn stop_after_ends_the_queue_at_the_marked_track() {
    let written = Arc::new(AtomicU64::new(0));
    let p = player(Arc::clone(&written));
    let a = wav("stop-after-a", 2_000);
    let b = wav("stop-after-b", 3_000);
    p.play_all_from_paused(&[a.clone(), b.clone()], "fake", None, true);
    p.set_stop_after(Some(0));
    p.set_paused(false);

    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
    let startups = events.iter().filter(|e| matches!(e, PlayerEvent::Started { .. })).count();
    assert_eq!(startups, 1, "the second track never starts");
    assert_eq!(written.load(Ordering::Relaxed), 2_000, "the marked track finishes cleanly");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn skipping_advances_the_queue_instead_of_stopping_it() {
    let written = Arc::new(AtomicU64::new(0));
    let p = player(Arc::clone(&written));
    // Long tracks: without skipping, the test would take a while to reach the end.
    let a = wav("skip-a", 400_000);
    let b = wav("skip-b", 1_000);
    p.play_all(&[a.clone(), b.clone()], "fake");

    // As soon as the first one starts, a skip is asked for. The events
    // are gathered from both waits: each one consumes what it picked up.
    let mut events =
        wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Started { .. })));
    p.next();
    events.extend(wait(&p, |e| {
        e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. }))
    }));

    let startups = events
        .iter()
        .filter(|e| matches!(e, PlayerEvent::Started { .. }))
        .count();
    assert!(startups >= 2, "the second track started: {startups} startups");
    assert!(
        written.load(Ordering::Relaxed) < 401_000,
        "the first one did not play whole"
    );
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn a_missing_file_is_reported_and_the_player_stays_alive() {
    let p = player(Arc::new(AtomicU64::new(0)));
    p.play(Path::new("/does/not/exist.flac"), "fake");
    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Failed { .. })));
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })));

    let file = wav("after-failing", 1_000);
    p.play(&file, "fake");
    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
    assert!(events.iter().any(|e| matches!(e, PlayerEvent::Finished { .. })), "it still accepts orders");
    let _ = std::fs::remove_file(file);
}

#[test]
fn an_output_that_refused_the_play_is_how_it_ends_whatever_the_queue_returned() {
    let refused = ending_event(Some("no DSD here".into()), Ok(Outcome::Stopped { frames: 0 }), "fake");
    assert_eq!(refused, PlayerEvent::Failed { message: "no DSD here".into() }, "one ending, and it is the refusal");
    assert_eq!(ending_event(None, Ok(Outcome::Stopped { frames: 4 }), "fake"), PlayerEvent::Stopped { frames: 4 });
}

/// Two tracks of 100 sectors of silence; sector 75 will not read.
#[cfg(target_os = "linux")]
struct ScratchedDisc;

#[cfg(target_os = "linux")]
impl chilpa_cd::CdDevice for ScratchedDisc {
    fn read_toc(&mut self) -> Result<(Vec<chilpa_cd::TocEntry>, u32), chilpa_cd::CdError> {
        Ok((
            vec![
                chilpa_cd::TocEntry { number: 1, start_lsn: 0, is_audio: true, session: 1 },
                chilpa_cd::TocEntry { number: 2, start_lsn: 100, is_audio: true, session: 1 },
            ],
            200,
        ))
    }
    fn read_audio(&mut self, lsn: u32, sectors: u32, out: &mut Vec<u8>) -> Result<(), chilpa_cd::CdError> {
        if (lsn..lsn + sectors).contains(&75) {
            return Err(chilpa_cd::CdError::Read { lsn, why: "C2 error".into() });
        }
        out.clear();
        out.resize(sectors as usize * chilpa_cd::SECTOR_BYTES, 0);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_disc_with_a_sector_that_will_not_read_says_so_goes_on_and_ends_once() {
    let (to, heard) = channel();
    let events = Events { to, generation: 7 };
    let caps = DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true };
    let mut open: OpenSink = Box::new(|_, stream, _| Ok(Box::new(NullSink { stream, written: Arc::default() }) as Box<dyn AudioSink + Send>));
    let outcome = play_cd_drive(
        &mut ScratchedDisc,
        Path::new("/dev/fake"),
        Path::new("cdda:/dev/fake"),
        "fake",
        false,
        &caps,
        &crate::queue_playback::Controls::new(),
        &mut open,
        &events,
    );
    let said: Vec<PlayerEvent> = heard.try_iter().map(|(_, event)| event).collect();
    let started: Vec<usize> = said.iter().filter_map(|event| match event {
        PlayerEvent::Started { index, .. } => Some(*index),
        _ => None,
    }).collect();
    assert_eq!(started, [0, 1], "the disc went on to its second track");
    let notices: Vec<u64> = said.iter().filter_map(|event| match event {
        PlayerEvent::Notice { attempt, .. } => Some(*attempt),
        _ => None,
    }).collect();
    assert_eq!(notices, [1], "the cut track is said, against its own turn");
    assert!(!said.iter().any(|event| matches!(event, PlayerEvent::Failed { .. } | PlayerEvent::Finished { .. } | PlayerEvent::Stopped { .. })), "nothing here ends the play: the thread says that once, after: {said:?}");
    assert!(matches!(outcome, Ok(Outcome::Finished { .. })), "{outcome:?}");
}

/// One track of 200 sectors of silence: a little over two and a half
/// seconds.
#[cfg(target_os = "linux")]
struct LongDisc;

#[cfg(target_os = "linux")]
impl chilpa_cd::CdDevice for LongDisc {
    fn read_toc(&mut self) -> Result<(Vec<chilpa_cd::TocEntry>, u32), chilpa_cd::CdError> {
        Ok((vec![chilpa_cd::TocEntry { number: 1, start_lsn: 0, is_audio: true, session: 1 }], 200))
    }
    fn read_audio(&mut self, _: u32, sectors: u32, out: &mut Vec<u8>) -> Result<(), chilpa_cd::CdError> {
        out.clear();
        out.resize(sectors as usize * chilpa_cd::SECTOR_BYTES, 0);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_disc_launched_with_a_start_position_starts_there() {
    let p = Player::spawn_with(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
        Box::new(|_, stream, _| Ok(Box::new(NullSink { stream, written: Arc::default() }) as Box<dyn AudioSink + Send>)),
        Box::new(|_| Ok(Box::new(LongDisc) as Box<dyn chilpa_cd::CdDevice>)),
    );
    // A second and a half in. The first block of a disc is a second long,
    // so a start at the beginning reports 44,100 first, never this much.
    p.play_from_index(&[PathBuf::from("cdda:/dev/fake")], "fake", Some(Duration::from_millis(1_500)), false, Some(0), &[], None);
    let events = wait(&p, |events| events.iter().any(|event| matches!(event, PlayerEvent::Progress { .. })));
    let first = events.iter().find_map(|event| match event {
        PlayerEvent::Progress { frames, .. } => Some(*frames),
        _ => None,
    });
    assert!(first.is_some_and(|at| at >= 66_150), "the disc started where it was asked to: {first:?}");
    p.stop();
}

#[test]
fn a_stream_that_was_announced_is_closed_once_whether_or_not_it_got_to_play() {
    let (to, heard) = channel();
    let events = Events { to, generation: 9 };
    // Announced, and then let go of with nothing said: the play unwound.
    let owed = Owed { events: &events, index: 3, announced: std::cell::Cell::new(false), report: Default::default(), touched: Default::default() };
    owed.announced.set(true);
    drop(owed);
    // Announced and played.
    let owed = Owed { events: &events, index: 3, announced: std::cell::Cell::new(false), report: Default::default(), touched: Default::default() };
    owed.announced.set(true);
    owed.close(480, crate::attempts::How::Stopped);
    drop(owed);
    // Never announced: nothing is owed.
    drop(Owed { events: &events, index: 3, announced: std::cell::Cell::new(false), report: Default::default(), touched: Default::default() });
    let said: Vec<PlayerEvent> = heard.try_iter().map(|(_, event)| event).collect();
    assert_eq!(said, [
        PlayerEvent::Closed { attempt: 1, index: 3, heard: 0, how: crate::attempts::How::Failed, processed: false },
        PlayerEvent::Closed { attempt: 1, index: 3, heard: 480, how: crate::attempts::How::Stopped, processed: false },
    ]);
}

#[cfg(target_os = "linux")]
#[test]
fn a_disc_s_track_cut_by_a_bad_sector_is_closed_as_broken_and_the_next_as_completed() {
    let p = Player::spawn_with(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
        Box::new(|_, stream, _| Ok(Box::new(NullSink { stream, written: Arc::default() }) as Box<dyn AudioSink + Send>)),
        Box::new(|_| Ok(Box::new(ScratchedDisc) as Box<dyn chilpa_cd::CdDevice>)),
    );
    p.play(Path::new("cdda:/dev/fake"), "fake");
    let events = wait(&p, |events| events.iter().any(|event| matches!(event, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. } | PlayerEvent::Stopped { .. })));
    let started: Vec<u64> = events.iter().filter_map(|event| match event {
        PlayerEvent::Started { attempt, .. } => Some(*attempt),
        _ => None,
    }).collect();
    assert_eq!(started, [1, 2], "the disc went on: {events:?}");
    let closed: Vec<(u64, crate::attempts::How)> = events.iter().filter_map(|event| match event {
        PlayerEvent::Closed { attempt, how, .. } => Some((*attempt, *how)),
        _ => None,
    }).collect();
    assert_eq!(closed, [(1, crate::attempts::How::Broken), (2, crate::attempts::How::Completed)], "{events:?}");
    assert!(events.iter().any(|event| matches!(event, PlayerEvent::Notice { attempt: 1, .. })), "{events:?}");
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })), "and ended once, as a disc that played out: {events:?}");
    assert_eq!(events.iter().filter(|event| matches!(event, PlayerEvent::Failed { .. })).count(), 0);
}

/// A real HTTP stream whose only output is the test's in-memory sink.
struct StreamFixture {
    path: PathBuf,
    address: std::net::SocketAddr,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl StreamFixture {
    fn wave(name: &str, frames: u32) -> Self {
        let file = wav(name, frames);
        let bytes = std::fs::read(&file).unwrap();
        std::fs::remove_file(file).unwrap();
        Self::serving(bytes)
    }

    /// Serves `bytes` as a WAV, once.
    fn serving(bytes: Vec<u8>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            use std::io::BufRead;
            let (mut peer, _) = listener.accept().unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            peer.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut reader = std::io::BufReader::new(peer.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 { return; }
                if line == "\r\n" { break; }
            }
            let header = format!("HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len());
            let _ = peer.write_all(header.as_bytes()).and_then(|_| peer.write_all(&bytes));
        });
        Self { path: format!("http://{address}/audio.wav").into(), address, thread: Some(thread) }
    }
}

impl Drop for StreamFixture {
    fn drop(&mut self) {
        let _ = std::net::TcpStream::connect_timeout(&self.address, Duration::from_secs(1));
        self.thread.take().unwrap().join().unwrap();
    }
}

fn finished_events(player: &Player) -> Vec<PlayerEvent> {
    let events = wait(player, |events| events.iter().any(|event| matches!(event,
        PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. } | PlayerEvent::Stopped { .. })));
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. } | PlayerEvent::Stopped { .. })), "no terminal before deadline: {events:?}");
    events
}

fn invalid_start(index: usize, name: &str) {
    let file = wav(name, 5_000);
    {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(written.clone());
        p.launch(crate::launch::LaunchSpec {
            paths: vec![file.clone()], device: "fake".into(), start_index: Some(index),
            start_at: Some(Duration::from_millis(50)), ..Default::default()
        });
        let events = finished_events(&p);
        assert_eq!(written.load(Ordering::Acquire), 5_000, "index {index}: {events:?}");
        assert!(events.iter().any(|event| matches!(event, PlayerEvent::Started { index: 0, .. })));
    }
    std::fs::remove_file(file).unwrap();
}

#[test]
fn an_out_of_range_start_does_not_seek_the_track_that_plays_instead() {
    invalid_start(99, "invalid-start");
}

#[test]
fn the_largest_explicit_start_is_not_an_unspecified_start() {
    invalid_start(usize::MAX, "invalid-start-largest");
}

#[cfg(target_os = "linux")]
fn check_initial_route(name: &str, remote: PathBuf, order: Vec<usize>, start_index: Option<usize>, removed: Vec<bool>, expected_remote: bool) {
    let local = wav(name, 5_000);
    let written = Arc::new(AtomicU64::new(0));
    let output = written.clone();
    let opened_drives = Arc::new(std::sync::Mutex::new(Vec::new()));
    let drives = opened_drives.clone();
    let p = Player::spawn_with(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
        Box::new(move |_, stream, _| Ok(Box::new(NullSink { stream, written: output.clone() }) as Box<dyn AudioSink + Send>)),
        Box::new(move |node| {
            drives.lock().unwrap().push(node.to_path_buf());
            Ok(Box::new(LongDisc) as Box<dyn chilpa_cd::CdDevice>)
        }),
    );
    p.launch(crate::launch::LaunchSpec {
        paths: vec![local.clone(), remote.clone()], device: "fake".into(), order, start_index, removed,
        start_at: Some(Duration::from_millis(50)), stop_after: Some(if expected_remote { 1 } else { 0 }),
        ..Default::default()
    });
    let events = finished_events(&p);
    let starts: Vec<_> = events.iter().filter_map(|event| match event { PlayerEvent::Started { path, .. } => Some(path), _ => None }).collect();
    let expected = if expected_remote { &remote } else { &local };
    assert_eq!(starts, [expected], "{events:?}");
    assert!(written.load(Ordering::Acquire) > 0, "the selected track reached the output");
    if !expected_remote && start_index != Some(0) {
        assert_eq!(written.load(Ordering::Acquire), 5_000, "the removed track's seek was discarded");
    }
    if expected_remote && is_cd(&remote) && start_index == Some(0) {
        assert_eq!(written.load(Ordering::Acquire), 200 * 588, "the omitted local row's seek did not reach the disc");
    }
    assert_eq!(opened_drives.lock().unwrap().len(), usize::from(expected_remote && is_cd(&remote)));
    std::fs::remove_file(local).unwrap();
}

#[cfg(target_os = "linux")]
mod initial_routes {
    use super::*;

    macro_rules! route_case {
        ($name:ident, $cd:expr, $order:expr, $start:expr, $removed:expr, $remote:expr) => {
            #[test]
            fn $name() {
                let stream = (!$cd).then(|| StreamFixture::wave(concat!(stringify!($name), "-http"), 4_410));
                let path = stream.as_ref().map_or_else(|| "cdda:/dev/fake-initial".into(), |stream| stream.path.clone());
                check_initial_route(stringify!($name), path, $order, $start, $removed, $remote);
            }
        };
    }

    route_case!(initial_url_follows_reordered_rows, false, vec![1, 0], None, vec![false, false], true);
    route_case!(initial_url_skips_a_removed_url, false, vec![1, 0], Some(1), vec![false, true], false);
    route_case!(initial_url_follows_a_removed_local_row, false, vec![0, 1], Some(0), vec![true, false], true);
    route_case!(initial_url_respects_a_named_local_row, false, vec![1, 0], Some(0), vec![false, false], false);
    route_case!(initial_cd_follows_reordered_rows, true, vec![1, 0], None, vec![false, false], true);
    route_case!(initial_cd_skips_a_removed_cd, true, vec![1, 0], Some(1), vec![false, true], false);
    route_case!(initial_cd_follows_a_removed_local_row, true, vec![0, 1], Some(0), vec![true, false], true);
    route_case!(initial_cd_respects_a_named_local_row, true, vec![1, 0], Some(0), vec![false, false], false);
}

#[test]
fn an_invalid_named_start_discards_seek_and_next_asked_during_the_probe() {
    let file = wav("invalid-start-probe", 5_000);
    let written = Arc::new(AtomicU64::new(0));
    let output = written.clone();
    let (entered, seen) = channel();
    let (release, released) = channel();
    let p = Player::spawn(
        Box::new(move |_| {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })
        }),
        Box::new(move |_, stream, _| Ok(Box::new(NullSink { stream, written: output.clone() }) as Box<dyn AudioSink + Send>)),
    );
    let launch = p.launch(crate::launch::LaunchSpec {
        paths: vec![file.clone()], device: "fake".into(), start_index: Some(99),
        start_at: Some(Duration::from_millis(50)), ..Default::default()
    });
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    launch.seek(Duration::from_millis(75));
    launch.next();
    release.send(()).unwrap();
    let events = finished_events(&p);
    assert_eq!(written.load(Ordering::Acquire), 5_000, "the omitted start's controls were not transferred: {events:?}");
    std::fs::remove_file(file).unwrap();
}

#[test]
fn a_direct_queue_discards_controls_for_an_out_of_range_start() {
    let file = wav("invalid-direct-start", 5_000);
    let controls = crate::queue_playback::Controls::new();
    controls.start_index.store(99, Ordering::Release);
    controls.seek.request(Duration::from_millis(50));
    controls.skip.skip();
    let written = Arc::new(AtomicU64::new(0));
    let results = crate::queue_playback::play_queue_told(
        &crate::queue_playback::SharedQueue::new(vec![file.clone()]),
        &DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true },
        Policy::AllowAnything, &controls,
        |stream| Ok(NullSink { stream, written: written.clone() }),
        &mut |_, _, _, _| {}, &mut |_, _| {}, &mut |_, _, _, _| {},
    ).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(written.load(Ordering::Acquire), 5_000);
    std::fs::remove_file(file).unwrap();
}

#[derive(Clone, Copy)]
enum StreamFailure { Drain, Close, DrainPanic, ClosePanic, WritePanic, DeviceLost }

struct FailingStreamSink {
    format: StreamFormat,
    failure: StreamFailure,
    written: Arc<AtomicU64>,
    discarded: bool,
}

impl AudioSink for FailingStreamSink {
    fn format(&self) -> StreamFormat { self.format }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        if self.written.load(Ordering::Acquire) > 0 {
            match self.failure {
                StreamFailure::WritePanic => { self.discarded = true; panic!("stream output write panicked"); }
                StreamFailure::DeviceLost => { self.discarded = true; return Err(SinkError::DeviceLost); }
                _ => {}
            }
        }
        let frames = (bytes.len() / self.format.frame_bytes()).min(64);
        self.written.fetch_add(frames as u64, Ordering::Release);
        Ok(frames)
    }
    fn held(&self) -> u64 {
        if self.discarded { 0 } else { self.written.load(Ordering::Acquire).min(5) }
    }
    fn drain(&mut self, _: &dyn Fn() -> bool) -> Result<(), SinkError> {
        self.discarded = true;
        match self.failure {
            StreamFailure::Drain => Err(SinkError::DeviceLost),
            StreamFailure::DrainPanic => panic!("stream output drain panicked"),
            _ => Ok(()),
        }
    }
    fn close(&mut self) -> Result<(), SinkError> {
        match self.failure {
            StreamFailure::Close => Err(SinkError::DeviceLost),
            StreamFailure::ClosePanic => panic!("stream output close panicked"),
            _ => Ok(()),
        }
    }
}

fn stream_failure(failure: StreamFailure, name: &str) {
    let stream = StreamFixture::wave(name, 4_410);
    let written = Arc::new(AtomicU64::new(0));
    let output = written.clone();
    let p = Player::spawn(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
        Box::new(move |_, format, _| Ok(Box::new(FailingStreamSink { format, failure, written: output.clone(), discarded: false }) as Box<dyn AudioSink + Send>)),
    );
    p.play(&stream.path, "fake");
    let events = finished_events(&p);
    let progress = events.iter().filter_map(|event| match event { PlayerEvent::Progress { played, .. } => Some(*played), _ => None }).max().unwrap_or(0);
    assert!(progress > 0, "real decoded stream audio reached the output: {events:?}");
    let closed: Vec<_> = events.iter().filter_map(|event| match event {
        PlayerEvent::Closed { heard, how, .. } => Some((*heard, *how)), _ => None,
    }).collect();
    let sent = written.load(Ordering::Acquire);
    let expected = if matches!(failure, StreamFailure::Close | StreamFailure::ClosePanic) { sent } else { sent - 5 };
    assert_eq!(closed, [(expected, crate::attempts::How::Failed)], "keep the meter's last sample, including on unwind: {events:?}");
    assert!(closed[0].0 >= progress);
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })), "{events:?}");
    assert_eq!(events.iter().filter(|event| matches!(event, PlayerEvent::Started { .. })).count(), 1);
}

#[test]
fn a_real_stream_keeps_its_heard_count_when_drain_fails() { stream_failure(StreamFailure::Drain, "stream-drain-error"); }
#[test]
fn a_real_stream_keeps_its_heard_count_when_close_fails() { stream_failure(StreamFailure::Close, "stream-close-error"); }
#[test]
fn a_real_stream_keeps_its_last_sample_when_drain_panics() { stream_failure(StreamFailure::DrainPanic, "stream-drain-panic"); }
#[test]
fn a_real_stream_keeps_its_heard_count_when_close_panics() { stream_failure(StreamFailure::ClosePanic, "stream-close-panic"); }
#[test]
fn a_real_stream_keeps_its_last_sample_when_write_panics() { stream_failure(StreamFailure::WritePanic, "stream-write-panic"); }
#[test]
fn a_real_stream_keeps_its_heard_count_when_the_device_is_lost() { stream_failure(StreamFailure::DeviceLost, "stream-device-lost"); }

/// Keeps every byte the output was handed.
struct RecordingSink {
    stream: StreamFormat,
    bytes: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl AudioSink for RecordingSink {
    fn format(&self) -> StreamFormat { self.stream }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let n = frames.len() / self.stream.frame_bytes();
        self.bytes.lock().unwrap().extend_from_slice(&frames[..n * self.stream.frame_bytes()]);
        Ok(n)
    }
    fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
}

/// The samples a real HTTP stream reached a 16-bit output with, the
/// player's volume set to `factor` before it started.
fn heard(stream: StreamFixture, factor: f32) -> Vec<i16> {
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let output = bytes.clone();
    let p = Player::spawn(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
        Box::new(move |_, stream, _| Ok(Box::new(RecordingSink { stream, bytes: output.clone() }) as Box<dyn AudioSink + Send>)),
    );
    p.set_volume(factor);
    p.play(&stream.path, "fake");
    let events = finished_events(&p);
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })), "{events:?}");
    let samples = bytes.lock().unwrap().as_chunks::<2>().0.iter().map(|pair| i16::from_le_bytes(*pair)).collect();
    samples
}

fn stream_at_volume(name: &str, factor: f32) -> Vec<i16> {
    heard(StreamFixture::wave(name, 4_410), factor)
}

/// A stereo 32-bit float WAV holding `level` and its negative on every frame.
fn float_wave(frames: u32, level: f32) -> Vec<u8> {
    let data = frames * 8;
    let mut bytes = Vec::with_capacity(44 + data as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&44_100u32.to_le_bytes());
    bytes.extend_from_slice(&352_800u32.to_le_bytes());
    bytes.extend_from_slice(&8u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for _ in 0..frames {
        bytes.extend_from_slice(&level.to_le_bytes());
        bytes.extend_from_slice(&(-level).to_le_bytes());
    }
    bytes
}

#[test]
fn a_stream_is_scaled_by_the_player_volume_like_a_file() {
    let untouched = stream_at_volume("stream-volume-unity", 1.0);
    assert!(untouched.iter().any(|sample| *sample != 0), "the stream carries sound");
    let muted = stream_at_volume("stream-volume-muted", 0.0);
    assert_eq!(muted.len(), untouched.len());
    assert!(muted.iter().all(|sample| *sample == 0), "a muted player plays a stream silent");
    let quarter = stream_at_volume("stream-volume-quarter", 0.25);
    assert_eq!(quarter.len(), untouched.len());
    assert!(quarter.iter().zip(&untouched).all(|(low, full)| (f32::from(*low) - f32::from(*full) * 0.25).abs() <= 1.0),
        "each sample is a quarter of itself, its sign kept, give or take the rounding");
}

#[test]
fn a_stream_s_floats_are_scaled_before_the_integer_output_as_a_file_s() {
    // Decoded lossy audio goes past full scale. A file is scaled while it is
    // still float, so a lowered volume brings the overs back instead of
    // clipping them first and halving the clipped samples.
    let heard = heard(StreamFixture::serving(float_wave(1_000, 1.5)), 0.5);
    assert_eq!(heard.len(), 2_000);
    assert!(heard.as_chunks::<2>().0.iter().all(|pair| *pair == [24_576, -24_576]), "{:?}", &heard[..4]);
}

#[test]
fn a_panic_releasing_pending_audio_marks_every_release_path_as_failed() {
    use crate::playback_events::Measured;
    struct PanickingRelease { format: StreamFormat, discarded: bool }
    impl AudioSink for PanickingRelease {
        fn format(&self) -> StreamFormat { self.format }
        fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> { Ok(bytes.len() / self.format.frame_bytes()) }
        fn held(&self) -> u64 { if self.discarded { 0 } else { 5 } }
        fn close(&mut self) -> Result<(), SinkError> { self.discarded = true; panic!("close panicked"); }
        fn handoff(&mut self) -> Result<(), SinkError> { self.discarded = true; panic!("handoff panicked"); }
        fn flush(&mut self) -> Result<(), SinkError> { self.discarded = true; panic!("flush panicked"); }
        fn suspend(&mut self) -> Result<(), SinkError> { self.discarded = true; panic!("suspend panicked"); }
        fn set_paused(&mut self, _: bool) -> Result<(), SinkError> { self.discarded = true; panic!("pause panicked"); }
        fn wake(&mut self) -> Result<(), SinkError> { self.discarded = true; panic!("wake panicked"); }
    }
    let mut results = Vec::new();
    for operation in ["close", "handoff", "flush", "suspend", "pause", "wake"] {
        let mut said = Vec::new();
        {
            let mut tell = |closed| said.push(closed);
            let mut attempts = crate::attempts::Attempts::new(&mut tell, StopSignal::new());
            attempts.open(1, 0);
            let mut sink = Measured::reporting(PanickingRelease { format: StreamFormat::new(SampleFormat::S16, 44_100, 2), discarded: false }, attempts.report());
            sink.begin_attempt(1, None);
            sink.write(&[0; 256]).unwrap();
            sink.end("finished");
            attempts.ended(1, crate::attempts::How::Completed);
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                match operation {
                    "close" => sink.close(), "handoff" => sink.handoff(),
                    "flush" => sink.flush(), "suspend" => sink.suspend(),
                    "pause" => sink.set_paused(true), _ => sink.wake(),
                }.unwrap();
            })).is_err());
        }
        assert_eq!(said.len(), 1);
        results.push((operation, said[0].heard, said[0].how));
    }
    assert_eq!(results, ["close", "handoff", "flush", "suspend", "pause", "wake"].map(|operation| (operation, 59, crate::attempts::How::Failed)));
}

/// Lowers the player's volume once it has been handed its first block,
/// as a listener does halfway through a track; with no volume to lower it
/// only counts.
struct LoweringSink {
    stream: StreamFormat,
    volume: Arc<std::sync::OnceLock<crate::queue_playback::Level>>,
    written: bool,
}

impl AudioSink for LoweringSink {
    fn format(&self) -> StreamFormat { self.stream }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        if !std::mem::replace(&mut self.written, true) {
            if let Some(volume) = self.volume.get() { volume.set(0.5); }
        }
        Ok(frames.len() / self.stream.frame_bytes())
    }
    fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
}

/// What the `Closed` of `path` says of its samples, the volume lowered
/// after its first block or left alone.
fn closed_as_processed(path: &Path, lower: bool) -> Option<bool> {
    let volume = Arc::new(std::sync::OnceLock::new());
    let held = volume.clone();
    let p = Player::spawn(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
        Box::new(move |_, stream, _| Ok(Box::new(LoweringSink { stream, volume: held.clone(), written: false }) as Box<dyn AudioSink + Send>)),
    );
    if lower {
        volume.set(p.controls.volume.clone()).unwrap();
    }
    p.play(path, "fake");
    let events = finished_events(&p);
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })), "{events:?}");
    events.iter().find_map(|event| match event { PlayerEvent::Closed { processed, .. } => Some(*processed), _ => None })
}

#[test]
fn a_track_whose_volume_moved_after_it_started_is_closed_as_processed() {
    let file = wav("volume-moved", 44_100);
    assert_eq!(closed_as_processed(&file, false), Some(false), "a file left alone");
    assert_eq!(closed_as_processed(&file, true), Some(true), "a file lowered halfway");
    std::fs::remove_file(file).unwrap();
    let stream = StreamFixture::wave("volume-moved-stream", 44_100);
    assert_eq!(closed_as_processed(&stream.path, true), Some(true), "a stream lowered halfway");
    let stream = StreamFixture::wave("volume-left-stream", 44_100);
    assert_eq!(closed_as_processed(&stream.path, false), Some(false), "a stream left alone");
}

/// What the `Closed` of `path` says of its samples, changed from the start
/// by a lowered volume or by DSP, and then cut by `failure` at the output.
fn closed_after_failure(path: &Path, failure: StreamFailure, by_dsp: bool) -> Vec<bool> {
    let written = Arc::new(AtomicU64::new(0));
    let output = written.clone();
    let p = Player::spawn(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
        Box::new(move |_, format, _| Ok(Box::new(FailingStreamSink { format, failure, written: output.clone(), discarded: false }) as Box<dyn AudioSink + Send>)),
    );
    if by_dsp {
        p.dsp().set(Some(chilpa_audio::dsp::Preset { preamp_db: -6.0, ..Default::default() })).unwrap();
    } else {
        p.set_volume(0.5);
    }
    p.play(path, "fake");
    let events = finished_events(&p);
    assert!(written.load(Ordering::Acquire) > 0, "changed audio reached the output first: {events:?}");
    events.iter().filter_map(|event| match event { PlayerEvent::Closed { processed, .. } => Some(*processed), _ => None }).collect()
}

#[test]
fn a_scaled_stream_whose_drain_failed_is_closed_as_processed() {
    let stream = StreamFixture::wave("scaled-then-drain-failed", 4_410);
    assert_eq!(closed_after_failure(&stream.path, StreamFailure::Drain, false), [true]);
}

#[test]
fn a_scaled_stream_that_unwound_is_closed_as_processed() {
    let stream = StreamFixture::wave("scaled-stream-then-write-panicked", 4_410);
    assert_eq!(closed_after_failure(&stream.path, StreamFailure::WritePanic, false), [true]);
}

#[test]
fn a_scaled_file_that_unwound_is_closed_as_processed() {
    let file = wav("scaled-file-then-write-panicked", 4_410);
    assert_eq!(closed_after_failure(&file, StreamFailure::WritePanic, false), [true]);
    std::fs::remove_file(file).unwrap();
}

#[test]
fn a_file_the_dsp_changed_and_that_unwound_is_closed_as_processed() {
    // The volume stays at unity: only the DSP's witness can say it.
    let file = wav("dsp-then-write-panicked", 4_410);
    assert_eq!(closed_after_failure(&file, StreamFailure::WritePanic, true), [true]);
    std::fs::remove_file(file).unwrap();
}

/// A machine whose outputs open only where `opens` says: every output the
/// play asks for is recorded, in order, and the others refuse the way ALSA
/// refused `default` beside a foreign `libpipewire`.
#[cfg(target_os = "linux")]
fn opening_only(opens: &'static [&'static str], asked: Arc<std::sync::Mutex<Vec<String>>>) -> Player {
    // Only at 44.1 kHz, so a play that reopens at another rate finds every
    // route refusing.
    opening_only_at(opens, &[44_100], asked)
}

/// As `opening_only`, with the rates at which those outputs open.
#[cfg(target_os = "linux")]
fn opening_only_at(opens: &'static [&'static str], rates: &'static [u32], asked: Arc<std::sync::Mutex<Vec<String>>>) -> Player {
    Player::spawn(
        Box::new(|_| {
            Ok(DeviceCaps { sample_rates: vec![44_100, 48_000], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })
        }),
        Box::new(move |id, stream, _| {
            asked.lock().unwrap().push(id.to_string());
            if opens.contains(&id) && rates.contains(&stream.sample_rate_hz) {
                Ok(Box::new(NullSink { stream, written: Arc::new(AtomicU64::new(0)) }) as Box<dyn AudioSink + Send>)
            } else {
                Err(SinkError::Backend(format!("snd_pcm_open failed on {id}: No such device or address (6)")))
            }
        }),
    )
}

#[cfg(target_os = "linux")]
fn ended(p: &Player) -> Vec<PlayerEvent> {
    wait(p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. })))
}

#[cfg(target_os = "linux")]
#[test]
fn the_standing_output_that_will_not_open_still_plays() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let p = opening_only(&["pulse:"], Arc::clone(&asked));
    let file = wav("standing-falls-back", 1_000);
    p.play(&file, "system:default");
    let events = ended(&p);
    assert!(events.iter().any(|e| matches!(e, PlayerEvent::Finished { .. })), "it played: {events:?}");
    let started = events.iter().find_map(|e| match e {
        PlayerEvent::Started { route, bit_perfect, .. } => Some((route.device.clone(), *bit_perfect)),
        _ => None,
    });
    assert_eq!(started, Some(("pulse:".to_string(), false)), "the route is where it went, and a server is never exact");
    assert_eq!(asked.lock().unwrap()[..2], ["system:default".to_string(), "pulse:".to_string()]);
    let _ = std::fs::remove_file(file);
}

#[cfg(target_os = "linux")]
#[test]
fn a_play_that_fell_back_says_where_once_and_stays_there() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let p = opening_only_at(&["alsapcm:pipewire"], &[44_100, 48_000], Arc::clone(&asked));
    // The second track is at another rate, so the output is opened again,
    // and the route the play went to takes it.
    let (a, b) = (wav("fell-back-a", 1_000), wav_at("fell-back-b", 1_000, 48_000));
    p.play_all(&[a.clone(), b.clone()], "system:default");
    let events = ended(&p);
    assert_eq!(events.iter().filter(|e| matches!(e, PlayerEvent::Started { .. })).count(), 2, "both played: {events:?}");
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })), "{events:?}");
    let said: Vec<&str> = events.iter().filter_map(|e| match e {
        PlayerEvent::FellBack { message } => Some(message.as_str()),
        _ => None,
    }).collect();
    assert_eq!(said.len(), 1, "said once: {events:?}");
    assert!(said[0].contains("PipeWire"), "{}", said[0]);
    let asked = asked.lock().unwrap();
    assert_eq!(asked[..3], ["system:default".to_string(), "pulse:".to_string(), "alsapcm:pipewire".to_string()]);
    assert!(!asked[3..].is_empty(), "the second track opened the output again: {asked:?}");
    assert!(asked[3..].iter().all(|id| id == "alsapcm:pipewire"), "a next opening goes where the play went: {asked:?}");
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[cfg(target_os = "linux")]
#[test]
fn an_output_somebody_chose_is_never_played_somewhere_else() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let p = opening_only(&["pulse:", "alsapcm:pipewire"], Arc::clone(&asked));
    let file = wav("chosen-refuses", 1_000);
    p.play(&file, "alsa:hw:7,0");
    let events = ended(&p);
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })), "{events:?}");
    assert!(!events.iter().any(|e| matches!(e, PlayerEvent::FellBack { .. })));
    assert_eq!(*asked.lock().unwrap(), ["alsa:hw:7,0".to_string()]);
    let _ = std::fs::remove_file(file);
}

#[cfg(target_os = "linux")]
#[test]
fn when_no_route_opens_each_is_tried_once_and_the_default_s_answer_is_said() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let p = opening_only(&[], Arc::clone(&asked));
    let file = wav("nothing-opens", 1_000);
    p.play(&file, "system:default");
    let events = ended(&p);
    match events.last() {
        Some(PlayerEvent::Failed { message }) => assert!(message.contains("on system:default"), "{message}"),
        other => panic!("it ends failed: {other:?}"),
    }
    let asked = asked.lock().unwrap();
    assert_eq!(asked[..3], ["system:default".to_string(), "pulse:".to_string(), "alsapcm:pipewire".to_string()]);
    let mut once = asked.clone();
    once.sort();
    once.dedup();
    assert_eq!(once.len(), asked.len(), "no route asked twice: {asked:?}");
    assert!(asked.len() <= 3 + crate::backends::MOST_PLUG_ROUTES, "{asked:?}");
    let _ = std::fs::remove_file(file);
}

#[cfg(target_os = "linux")]
#[test]
fn a_reopening_that_finds_no_route_ends_with_the_default_s_answer() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let p = opening_only(&["pulse:"], Arc::clone(&asked));
    // The second track is at another rate: the output is opened again, and
    // nothing opens at that rate.
    let (a, b) = (wav("reopen-a", 1_000), wav_at("reopen-b", 1_000, 48_000));
    p.play_all(&[a.clone(), b.clone()], "system:default");
    let events = ended(&p);
    let asked = asked.lock().unwrap();
    assert!(asked.iter().filter(|id| *id == "pulse:").count() >= 2, "the reopening started where the play went: {asked:?}");
    match events.last() {
        Some(PlayerEvent::Failed { message }) => assert!(message.contains("on system:default"), "{message}"),
        other => panic!("it ends failed: {other:?}"),
    }
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[cfg(target_os = "linux")]
#[test]
fn a_stream_that_fell_back_announces_the_route_it_opened() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let p = opening_only(&["pulse:"], Arc::clone(&asked));
    let stream = StreamFixture::wave("stream-falls-back", 4_410);
    p.play(&stream.path, "system:default");
    let events = ended(&p);
    let routes: Vec<String> = events.iter().filter_map(|e| match e {
        PlayerEvent::Started { route, .. } => Some(route.device.clone()),
        _ => None,
    }).collect();
    assert_eq!(routes, ["pulse:".to_string()], "{events:?}");
    let fell = events.iter().position(|e| matches!(e, PlayerEvent::FellBack { .. }));
    let started = events.iter().position(|e| matches!(e, PlayerEvent::Started { .. }));
    assert!(fell.is_some() && fell < started, "said where before it started: {events:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn a_stream_whose_output_will_not_open_announces_nothing() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let p = opening_only(&[], Arc::clone(&asked));
    let stream = StreamFixture::wave("stream-opens-nothing", 4_410);
    p.play(&stream.path, "system:default");
    let events = ended(&p);
    assert!(!events.iter().any(|e| matches!(e, PlayerEvent::Started { .. })), "{events:?}");
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })), "{events:?}");
}
