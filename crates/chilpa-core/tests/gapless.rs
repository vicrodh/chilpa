//! Two tracks in a row come out joined, with no gap and no repeated samples.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::queue_playback::{play_queue_with, Controls, SharedQueue};
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Clone, Default)]
struct Capture(Rc<RefCell<Vec<i16>>>);

impl AudioSink for Capture {
    fn format(&self) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let mut m = self.0.borrow_mut();
        for pair in frames.chunks_exact(2) {
            m.push(i16::from_le_bytes([pair[0], pair[1]]));
        }
        Ok(frames.len() / 4)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// A WAV whose left channel counts from `from`.
fn ramp(name: &str, from: i32, frames: u32) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-gapless-{name}.wav"));
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
        let v = ((from + i as i32) % 30_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&(-v).to_le_bytes()).unwrap();
    }
    p
}

fn caps(rates: &[u32]) -> DeviceCaps {
    DeviceCaps {
        sample_rates: rates.to_vec(),
        formats: vec![SampleFormat::S16, SampleFormat::S24In32],
        channels: vec![2],
        probed: true,
    }
}

fn play(tracks: &[PathBuf], caps: &DeviceCaps) -> (Vec<i16>, usize) {
    let cap = Capture::default();
    let mut opens = 0;
    play_queue_with(
        &SharedQueue::new(tracks.to_vec()),
        caps,
        Policy::BitPerfect,
        &Controls::new(),
        |_| {
            opens += 1;
            Ok(cap.clone())
        },
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("the queue runs");
    let samples = cap.0.borrow().clone();
    (samples, opens)
}

#[test]
fn two_tracks_in_the_same_format_come_out_back_to_back() {
    // The second continues the first one's count: with a gap, a repeat or a
    // trim, the sequence breaks at the join.
    let a = ramp("a", 0, 5_000);
    let b = ramp("b", 5_000, 5_000);
    let (samples, opens) = play(&[a.clone(), b.clone()], &caps(&[44_100]));

    assert_eq!(opens, 1, "the device was opened once");
    assert_eq!(samples.len(), 20_000, "all 10,000 stereo frames came out");

    let left: Vec<i16> = samples.iter().step_by(2).copied().collect();
    let expected: Vec<i16> = (0..10_000).map(|i| (i % 30_000) as i16).collect();
    assert_eq!(left, expected, "the join between tracks is not continuous");

    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[test]
fn if_the_format_changes_the_device_reopens_and_nothing_is_lost() {
    let a = ramp("cd", 0, 4_000);
    // The second track is at another rate: chaining with no gap is
    // impossible, but not one sample may be lost.
    let b = {
        let mut p = std::env::temp_dir();
        p.push("chilpa-gapless-hires.wav");
        let frames = 3_000u32;
        let data = frames * 4;
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&96_000u32.to_le_bytes()).unwrap();
        f.write_all(&384_000u32.to_le_bytes()).unwrap();
        f.write_all(&4u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data.to_le_bytes()).unwrap();
        for i in 0..frames {
            let v = (i % 30_000) as i16;
            f.write_all(&v.to_le_bytes()).unwrap();
            f.write_all(&(-v).to_le_bytes()).unwrap();
        }
        p
    };

    let (samples, opens) = play(&[a.clone(), b.clone()], &caps(&[44_100, 96_000]));
    assert_eq!(opens, 2, "changing the rate forces a reopen");
    assert_eq!(samples.len(), (4_000 + 3_000) * 2, "not one sample was lost");

    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

/// Encodes `wav` to MP3 with LAME, which writes how many frames at each end
/// are not music into the file's first frame. `None` without `lame`.
fn mp3_of(wav: &PathBuf, name: &str) -> Option<PathBuf> {
    let mut out = std::env::temp_dir();
    out.push(format!("chilpa-gapless-{name}.mp3"));
    let encoded = std::process::Command::new("lame")
        .args(["--quiet", "-b", "192"])
        .arg(wav)
        .arg(&out)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    encoded.then_some(out)
}

/// Counts frames in whatever format the device was opened at.
struct Counting {
    format: StreamFormat,
    frames: Rc<RefCell<u64>>,
}

impl AudioSink for Counting {
    fn format(&self) -> StreamFormat {
        self.format
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let n = frames.len() / self.format.frame_bytes();
        *self.frames.borrow_mut() += n as u64;
        Ok(n)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

#[test]
fn an_mp3_comes_out_without_its_encoder_delay_or_padding() {
    // Left in, the frames the encoder added click at every join of a
    // gapless album. The integer path trimmed them; an MP3 decodes to
    // floats, and the float path went around the trim.
    let wav = ramp("lame", 0, 44_100);
    let Some(mp3) = mp3_of(&wav, "lame") else {
        eprintln!("lame is not installed; skipped");
        return;
    };
    let frames = Rc::new(RefCell::new(0u64));
    let counted = Rc::clone(&frames);
    play_queue_with(
        &SharedQueue::new(vec![mp3.clone(), mp3.clone()]),
        &caps(&[44_100]),
        Policy::AllowAnything,
        &Controls::new(),
        move |format| Ok(Counting { format, frames: Rc::clone(&counted) }),
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
    .expect("the queue runs");
    assert_eq!(*frames.borrow(), 2 * 44_100, "two tracks of 44,100 frames, joined, and nothing else");
    let _ = std::fs::remove_file(wav);
    let _ = std::fs::remove_file(mp3);
}

#[test]
fn a_mono_track_in_the_queue_comes_out_whole_in_both_channels() {
    // The queue counts frames to trim and to seek; after a mono track opens
    // in stereo, counting with the device's two channels would halve it.
    let mut path = std::env::temp_dir();
    path.push("chilpa-gapless-mono.wav");
    let frames = 5_000u32;
    let data = frames * 2;
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&44_100u32.to_le_bytes()).unwrap();
    f.write_all(&88_200u32.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    for i in 0..frames {
        f.write_all(&((i % 30_000) as i16).to_le_bytes()).unwrap();
    }
    drop(f);

    let (samples, _) = play(&[path.clone()], &caps(&[44_100]));
    let expected: Vec<i16> = (0..frames).flat_map(|i| [(i % 30_000) as i16; 2]).collect();
    assert_eq!(samples.len(), expected.len(), "every frame, once, in two channels");
    assert_eq!(samples, expected);
    let _ = std::fs::remove_file(path);
}
