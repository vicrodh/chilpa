//! A track that cannot be played, crossed with something else happening at
//! the same moment: a seek or a Next aimed at it, a copy of it further on, a
//! play that replaces the one it belongs to.
//!
//! Each of these left something of the broken track, or of the play that was
//! replaced, in the hands of what came next: a seek served to the following
//! track, a stop mark spent by a play on its way out, a failure reported
//! against a queue that never asked for it.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::render::Outcome;
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_told, Controls, DsdMode, SharedQueue, TrackResult};
use chilpa_core::remote::{Described, Located, Media, RemoteOpener};
use chilpa_models::{RemoteEntry, SourceId};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const DSD64: u64 = 2_822_400;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chilpa-unreadable-races-{name}"));
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
    let path = dir.join(name);
    std::fs::write(&path, wave_bytes(4_410)).unwrap();
    path
}

/// Two channel DSD64 silence as a DSF, `samples` one-bit samples long, with
/// its last `cut` bytes missing: cut, it opens, plays and breaks at the end.
fn dsf(dir: &Path, name: &str, samples: u64, cut: usize) -> PathBuf {
    let (channels, block) = (2u32, 4096u32);
    let per_channel = ((samples / 8) as usize).div_ceil(block as usize) * block as usize;
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
    out.extend_from_slice(&samples.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_chunk.to_le_bytes());
    out.resize(out.len() + data_len - cut, 0x69);
    let path = dir.join(name);
    std::fs::write(&path, out).unwrap();
    path
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

/// A device that takes what it is given at once.
struct Swallow(StreamFormat);

impl AudioSink for Swallow {
    fn format(&self) -> StreamFormat {
        self.0
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        Ok(frames.len() / self.0.frame_bytes())
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// The fake of the facade's opener. Each test registers where its entry is.
#[derive(Default)]
struct Opener {
    /// Where an entry's file is served.
    served: Mutex<HashMap<String, String>>,
}

impl RemoteOpener for Opener {
    fn locate(&self, entry: &RemoteEntry) -> Result<Located, String> {
        let url = self.served.lock().unwrap().get(&entry.external_id).cloned().ok_or("nobody serves this entry")?;
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

fn wait_for(flag: &AtomicBool) {
    let limit = Instant::now() + Duration::from_secs(20);
    while !flag.load(Ordering::Acquire) && Instant::now() < limit {
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Serves `file` by `Range`. With `stall`, only its first window is sent: a
/// request for anything after it is held until `stall` is set and then
/// dropped unanswered, which is a read that blocks and then fails.
fn serve(file: Vec<u8>, stall: Option<Arc<AtomicBool>>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/audio/file", listener.local_addr().unwrap().port());
    let file = Arc::new(file);
    std::thread::spawn(move || {
        for peer in listener.incoming() {
            let Ok(mut peer) = peer else { break };
            let (file, stall) = (Arc::clone(&file), stall.clone());
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
                if let Some(stall) = stall.filter(|_| a > 0) {
                    wait_for(&stall);
                    return;
                }
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

/// Plays a remote track whose second window never arrives, then a file. Once
/// the device has everything the first window held and the read of the
/// second is blocked, `aim` is called with the controls; then the read
/// fails. Returns what the device was given after that point.
fn after_a_blocked_read_fails(id: &str, aim: impl FnOnce(&Controls)) -> Vec<u8> {
    let dir = folder(id);
    let stall = Arc::new(AtomicBool::new(false));
    // Four and a half seconds: more than the first window carries.
    opener().served.lock().unwrap().insert(id.into(), serve(wave_bytes(200_000), Some(Arc::clone(&stall))));
    let tracks = vec![entry(id), wave(&dir, "02.wav")];
    let controls = Controls::new();
    let kept = Arc::new(Mutex::new(Vec::new()));
    let (device, driven) = (Arc::clone(&kept), controls.clone());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut passed = Vec::new();
        let result = play_queue_told(
            &SharedQueue::new(tracks),
            &caps_16(),
            Policy::AllowAnything,
            &driven,
            |format| Ok(Keep { format, kept: Arc::clone(&device) }),
            &mut |_, _, _, _| {},
            &mut |_, _| {},
            &mut |index, _, _, played| passed.push((index, played)),
        );
        let _ = tx.send((passed, result.map(|results| results.len())));
    });
    // The device stops being fed once the first window has gone out: the
    // source is then inside the read of the second one.
    let limit = Instant::now() + Duration::from_secs(20);
    let mut seen = (0usize, Instant::now());
    loop {
        assert!(Instant::now() < limit, "the first window never went out");
        let now = kept.lock().unwrap().len();
        if now != seen.0 {
            seen = (now, Instant::now());
        } else if now >= 100_000 * 4 && seen.1.elapsed() >= Duration::from_millis(300) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let before = seen.0;
    aim(&controls);
    stall.store(true, Ordering::Release);
    let (passed, played) = rx.recv_timeout(Duration::from_secs(20)).expect("the queue ended");
    assert!(matches!(passed.as_slice(), [(0, Some(frames))] if *frames > 0), "the remote track broke partway: {passed:?}");
    assert_eq!(played.expect("the queue went on"), 2);
    let after = kept.lock().unwrap()[before..].to_vec();
    let _ = std::fs::remove_dir_all(dir);
    after
}

fn first_samples(bytes: &[u8]) -> Vec<i16> {
    bytes.iter().take(8).copied().collect::<Vec<u8>>().chunks(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]])).collect()
}

#[test]
fn a_seek_asked_while_a_read_was_blocked_is_not_served_to_the_next_track() {
    let after = after_a_blocked_read_fails("blocked-seek", |controls| controls.seek.request(Duration::from_millis(50)));
    assert_eq!(first_samples(&after), [0, 0, 1, 1], "the next track starts at its beginning, not 50 ms in");
    assert_eq!(after.len(), 4_410 * 4, "and plays whole");
}

#[test]
fn a_next_asked_while_a_read_was_blocked_does_not_skip_the_next_track() {
    let after = after_a_blocked_read_fails("blocked-next", |controls| controls.skip.skip());
    assert_eq!(after.len(), 4_410 * 4, "the track after the broken one played whole");
    assert_eq!(first_samples(&after), [0, 0, 1, 1]);
}

/// How the track after a one-bit track that breaks ended, in each delivery
/// mode. `aim` runs as the break is said, before the queue moves on: a read
/// of a file cannot be made to block here, so the seek or the Next that came
/// during the failing read is raised at the first moment after it.
fn after_a_dsd_track_breaks(name: &str, aim: &dyn Fn(&Controls)) -> Vec<(DsdMode, Outcome)> {
    let dir = folder(name);
    let tracks = vec![dsf(&dir, "01.dsf", DSD64 / 10, 1), dsf(&dir, "02.dsf", DSD64 / 2, 0)];
    let mut ended = Vec::new();
    for (mode, format, rate) in [
        (DsdMode::Pcm, SampleFormat::S24In32, 88_200),
        (DsdMode::Dop, SampleFormat::S24In32, 176_400),
        (DsdMode::Native, SampleFormat::DsdU32Be, 88_200),
    ] {
        let caps = DeviceCaps { sample_rates: vec![rate], formats: vec![format], channels: vec![2], probed: true };
        let controls = Controls::new();
        controls.dsd_mode.set(mode);
        controls.direct_dsd.set(true);
        let results: Vec<TrackResult> = play_queue_told(
            &SharedQueue::new(tracks.clone()),
            &caps,
            Policy::AllowAnything,
            &controls,
            |opened| Ok(Swallow(opened)),
            &mut |_, _, _, _| {},
            &mut |_, _| {},
            &mut |_, _, _, _| aim(&controls),
        )
        .expect("the queue went on past the broken track");
        assert_eq!(results.len(), 2, "{mode:?}");
        assert!(matches!(results[0].outcome, Outcome::Unreadable { .. }), "{mode:?}: {:?}", results[0].outcome);
        ended.push((mode, results[1].outcome.clone()));
    }
    let _ = std::fs::remove_dir_all(dir);
    ended
}

#[test]
fn a_seek_or_a_next_asked_while_a_dsd_track_broke_is_not_served_to_the_next_track() {
    let undisturbed = after_a_dsd_track_breaks("dsd-undisturbed", &|_| {});
    for (_, outcome) in &undisturbed {
        assert!(matches!(outcome, Outcome::Finished { frames } if *frames > 0), "{outcome:?}");
    }
    let sought = after_a_dsd_track_breaks("dsd-seek", &|controls| controls.seek.request(Duration::from_millis(200)));
    assert_eq!(sought, undisturbed, "the next track plays from its beginning, whole");
    let skipped = after_a_dsd_track_breaks("dsd-next", &|controls| controls.skip.skip());
    assert_eq!(skipped, undisturbed, "the next track is not skipped");
}
