//! A track on a server plays through the queue like a file (ADR-017).
//!
//! The server here is a socket on this machine that answers `Range`, and the
//! opener is a fake of the facade's: it says where each track is and what it
//! is called. Nothing leaves the machine.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::DeviceCaps;
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::player::{Player, PlayerEvent};
use chilpa_core::remote::{Described, Located, RemoteOpener};
use chilpa_models::{RemoteEntry, SourceId};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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

fn wav(frames: u32) -> Vec<u8> {
    let data = frames * 4;
    let mut f = Vec::new();
    f.extend_from_slice(b"RIFF");
    f.extend_from_slice(&(36 + data).to_le_bytes());
    f.extend_from_slice(b"WAVEfmt ");
    f.extend_from_slice(&16u32.to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&2u16.to_le_bytes());
    f.extend_from_slice(&44_100u32.to_le_bytes());
    f.extend_from_slice(&176_400u32.to_le_bytes());
    f.extend_from_slice(&4u16.to_le_bytes());
    f.extend_from_slice(&16u16.to_le_bytes());
    f.extend_from_slice(b"data");
    f.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        let v = (i % 1000) as i16;
        f.extend_from_slice(&v.to_le_bytes());
        f.extend_from_slice(&(-v).to_le_bytes());
    }
    f
}

/// Serves one file with `Range`, and counts the requests that carried the
/// header the opener asked for.
fn serve(file: Vec<u8>) -> (String, Arc<AtomicU64>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/audio/file", listener.local_addr().unwrap().port());
    let authorized = Arc::new(AtomicU64::new(0));
    let seen = Arc::clone(&authorized);
    std::thread::spawn(move || {
        for peer in listener.incoming() {
            let Ok(mut peer) = peer else { break };
            let mut reader = BufReader::new(peer.try_clone().unwrap());
            let mut range = (0usize, file.len() - 1);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" { break; }
                let lower = line.to_ascii_lowercase();
                if lower.starts_with("x-token: letmein") {
                    seen.fetch_add(1, Ordering::Relaxed);
                }
                if let Some(v) = lower.strip_prefix("range: bytes=") {
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
        }
    });
    (url, authorized)
}

struct Fake {
    url: String,
    failed: Mutex<Vec<String>>,
}

impl RemoteOpener for Fake {
    fn locate(&self, entry: &RemoteEntry) -> Result<Located, String> {
        if entry.external_id == "gone" {
            return Err("the server does not answer".into());
        }
        // Nothing listens on port 1: the server went away after the queue
        // was built.
        let url = if entry.external_id == "closed" { "http://127.0.0.1:1/audio/file".to_string() } else { self.url.clone() };
        Ok(Located {
            playback: entry.external_id.starts_with("events-").then(|| Arc::new(Recorded(entry.external_id.clone())) as Arc<dyn chilpa_core::playback_events::Observer>),
            request: chilpa_core::RangedRequest { url, headers: vec![("X-Token".into(), "letmein".into())] },
            extension: Some("wav".into()),
            server: "Lab".into(),
        })
    }
    fn describe(&self, entry: &RemoteEntry) -> Described {
        Described {
            tags: chilpa_tags::Tags { title: Some(format!("Song {}", entry.external_id)), ..Default::default() },
            artwork: None,
        }
    }
    fn failed(&self, entry: &RemoteEntry, why: &str) {
        self.failed.lock().unwrap().push(format!("{}: {why}", entry.external_id));
    }
}

fn fake() -> (&'static Fake, Arc<AtomicU64>) {
    static FAKE: OnceLock<(&'static Fake, Arc<AtomicU64>)> = OnceLock::new();
    let (fake, authorized) = FAKE.get_or_init(|| {
        let (url, authorized) = serve(wav(30_000));
        let fake: &'static Fake = Box::leak(Box::new(Fake { url, failed: Mutex::new(Vec::new()) }));
        struct Shared(&'static Fake);
        impl RemoteOpener for Shared {
            fn locate(&self, e: &RemoteEntry) -> Result<Located, String> { self.0.locate(e) }
            fn describe(&self, e: &RemoteEntry) -> Described { self.0.describe(e) }
            fn failed(&self, e: &RemoteEntry, why: &str) { self.0.failed(e, why) }
        }
        chilpa_core::remote::install(Arc::new(Shared(fake)));
        (fake, authorized)
    });
    (*fake, Arc::clone(authorized))
}

fn player(written: Arc<AtomicU64>) -> Player {
    Player::spawn(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
        Box::new(move |_, stream, _| Ok(Box::new(NullSink { stream, written: Arc::clone(&written) }) as Box<dyn AudioSink + Send>)),
    )
}

fn wait(p: &Player, done: impl Fn(&[PlayerEvent]) -> bool) -> Vec<PlayerEvent> {
    let limit = Instant::now() + Duration::from_secs(10);
    let mut all = Vec::new();
    while Instant::now() < limit {
        all.extend(p.poll());
        if done(&all) { break; }
        std::thread::sleep(Duration::from_millis(5));
    }
    all
}

fn entry(id: &str) -> PathBuf {
    PathBuf::from(RemoteEntry::new(SourceId::new("example-server", "one"), id).to_entry())
}

#[test]
fn a_remote_track_plays_whole_with_its_name_and_exact_length() {
    let (_, authorized) = fake();
    let written = Arc::new(AtomicU64::new(0));
    let p = player(Arc::clone(&written));

    p.play_all(&[entry("1"), entry("2")], "fake");
    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. })));

    let started: Vec<_> = events.iter().filter_map(|e| match e {
        PlayerEvent::Started { tags, total_frames, index, .. } => Some((tags.as_ref().and_then(|t| t.title.clone()), *total_frames, *index)),
        _ => None,
    }).collect();
    assert_eq!(started, vec![
        (Some("Song 1".into()), Some(30_000), 0),
        (Some("Song 2".into()), Some(30_000), 1),
    ], "each track named by the library and timed by the decoder");
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { frames: 30_000 })));
    assert_eq!(written.load(Ordering::Relaxed), 60_000, "every frame of both, gapless");
    assert!(authorized.load(Ordering::Relaxed) >= 2, "the opener's header went with each request");
}

#[test]
fn a_remote_track_seeks() {
    fake();
    let written = Arc::new(AtomicU64::new(0));
    let p = player(Arc::clone(&written));
    p.play_all_from(&[entry("3")], "fake", Some(Duration::from_millis(500)));
    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. })));
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })), "{events:?}");
    assert_eq!(written.load(Ordering::Relaxed), 30_000 - 22_050, "it started half a second in");
}

#[test]
fn a_track_whose_server_does_not_answer_is_passed_over() {
    let (fake, _) = fake();
    let written = Arc::new(AtomicU64::new(0));
    let p = player(Arc::clone(&written));
    p.play_all(&[entry("gone"), entry("4"), entry("gone")], "fake");
    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. })));
    let started: Vec<_> = events.iter().filter_map(|e| match e {
        PlayerEvent::Started { index, .. } => Some(*index),
        _ => None,
    }).collect();
    assert_eq!(started, vec![1], "the playable one plays, the others are passed over");
    assert!(matches!(events.last(), Some(PlayerEvent::Finished { .. })), "the queue ends as it would: {events:?}");
    assert_eq!(written.load(Ordering::Relaxed), 30_000);
    assert!(fake.failed.lock().unwrap().iter().filter(|f| f.starts_with("gone:")).count() >= 2, "the facade hears of each, to dim its server");
}

#[test]
fn a_queue_where_nothing_answers_stops_and_is_reported() {
    let (fake, _) = fake();
    let p = player(Arc::new(AtomicU64::new(0)));
    p.play_all(&[entry("gone")], "fake");
    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Failed { .. })));
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { message }) if message.contains("does not answer")), "{events:?}");
    assert!(fake.failed.lock().unwrap().iter().any(|f| f.starts_with("gone:")), "the facade is told which track, to mark its server");
}

#[test]
fn a_server_that_went_away_is_named_in_the_error() {
    fake();
    let p = player(Arc::new(AtomicU64::new(0)));
    p.play_all(&[entry("closed")], "fake");
    let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Failed { .. })));
    assert!(matches!(events.last(), Some(PlayerEvent::Failed { message }) if message.contains("Lab")), "{events:?}");
}


static REPORTS: Mutex<Vec<(String, u64, u32, String)>> = Mutex::new(Vec::new());
struct Recorded(String);
impl chilpa_core::playback_events::Observer for Recorded {
    fn update(&self, frames: u64, rate: u32, _: &'static str, ended: Option<&'static str>) {
        if let Some(reason) = ended {
            REPORTS.lock().unwrap().push((self.0.clone(), frames, rate, reason.into()));
        }
    }
}

#[test]
fn repeated_remote_resources_each_report_their_consumed_audio_once() {
    fake();
    let written = Arc::new(AtomicU64::new(0));
    let mut player = player(written);
    player.play_all(&[entry("events-repeat"), entry("events-repeat")], "fake");
    wait(&player, |events| events.iter().any(|e| matches!(e, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. })));
    player.shutdown();
    let reports = REPORTS.lock().unwrap();
    let reports: Vec<_> = reports.iter().filter(|r| r.0 == "events-repeat").collect();
    assert_eq!(reports.len(), 2);
    for report in reports {
        assert_eq!(report.1, 30_000);
        assert_eq!(report.2, 44_100);
        assert_eq!(report.3, "finished");
    }
}
