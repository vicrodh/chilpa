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
    f.write_all(&44_100u32.to_le_bytes()).unwrap();
    f.write_all(&176_400u32.to_le_bytes()).unwrap();
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
    let refused = ending_event(Some("no DSD here".into()), Ok(Outcome::Stopped { frames: 0 }));
    assert_eq!(refused, PlayerEvent::Failed { message: "no DSD here".into() }, "one ending, and it is the refusal");
    assert_eq!(ending_event(None, Ok(Outcome::Stopped { frames: 4 })), PlayerEvent::Stopped { frames: 4 });
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
    // Announced, and then let go of with nothing said: the device would not
    // open, or the play unwound.
    let owed = Owed { events: &events, index: 3, announced: std::cell::Cell::new(false) };
    owed.announced.set(true);
    drop(owed);
    // Announced and played.
    let owed = Owed { events: &events, index: 3, announced: std::cell::Cell::new(false) };
    owed.announced.set(true);
    owed.close(480, crate::attempts::How::Stopped);
    drop(owed);
    // Never announced: nothing is owed.
    drop(Owed { events: &events, index: 3, announced: std::cell::Cell::new(false) });
    let said: Vec<PlayerEvent> = heard.try_iter().map(|(_, event)| event).collect();
    assert_eq!(said, [
        PlayerEvent::Closed { attempt: 1, index: 3, heard: 0, how: crate::attempts::How::Failed },
        PlayerEvent::Closed { attempt: 1, index: 3, heard: 480, how: crate::attempts::How::Stopped },
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
