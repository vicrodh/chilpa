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
use chilpa_core::queue_playback::{play_queue_with, Controls, DsdMode, SharedQueue};
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
    dsf_with(name, RATE, samples, 4096, &|_| 0x69)
}

/// A byte for each place of a track, so that a part of it lost, repeated or
/// moved does not compare equal to the track.
fn pattern(place: usize) -> u8 {
    ((place * 7 + 3) % 251) as u8
}

/// A two channel DSF at `rate`, `samples` one-bit samples long, in blocks of
/// `block` bytes a channel, each byte of its audio as `fill` says.
fn dsf_with(name: &str, rate: u32, samples: u64, block: u32, fill: &dyn Fn(usize) -> u8) -> PathBuf {
    let channels = 2u32;
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
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&samples.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_chunk.to_le_bytes());
    out.extend((0..data_len).map(fill));
    let path = std::env::temp_dir().join(format!("chilpa-dsd-queue-{name}.dsf"));
    std::fs::write(&path, out).unwrap();
    path
}

/// Sixteen bit stereo at 44.1 kHz, each frame carrying its own number.
fn wave(name: &str, frames: u32) -> PathBuf {
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
    let path = std::env::temp_dir().join(format!("chilpa-dsd-queue-{name}.wav"));
    std::fs::write(&path, out).unwrap();
    path
}

/// A device that takes what it is given at once.
struct Swallow(StreamFormat);

#[test]
fn enabled_pcm_dsp_leaves_dop_and_native_dsd_untouched() {
    struct Capture(StreamFormat, Arc<Mutex<Vec<u8>>>);
    impl AudioSink for Capture {
        fn format(&self) -> StreamFormat { self.0 }
        fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
            self.1.lock().unwrap().extend_from_slice(bytes); Ok(bytes.len()/self.0.frame_bytes())
        }
        fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
    }
    let track = dsf_of("dsp-passthrough", u64::from(RATE)/20);
    for (mode, format, rate) in [(DsdMode::Dop, SampleFormat::S24In32, 176_400), (DsdMode::Native, SampleFormat::DsdU32Be, 88_200)] {
        let render = |preset| {
            let controls = Controls::new(); controls.dsd_mode.set(mode); controls.direct_dsd.set(true);
            controls.dsp.set(preset).unwrap();
            let caps = DeviceCaps { sample_rates: vec![rate], formats: vec![format], channels: vec![2], probed: true };
            let kept = Arc::new(Mutex::new(Vec::new()));
            play_queue_with(&SharedQueue::new(vec![track.clone()]), &caps, Policy::AllowAnything, &controls,
                |opened| Ok(Capture(opened, kept.clone())), &mut |_, _, delivery| {
                    assert!(!delivery.transforms.contains(&Transform::DspApplied));
                }, &mut |_, _| {}).unwrap();
            assert!(!controls.dsp.touched());
            let bytes = kept.lock().unwrap().clone(); bytes
        };
        let original = render(None);
        let processed = render(Some(chilpa_core::dsp::Preset { preamp_db: -20.0, width: 0.0, ..Default::default() }));
        assert!(!original.is_empty()); assert_eq!(original, processed, "{mode:?}");
    }
    std::fs::remove_file(track).unwrap();
}

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
fn truncated_dsd_is_said_and_passed_over_in_every_delivery_mode() {
    let broken = dsf("truncated", 1);
    let next = dsf("after-truncated", 1);
    let file = std::fs::OpenOptions::new().write(true).open(&broken).unwrap();
    file.set_len(file.metadata().unwrap().len() - 1).unwrap();
    for (mode, format, rate) in [
        (DsdMode::Pcm, SampleFormat::S24In32, 88_200),
        (DsdMode::Dop, SampleFormat::S24In32, 176_400),
        (DsdMode::Native, SampleFormat::DsdU32Be, 88_200),
    ] {
        let caps = DeviceCaps {
            sample_rates: vec![rate], formats: vec![format], channels: vec![2], probed: true,
        };
        let controls = Controls::new();
        controls.dsd_mode.set(mode);
        controls.direct_dsd.set(true);
        let mut started = Vec::new();
        let result = play_queue_with(
            &SharedQueue::new(vec![broken.clone(), next.clone()]),
            &caps,
            Policy::AllowAnything,
            &controls,
            |format| Ok(Swallow(format)),
            &mut |index, _, _| started.push(index),
            &mut |_, _| {},
        );
        let results = result.expect("the queue reports the track outcome");
        assert_eq!(results.len(), 2, "{mode:?}: the queue goes on past the broken track");
        assert!(
            matches!(results[0].outcome, chilpa_audio::render::Outcome::Unreadable { .. }),
            "{mode:?}: missing audio is never taken for a natural end"
        );
        assert!(results[1].outcome.may_advance_queue(), "{mode:?}: the next track plays whole");
        assert_eq!(started, [0, 1], "{mode:?}");
    }
    std::fs::remove_file(broken).unwrap();
    std::fs::remove_file(next).unwrap();
}

/// What a device was given, opening by opening, and whether each was played
/// out or had what it held thrown away.
#[derive(Default)]
struct Openings {
    bytes: Vec<Vec<u8>>,
    drained: Vec<bool>,
    flushed: Vec<bool>,
    /// How many frames each track says went out, track by track.
    frames: Vec<u64>,
    frame_bytes: usize,
}

impl Openings {
    /// Every byte the device got is a frame some track counted.
    fn every_frame_is_counted(&self) -> bool {
        self.bytes.iter().map(Vec::len).sum::<usize>() == self.frames.iter().sum::<u64>() as usize * self.frame_bytes
    }
}

struct Record(StreamFormat, Arc<Mutex<Openings>>, usize);

impl AudioSink for Record {
    fn format(&self) -> StreamFormat {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        self.1.lock().unwrap().bytes[self.2].extend_from_slice(bytes);
        Ok(bytes.len() / self.0.frame_bytes())
    }
    fn drain(&mut self, _: &dyn Fn() -> bool) -> Result<(), SinkError> {
        self.1.lock().unwrap().drained[self.2] = true;
        Ok(())
    }
    fn flush(&mut self) -> Result<(), SinkError> {
        self.1.lock().unwrap().flushed[self.2] = true;
        Ok(())
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// Plays `tracks` in one delivery mode and keeps what each opening got.
fn record(tracks: &[PathBuf], mode: DsdMode, format: SampleFormat, rate: u32, gapless: bool) -> Openings {
    let caps = DeviceCaps { sample_rates: vec![rate], formats: vec![format], channels: vec![2], probed: true };
    let controls = Controls::new();
    controls.dsd_mode.set(mode);
    controls.direct_dsd.set(true);
    controls.gapless.set(gapless);
    let kept = Arc::new(Mutex::new(Openings::default()));
    let results = play_queue_with(
        &SharedQueue::new(tracks.to_vec()),
        &caps,
        Policy::AllowAnything,
        &controls,
        |opened| {
            let mut openings = kept.lock().unwrap();
            openings.bytes.push(Vec::new());
            openings.drained.push(false);
            openings.flushed.push(false);
            openings.frame_bytes = opened.frame_bytes();
            Ok(Record(opened, Arc::clone(&kept), openings.bytes.len() - 1))
        },
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("the queue reports each track");
    let mut openings = std::mem::take(&mut *kept.lock().unwrap());
    openings.frames = results.iter().map(|result| result.frames()).collect();
    openings
}

/// Cuts the last byte off a file: its last block cannot be read whole.
fn cut_short(path: &PathBuf) {
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_len(file.metadata().unwrap().len() - 1).unwrap();
}

/// The DoP marker of each frame: the top byte of each left-justified 24 bit
/// word, the same on both channels.
fn markers(bytes: &[u8]) -> Vec<u8> {
    bytes.chunks_exact(8).map(|frame| {
        assert_eq!(frame[3], frame[7], "both channels of a frame carry one marker");
        frame[3]
    }).collect()
}

/// What DoP carries once its markers are left out.
fn payload(bytes: &[u8]) -> Vec<u8> {
    bytes.chunks_exact(4).flat_map(|word| [word[1], word[2]]).collect()
}

fn alternates(markers: &[u8]) -> bool {
    markers.iter().all(|m| *m == 0x05 || *m == 0xFA) && markers.windows(2).all(|pair| pair[0] != pair[1])
}

#[test]
fn after_a_truncated_track_the_next_one_is_heard_whole_in_every_delivery_mode() {
    let broken = dsf("transition-truncated", 1);
    // Not the broken track's silence: a frame of the next one lost at the
    // break cannot hide behind an equal one of the track before it.
    let next = dsf_with("transition-next", RATE, u64::from(RATE) / 4, 4096, &pattern);
    cut_short(&broken);
    // Every block but the last, which the cut left short, in bytes a channel.
    let whole = (u64::from(RATE) / 8).div_ceil(4096) * 4096 - 4096;
    for (mode, format, rate) in [
        (DsdMode::Pcm, SampleFormat::S24In32, 88_200),
        (DsdMode::Dop, SampleFormat::S24In32, 176_400),
        (DsdMode::Native, SampleFormat::DsdU32Be, 88_200),
    ] {
        let alone = record(std::slice::from_ref(&next), mode, format, rate, true);
        assert_eq!(alone.bytes.len(), 1, "{mode:?}");
        assert!(alone.every_frame_is_counted(), "{mode:?}");
        let alone_frames = alone.frames[0];
        let alone = &alone.bytes[0];
        assert!(!alone.is_empty(), "{mode:?}");

        // Gapless: one opening, the next track right after what the broken
        // one delivered, nothing of it thrown away.
        let kept = record(&[broken.clone(), next.clone()], mode, format, rate, true);
        assert_eq!(kept.bytes.len(), 1, "{mode:?}: the device stays open across the break");
        let bytes = &kept.bytes[0];
        assert!(bytes.len() > alone.len(), "{mode:?}: the broken track delivered its first part");
        assert!(!kept.flushed[0], "{mode:?}: what the broken track delivered is heard, not thrown away");
        // Counted track by track: the broken one's frames, then every frame
        // of the next, and not one byte more or less.
        assert!(kept.every_frame_is_counted(), "{mode:?}: {:?} frames for {} bytes", kept.frames, bytes.len());
        assert_eq!(kept.frames[1], alone_frames, "{mode:?}: the next track delivers what it delivers alone");
        let (head, tail) = bytes.split_at(kept.frames[0] as usize * kept.frame_bytes);
        match mode {
            DsdMode::Dop => {
                assert_eq!(kept.frames[0], whole / 2, "{mode:?}: every whole block of the broken track, sixteen bits a frame");
                let own = payload(head);
                assert!(own.iter().all(|byte| *byte == own[0]), "{mode:?}: the broken track's part is all its own");
                assert_eq!(payload(tail), payload(alone), "{mode:?}: every frame of the next track");
                assert!(alternates(&markers(bytes)), "{mode:?}: the marker carries on across the break");
            }
            DsdMode::Native => {
                assert_eq!(kept.frames[0], whole / 4, "{mode:?}: every whole block of the broken track, thirty-two bits a frame");
                assert!(head.iter().all(|byte| *byte == head[0]), "{mode:?}: the broken track's part is all its own");
                assert!(tail == &alone[..], "{mode:?}: every frame of the next track, unchanged");
            }
            DsdMode::Pcm => assert!(tail == &alone[..], "{mode:?}: every frame of the next track, unchanged"),
        }

        // Without gapless: what the device held of the broken track is played
        // out, and the next track gets an opening of its own.
        let reopened = record(&[broken.clone(), next.clone()], mode, format, rate, false);
        assert_eq!(reopened.bytes.len(), 2, "{mode:?}: the device is opened again");
        assert!(reopened.drained[0], "{mode:?}: the broken track's last part was played out");
        assert!(reopened.every_frame_is_counted(), "{mode:?}");
        assert_eq!(reopened.frames, kept.frames, "{mode:?}: each track delivers the same frames either way");
        assert_eq!(reopened.bytes[0].len(), reopened.frames[0] as usize * reopened.frame_bytes, "{mode:?}: the first opening got the broken track and nothing else");
        assert!(reopened.bytes[1] == *alone, "{mode:?}: the next track from a fresh opening, as when alone");
        if mode == DsdMode::Dop {
            assert_eq!(markers(&reopened.bytes[1]).first(), Some(&0x05), "the marker starts over with the opening");
        }
    }
    std::fs::remove_file(broken).unwrap();
    std::fs::remove_file(next).unwrap();
}

#[test]
fn after_an_odd_count_of_dop_frames_the_marker_carries_on_into_the_next_track() {
    // Blocks of 4098 bytes are 2049 DoP frames each. Four of them, the last
    // cut: three go out, an odd count, so the broken track ends on 0x05 and
    // the next has to start on 0xFA. A packer that started over at the break
    // would put 0x05 twice in a row, which an even count never shows.
    let broken = dsf_with("odd-truncated", RATE, 4098 * 8 * 4, 4098, &|_| 0x69);
    let next = dsf_with("odd-next", RATE, u64::from(RATE) / 4, 4096, &pattern);
    cut_short(&broken);
    let alone = record(std::slice::from_ref(&next), DsdMode::Dop, SampleFormat::S24In32, 176_400, true);
    let kept = record(&[broken.clone(), next.clone()], DsdMode::Dop, SampleFormat::S24In32, 176_400, true);
    assert_eq!(kept.bytes.len(), 1, "the device stays open across the break");
    assert_eq!(kept.frames, [3 * 2049, alone.frames[0]], "an odd count of frames, then the next track whole");
    assert!(kept.every_frame_is_counted());
    let bytes = &kept.bytes[0];
    let marks = markers(bytes);
    assert_eq!((marks[3 * 2049 - 1], marks[3 * 2049]), (0x05, 0xFA), "the next track takes the marker up where the broken one left it");
    assert!(alternates(&marks), "and it alternates all the way");
    assert_eq!(payload(&bytes[3 * 2049 * 8..]), payload(&alone.bytes[0]), "every frame of the next track");
    std::fs::remove_file(broken).unwrap();
    std::fs::remove_file(next).unwrap();
}

#[test]
fn after_a_truncated_track_a_next_one_at_another_rate_reopens_the_device_with_gapless_on() {
    let broken = dsf("rate-change-truncated", 1);
    // DSD128: another carrier rate and another native rate. Converted to
    // PCM it comes out at the rate DSD64 does, so there a file at 44.1 kHz
    // is what changes the rate.
    let faster = dsf_with("rate-change-next", RATE * 2, u64::from(RATE) / 2, 4096, &pattern);
    let slower = wave("rate-change-next", 11_025);
    cut_short(&broken);
    for (mode, format, rates, next) in [
        (DsdMode::Pcm, SampleFormat::S24In32, [88_200, 44_100], &slower),
        (DsdMode::Dop, SampleFormat::S24In32, [176_400, 352_800], &faster),
        (DsdMode::Native, SampleFormat::DsdU32Be, [88_200, 176_400], &faster),
    ] {
        let play = |tracks: &[PathBuf]| {
            let caps = DeviceCaps { sample_rates: rates.to_vec(), formats: vec![format], channels: vec![2], probed: true };
            let controls = Controls::new();
            controls.dsd_mode.set(mode);
            controls.direct_dsd.set(true);
            let kept = Arc::new(Mutex::new(Openings::default()));
            let mut opened_at = Vec::new();
            let results = play_queue_with(
                &SharedQueue::new(tracks.to_vec()),
                &caps,
                Policy::AllowAnything,
                &controls,
                |opened| {
                    let mut openings = kept.lock().unwrap();
                    openings.bytes.push(Vec::new());
                    openings.drained.push(false);
                    openings.flushed.push(false);
                    opened_at.push(opened.sample_rate_hz);
                    Ok(Record(opened, Arc::clone(&kept), openings.bytes.len() - 1))
                },
                &mut |_, _, _| {},
                &mut |_, _| {},
            )
            .expect("the queue reports each track");
            let mut openings = std::mem::take(&mut *kept.lock().unwrap());
            openings.frames = results.iter().map(|result| result.frames()).collect();
            (openings, opened_at)
        };
        let (alone, alone_at) = play(std::slice::from_ref(next));
        let (both, both_at) = play(&[broken.clone(), next.clone()]);
        assert_eq!(both.bytes.len(), 2, "{mode:?}: gapless is on and the rate still changes, so the device is opened again");
        assert_ne!(both_at[0], both_at[1], "{mode:?}: at another rate");
        assert_eq!(both_at[1], alone_at[0], "{mode:?}");
        assert!(both.drained[0] && !both.flushed[0], "{mode:?}: what the broken track delivered is played out, not thrown away");
        assert!(!both.bytes[0].is_empty(), "{mode:?}: the broken track delivered its first part");
        assert_eq!(both.bytes[0].len(), both.frames[0] as usize * format.bytes() * 2, "{mode:?}: the first opening got the broken track and nothing else");
        assert_eq!(both.frames[1], alone.frames[0], "{mode:?}: the next track delivers what it delivers alone");
        assert!(both.bytes[1] == alone.bytes[0], "{mode:?}: every frame of the next track, from a fresh opening");
        if mode == DsdMode::Dop {
            assert_eq!(markers(&both.bytes[1]).first(), Some(&0x05), "the marker starts over with the opening");
            assert!(alternates(&markers(&both.bytes[1])));
        }
    }
    for file in [broken, faster, slower] {
        std::fs::remove_file(file).unwrap();
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

    player.set_dsd_mode(chilpa_core::queue_playback::DsdMode::Dop);
    player.play_all(&tracks, "alsa:hw:0,0");
    let mut started = Vec::new();
    let limit = Instant::now() + Duration::from_secs(10);
    let mut over = false;
    while !over && Instant::now() < limit {
        for event in player.poll() {
            match event {
                PlayerEvent::Started { index, dsd_multiple, dsd_passthrough, .. } => {
                    started.push((index, dsd_multiple, dsd_passthrough))
                }
                PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. } => over = true,
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        started,
        vec![(0, Some(64), true), (1, Some(64), true), (2, Some(64), true)],
        "every track of the album started, in order, as DSD64"
    );
    assert_eq!(*opens.lock().unwrap(), 1, "three tracks at one carrier rate share one opening");
    drop(player);
    for t in tracks {
        let _ = std::fs::remove_file(t);
    }
}

#[test]
fn native_dsd_opens_the_kernel_u32_format_without_a_pcm_carrier() {
    let track = dsf("native", 1);
    let opened = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&opened);
    let player = Player::spawn(
        Box::new(|_| Ok(DeviceCaps { sample_rates: vec![88_200], formats: vec![SampleFormat::DsdU32Be], channels: vec![2], probed: true })),
        Box::new(move |_, format, _| {
            seen.lock().unwrap().push(format);
            Ok(Box::new(Swallow(format)) as Box<dyn AudioSink + Send>)
        }),
    );
    player.set_dsd_mode(chilpa_core::queue_playback::DsdMode::Native);
    // Moving from shared PCM must not make the one-bit guard refuse DSD.
    player.set_volume(0.125);
    player.play(&track, "alsa:hw:0,0");

    let limit = Instant::now() + Duration::from_secs(10);
    let mut result = None;
    while result.is_none() && Instant::now() < limit {
        for event in player.poll() {
            match event {
                PlayerEvent::Finished { .. } => result = Some(Ok(())),
                PlayerEvent::Failed { message } => result = Some(Err(message)),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(result, Some(Ok(())));
    assert_eq!(*opened.lock().unwrap(), vec![StreamFormat::new(SampleFormat::DsdU32Be, 88_200, 2)]);

    drop(player);
    let _ = std::fs::remove_file(track);
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
    controls.dsd_mode.set(chilpa_core::queue_playback::DsdMode::Dop);
    controls.direct_dsd.set(true);
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
fn dsd_asked_untouched_is_converted_where_it_cannot_go_untouched() {
    // Through a sound server, and on a card that takes neither DoP nor the
    // native format: the record plays as PCM, whichever was asked for.
    let caps = DeviceCaps {
        sample_rates: vec![44_100, 88_200],
        formats: vec![SampleFormat::S24In32],
        channels: vec![2],
        probed: true,
    };
    for (mode, direct) in [(DsdMode::Native, false), (DsdMode::Dop, false), (DsdMode::Native, true)] {
        let track = dsf_of("asked-untouched", u64::from(RATE) / 10);
        let controls = Controls::new();
        controls.dsd_mode.set(mode);
        controls.direct_dsd.set(direct);
        let mut delivery = None;
        play_queue_with(
            &SharedQueue::new(vec![track.clone()]),
            &caps,
            Policy::AllowAnything,
            &controls,
            |format| Ok(Swallow(format)),
            &mut |_, _, opened| delivery = Some(opened.clone()),
            &mut |_, _| {},
        )
        .unwrap_or_else(|e| panic!("{mode:?}, direct {direct}: the queue runs, not {e:?}"));
        let delivery = delivery.unwrap_or_else(|| panic!("{mode:?}, direct {direct}: the track started"));
        assert!(delivery.transforms.contains(&Transform::DsdToPcm), "{mode:?}, direct {direct}");
        assert!(!delivery.nothing_was_converted(), "and nothing calls it bit-perfect");
        let _ = std::fs::remove_file(track);
    }
}

#[test]
fn a_shared_player_announces_pcm_instead_of_dop() {
    let track = dsf_of("shared-announcement", u64::from(RATE) / 10);
    let player = Player::spawn(
        Box::new(|_| {
            Ok(DeviceCaps {
                sample_rates: vec![88_200],
                formats: vec![SampleFormat::S24In32],
                channels: vec![2],
                probed: true,
            })
        }),
        Box::new(|_, format, _| Ok(Box::new(Swallow(format)) as Box<dyn AudioSink + Send>)),
    );
    player.play(&track, "default");
    let limit = Instant::now() + Duration::from_secs(5);
    let mut started = None;
    while started.is_none() && Instant::now() < limit {
        started = player
            .poll()
            .into_iter()
            .find(|event| matches!(event, PlayerEvent::Started { .. } | PlayerEvent::Failed { .. }));
        std::thread::sleep(Duration::from_millis(5));
    }
    match started {
        Some(PlayerEvent::Started { opened, dsd_multiple, dsd_passthrough, bit_perfect, .. }) => {
            assert_eq!(opened.sample_rate_hz, 88_200);
            assert_eq!(dsd_multiple, Some(64));
            assert!(!dsd_passthrough, "the shared route carries converted PCM");
            assert!(!bit_perfect, "DSD-to-PCM is a conversion");
        }
        other => panic!("shared DSD did not start as PCM: {other:?}"),
    }
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
    controls.dsd_mode.set(chilpa_core::queue_playback::DsdMode::Dop);
    controls.direct_dsd.set(true);
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

#[test]
fn pcm_volume_never_marks_or_changes_native_dsd_or_dop_payloads() {
    struct Capture(StreamFormat, Arc<Mutex<Vec<u8>>>);
    impl AudioSink for Capture {
        fn format(&self) -> StreamFormat { self.0 }
        fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
            self.1.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len() / self.0.frame_bytes())
        }
        fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
    }
    let track = dsf_of("saved-pcm-volume", u64::from(RATE) / 20);
    for (mode, format, rate) in [(DsdMode::Native, SampleFormat::DsdU32Be, 88_200), (DsdMode::Dop, SampleFormat::S24In32, 176_400)] {
        let mut reference = None;
        for volume in [1.0, 0.125, 0.0] {
            let controls = Controls::new();
            controls.dsd_mode.set(mode);
            controls.direct_dsd.set(true);
            controls.volume.set(volume);
            let captured = Arc::new(Mutex::new(Vec::new()));
            let caps = DeviceCaps { sample_rates: vec![rate], formats: vec![format], channels: vec![2], probed: true };
            play_queue_with(&SharedQueue::new(vec![track.clone()]), &caps, Policy::AllowAnything, &controls,
                |opened| Ok(Capture(opened, captured.clone())), &mut |_, _, delivery| {
                    assert!(!delivery.transforms.contains(&Transform::VolumeApplied), "{mode:?}: PCM volume is bypassed");
                }, &mut |_, _| {}).unwrap();
            let bytes = captured.lock().unwrap().clone();
            assert!(!bytes.is_empty());
            if let Some(original) = &reference { assert_eq!(&bytes, original, "{mode:?}: saved volume {volume}"); }
            else { reference = Some(bytes); }
        }
    }
    std::fs::remove_file(track).unwrap();
}

#[test]
fn entering_pcm_conversion_resumes_a_dsd_track_at_the_requested_position() {
    let track = dsf("pcm-output-switch", 2);
    let caps = DeviceCaps { sample_rates: vec![88_200], formats: vec![SampleFormat::S32], channels: vec![2], probed: true };
    // Every saved mode falls back to PCM on a shared output.
    for mode in [DsdMode::Pcm, DsdMode::Native, DsdMode::Dop] {
        let controls = Controls::new();
        controls.dsd_mode.set(mode);
        controls.seek.request(Duration::from_secs(1));
        let mut positions = Vec::new();
        let results = play_queue_with(&SharedQueue::new(vec![track.clone()]), &caps, Policy::AllowAnything, &controls,
            |opened| Ok(Swallow(opened)), &mut |_, _, delivery| assert!(delivery.transforms.contains(&Transform::DsdToPcm)),
            &mut |_, progress| positions.push(progress.position)).unwrap();
        assert_eq!(results[0].frames(), 88_200, "{mode:?}: only the remaining second is delivered");
        assert!(positions.first().is_some_and(|at| *at >= 88_200));
        assert_eq!(positions.last(), Some(&176_400));
    }
    std::fs::remove_file(track).unwrap();
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
    player.set_dsd_mode(chilpa_core::queue_playback::DsdMode::Dop);
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

#[test]
fn dop_goes_only_where_the_wire_takes_the_carrier_untouched() {
    // A stack that converts on its own -- a Mac's output unit -- takes any
    // rate, and DoP resampled is loud noise. What decides is what reaches
    // its wire untouched: without the carrier there, the record plays as
    // PCM; with it, as DoP.
    let takes_anything = DeviceCaps {
        sample_rates: vec![44_100, 88_200, 96_000, 176_400, 192_000],
        formats: vec![SampleFormat::S16, SampleFormat::S24In32, SampleFormat::F32],
        channels: vec![2],
        probed: true,
    };
    let device = |rates: Vec<u32>| DeviceCaps {
        sample_rates: rates,
        formats: vec![SampleFormat::S24In32],
        channels: vec![2],
        probed: true,
    };
    for (rates, dop) in [(vec![44_100, 48_000, 88_200, 96_000, 192_000], false), (vec![44_100, 88_200, 176_400], true)] {
        let track = dsf_of(&format!("untouched-{dop}"), u64::from(RATE) / 10);
        let controls = Controls::new();
        controls.dsd_mode.set(DsdMode::Dop);
        controls.direct_dsd.set(true);
        *controls.direct_caps.lock().unwrap() = Some(device(rates.clone()));
        let mut delivery = None;
        play_queue_with(
            &SharedQueue::new(vec![track.clone()]),
            &takes_anything,
            Policy::AllowAnything,
            &controls,
            |format| Ok(Swallow(format)),
            &mut |_, _, opened| delivery = Some(opened.clone()),
            &mut |_, _| {},
        )
        .unwrap_or_else(|e| panic!("{rates:?}: the queue runs, not {e:?}"));
        let delivery = delivery.unwrap_or_else(|| panic!("{rates:?}: the track started"));
        if dop {
            assert_eq!(delivery.opened, StreamFormat::new(SampleFormat::S24In32, 176_400, 2), "{rates:?}: DoP at the carrier");
            assert!(delivery.nothing_was_converted(), "{rates:?}");
        } else {
            assert!(delivery.transforms.contains(&Transform::DsdToPcm), "{rates:?}: converted, never DoP resampled");
        }
        let _ = std::fs::remove_file(track);
    }
}
