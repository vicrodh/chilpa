//! DSD tracks in a queue, one after another.
//!
//! The player looked at the first track of a queue to decide how to play it,
//! and a DSF went down a path that played that one file and returned. The
//! queue never moved on: a DSD album stopped after its first track.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Transform};
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_audio::negotiate::Policy;
use chilpa_core::player::{Player, PlayerEvent};
use chilpa_core::queue_playback::{play_queue_with, Controls, SharedQueue};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RATE: u32 = 2_822_400;

/// A two channel DSF of DSD64 silence, `seconds` long.
fn dsf(name: &str, seconds: u32) -> PathBuf {
    dsf_of(name, u64::from(RATE) * u64::from(seconds))
}

/// A two channel DSF of DSD64 silence, `samples` one-bit samples long.
fn dsf_of(name: &str, samples: u64) -> PathBuf {
    let channels = 2u32;
    let block = 4096u32;
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
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&samples.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_chunk.to_le_bytes());
    out.resize(out.len() + data_len, 0x69);
    let path = std::env::temp_dir().join(format!("chilpa-dsd-queue-{name}.dsf"));
    std::fs::write(&path, out).unwrap();
    path
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

#[test]
fn a_dsd_album_plays_past_its_first_track() {
    let tracks = [dsf("one", 1), dsf("two", 1), dsf("three", 1)];
    let opens = Arc::new(Mutex::new(0usize));
    let counted = Arc::clone(&opens);
    let player = Player::spawn(
        Box::new(|_| {
            Ok(DeviceCaps {
                sample_rates: vec![44_100, 176_400],
                formats: vec![SampleFormat::S24In32, SampleFormat::S32],
                channels: vec![2],
                probed: true,
            })
        }),
        Box::new(move |_, format, _| {
            *counted.lock().unwrap() += 1;
            Ok(Box::new(Swallow(format)) as Box<dyn AudioSink + Send>)
        }),
    );

    player.play_all(&tracks, "alsa:hw:0,0");
    let mut started = Vec::new();
    let limit = Instant::now() + Duration::from_secs(10);
    let mut over = false;
    while !over && Instant::now() < limit {
        for event in player.poll() {
            match event {
                PlayerEvent::Started { index, dsd_multiple, .. } => started.push((index, dsd_multiple)),
                PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. } => over = true,
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        started,
        vec![(0, Some(64)), (1, Some(64)), (2, Some(64))],
        "every track of the album started, in order, as DSD64"
    );
    assert_eq!(*opens.lock().unwrap(), 1, "three tracks at one carrier rate share one opening");
    drop(player);
    for t in tracks {
        let _ = std::fs::remove_file(t);
    }
}

/// Keeps what reaches it, as DoP words.
#[derive(Default)]
struct Keep(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

impl AudioSink for Keep {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S24In32, 176_400, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        self.0.borrow_mut().extend_from_slice(frames);
        Ok(frames.len() / 8)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

#[test]
fn the_dop_marker_alternates_across_the_join_between_tracks() {
    // Each track an odd number of DoP frames long -- 16,016 samples are
    // 1,001 frames -- so a marker that started over with every track would
    // repeat at the join: 0x05 at the end of the first and 0x05 again at the
    // start of the second, which a DAC can take for the end of DSD.
    let tracks = vec![dsf_of("odd-one", 16_016), dsf_of("odd-two", 16_016)];
    let kept = Keep::default();
    let bytes = std::rc::Rc::clone(&kept.0);
    let caps = DeviceCaps {
        sample_rates: vec![176_400],
        formats: vec![SampleFormat::S24In32],
        channels: vec![2],
        probed: true,
    };
    let mut kept = Some(kept);
    let controls = Controls::new();
    controls.dsd_passthrough.set(true);
    play_queue_with(
        &SharedQueue::new(tracks.clone()),
        &caps,
        Policy::BitPerfect,
        &controls,
        |_| Ok(kept.take().expect("opened once")),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("the queue runs");

    let markers: Vec<u8> = bytes.borrow().chunks_exact(8).map(|frame| frame[3]).collect();
    assert_eq!(markers.len(), 2 * 1_001, "both tracks, every frame");
    for (i, pair) in markers.windows(2).enumerate() {
        assert_ne!(pair[0], pair[1], "frame {} repeats the marker {:#04x}", i + 1, pair[1]);
    }
    for t in tracks {
        let _ = std::fs::remove_file(t);
    }
}

#[test]
fn a_shared_route_converts_dsd_to_pcm() {
    let track = dsf_of("converted", u64::from(RATE) / 10);
    let caps = DeviceCaps {
        sample_rates: vec![88_200, 176_400],
        formats: vec![SampleFormat::S24In32],
        channels: vec![2],
        probed: true,
    };
    let mut delivery = None;
    let results = play_queue_with(
        &SharedQueue::new(vec![track.clone()]),
        &caps,
        Policy::AllowAnything,
        &Controls::new(),
        |format| Ok(Swallow(format)),
        &mut |_, _, opened| delivery = Some(opened.clone()),
        &mut |_, _| {},
    )
    .expect("the queue runs");

    let delivery = delivery.expect("the converted track started");
    assert_eq!(delivery.opened.sample_rate_hz, 88_200);
    assert!(delivery.transforms.contains(&Transform::DsdToPcm));
    assert!(!delivery.nothing_was_converted());
    assert_eq!(results[0].frames(), 8_820, "a tenth of a second of PCM");
    let _ = std::fs::remove_file(track);
}

#[test]
fn a_seek_in_a_dsd_track_moves_the_music_and_the_position() {
    // DoP used to drop every seek: the track went on from where it was.
    let track = dsf("seek", 2);
    let seek = chilpa_audio::seek::SeekSignal::new();
    seek.request(Duration::from_secs(1));
    let caps = DeviceCaps {
        sample_rates: vec![176_400],
        formats: vec![SampleFormat::S24In32],
        channels: vec![2],
        probed: true,
    };
    let mut positions = Vec::new();
    let controls = Controls { seek, ..Controls::new() };
    controls.dsd_passthrough.set(true);
    let results = play_queue_with(
        &SharedQueue::new(vec![track.clone()]),
        &caps,
        Policy::BitPerfect,
        &controls,
        |format| Ok(Swallow(format)),
        &mut |_, _, _| {},
        &mut |_, progress| positions.push(progress.position),
    )
    .expect("the queue runs");

    // A DoP frame carries sixteen one-bit samples of each channel: a second
    // of DSD64 is 176,400 of them, and only the second second went out.
    assert_eq!(results[0].frames(), 176_400, "exactly the second half, block alignment included");
    assert!(positions.first().is_some_and(|p| *p >= 176_400), "it started at {:?}", positions.first());
    assert_eq!(positions.last(), Some(&352_800), "and ended at the end of the track");
    let _ = std::fs::remove_file(track);
}

/// A two channel DSDIFF of DSD64 silence, `seconds` long.
fn dff(name: &str, seconds: u32) -> PathBuf {
    fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        out.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            out.push(0);
        }
        out
    }
    let mut prop = b"SND ".to_vec();
    prop.extend(chunk(b"FS  ", &RATE.to_be_bytes()));
    prop.extend(chunk(b"CHNL", b"\x00\x02SLFTSRGT"));
    prop.extend(chunk(b"CMPR", b"DSD \x0enot compressed"));
    let mut form = b"DSD ".to_vec();
    form.extend(chunk(b"PROP", &prop));
    form.extend(chunk(b"DSD ", &vec![0x69; (RATE / 8 * seconds * 2) as usize]));
    let path = std::env::temp_dir().join(format!("chilpa-dsd-queue-{name}.dff"));
    std::fs::write(&path, chunk(b"FRM8", &form)).unwrap();
    path
}

#[test]
fn a_dsdiff_file_plays_in_a_queue_like_a_dsf() {
    let tracks = [dff("dsdiff", 1), dsf("after-dsdiff", 1)];
    let player = Player::spawn(
        Box::new(|_| {
            Ok(DeviceCaps {
                sample_rates: vec![176_400],
                formats: vec![SampleFormat::S24In32],
                channels: vec![2],
                probed: true,
            })
        }),
        Box::new(|_, format, _| Ok(Box::new(Swallow(format)) as Box<dyn AudioSink + Send>)),
    );
    player.play_all(&tracks, "alsa:hw:0,0");
    let mut started = Vec::new();
    let limit = Instant::now() + Duration::from_secs(10);
    let mut over = None;
    while over.is_none() && Instant::now() < limit {
        for event in player.poll() {
            match event {
                PlayerEvent::Started { index, dsd_multiple, total_frames, .. } => {
                    started.push((index, dsd_multiple, total_frames))
                }
                PlayerEvent::Finished { .. } => over = Some(Ok(())),
                PlayerEvent::Failed { message } => over = Some(Err(message)),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(over, Some(Ok(())), "the queue played through");
    assert_eq!(
        started,
        vec![(0, Some(64), Some(176_400)), (1, Some(64), Some(176_400))],
        "the DSDIFF started as DSD64 with its length, and the DSF after it"
    );
    drop(player);
    for t in tracks {
        let _ = std::fs::remove_file(t);
    }
}
