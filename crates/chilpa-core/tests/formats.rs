//! The same music in several containers and codecs.
//!
//! Two things are verified here:
//!
//! 1. **The lossless ones come out identical** to the original, whatever the
//!    container. It is the bit-perfect invariant applied to each format.
//! 2. **Decoding happens in blocks**, never the whole track in memory. In
//!    QBZ, AAC and ALAC in MP4 were decoded whole into a `Vec<f32>` (about
//!    2.7 GB for an hour at 24/96). Here the largest block the decoder
//!    delivers is measured: if anyone reads the whole file again, this test
//!    catches it.
//!
//! The files are generated with ffmpeg. If it is not installed, the tests
//! are skipped instead of failing.

use chilpa_decode::{Block, Decoded};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const RATE: u32 = 44_100;
const FRAMES: u32 = 44_100 * 3;
/// A block longer than a second is already suspect of reading too much.
const MAX_FRAMES_PER_BLOCK: usize = RATE as usize;

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn dir() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("chilpa-formats");
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Reference WAV: the left channel carries a ramp and the right its
/// negation, so any shift or conversion shows up.
fn write_wav(path: &Path, bits: u16) {
    let bytes = u32::from(bits) / 8;
    let data_len = FRAMES * 2 * bytes;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&RATE.to_le_bytes()).unwrap();
    f.write_all(&(RATE * 2 * bytes).to_le_bytes()).unwrap();
    f.write_all(&(2 * bits / 8).to_le_bytes()).unwrap();
    f.write_all(&bits.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for i in 0..FRAMES {
        let v = sample(i, bits);
        match bits {
            16 => {
                f.write_all(&(v as i16).to_le_bytes()).unwrap();
                f.write_all(&(-(v as i16)).to_le_bytes()).unwrap();
            }
            _ => {
                f.write_all(&v.to_le_bytes()[..3]).unwrap();
                f.write_all(&(-v).to_le_bytes()[..3]).unwrap();
            }
        }
    }
}

fn sample(i: u32, bits: u16) -> i32 {
    let period = if bits == 16 { 30_000 } else { 8_000_000 };
    (i as i32 % period) - period / 2
}

fn convert(origin: &Path, target: &Path, args: &[&str]) -> bool {
    let output = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(origin)
        .args(args)
        .arg(target)
        .output()
        .expect("ffmpeg runs");
    if !output.status.success() {
        eprintln!("ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    output.status.success()
}

/// Decodes everything and returns the left channel's samples plus the
/// largest block the decoder delivered.
fn decode(path: &Path) -> (Vec<i32>, usize, u8) {
    let mut decoded = Decoded::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let channels = decoded.stream_format().channels as usize;
    let mut left = Vec::new();
    let mut largest = 0usize;
    let mut bits = 0u8;
    while let Some(block) = decoded.next_block().unwrap() {
        match block {
            Block::Int { data, bits: b } => {
                bits = b;
                largest = largest.max(data.len() / channels);
                left.extend(data.iter().step_by(channels));
            }
            Block::Float(data) => {
                largest = largest.max(data.len() / channels);
                left.extend(data.iter().step_by(channels).map(|v| (v * 32768.0) as i32));
            }
        }
    }
    (left, largest, bits)
}

#[test]
fn lossless_formats_come_out_identical_and_in_blocks() {
    if !have_ffmpeg() {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let d = dir();
    let wav16 = d.join("ref16.wav");
    let wav24 = d.join("ref24.wav");
    write_wav(&wav16, 16);
    write_wav(&wav24, 24);

    let expected16: Vec<i32> = (0..FRAMES).map(|i| sample(i, 16)).collect();
    let expected24: Vec<i32> = (0..FRAMES).map(|i| sample(i, 24)).collect();

    let cases: &[(&str, &Path, &[&str], &Vec<i32>, u8)] = &[
        ("flac16.flac", &wav16, &["-c:a", "flac"], &expected16, 16),
        ("flac24.flac", &wav24, &["-c:a", "flac"], &expected24, 24),
        ("alac16.m4a", &wav16, &["-c:a", "alac"], &expected16, 16),
        ("alac24.m4a", &wav24, &["-c:a", "alac"], &expected24, 24),
        ("pcm24.wav", &wav24, &["-c:a", "pcm_s24le"], &expected24, 24),
        ("pcm16.aiff", &wav16, &["-c:a", "pcm_s16be"], &expected16, 16),
    ];

    for (name, origin, args, expected, expected_bits) in cases {
        let target = d.join(name);
        if !convert(origin, &target, args) {
            eprintln!("{name}: ffmpeg could not, skipped");
            continue;
        }
        let (came_out, largest, bits) = decode(&target);
        assert_eq!(bits, *expected_bits, "{name}: the file's bits rule");
        assert_eq!(came_out.len(), expected.len(), "{name}: frames were missing or left over");
        assert_eq!(&came_out, *expected, "{name}: the signal changed");
        assert!(
            largest <= MAX_FRAMES_PER_BLOCK,
            "{name}: a block of {largest} frames; too much is being decoded"
        );
        let _ = std::fs::remove_file(target);
    }
    let _ = std::fs::remove_file(wav16);
    let _ = std::fs::remove_file(wav24);
}

#[test]
fn lossy_formats_decode_in_blocks() {
    if !have_ffmpeg() {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let d = dir();
    let wav = d.join("ref-lossy.wav");
    write_wav(&wav, 16);

    // Opus does not make it: the decoder we use does not carry it yet. It
    // stays a known gap, and the test below pins it.
    for (name, args) in [
        ("aac.m4a", ["-c:a", "aac"]),
        ("mp3.mp3", ["-c:a", "mp3"]),
        ("vorbis.ogg", ["-c:a", "libvorbis"]),
    ] {
        let target = d.join(name);
        if !convert(&wav, &target, &args) {
            eprintln!("{name}: ffmpeg could not, skipped");
            continue;
        }
        let (came_out, largest, _) = decode(&target);
        assert!(!came_out.is_empty(), "{name}: no audio came out");
        // With lossy the duration moves a little because of encoder padding;
        // close enough is enough.
        let difference = (came_out.len() as i64 - FRAMES as i64).abs();
        assert!(
            difference < RATE as i64 / 2,
            "{name}: {} frames came out against {FRAMES}",
            came_out.len()
        );
        assert!(
            largest <= MAX_FRAMES_PER_BLOCK,
            "{name}: a block of {largest} frames; too much is being decoded"
        );
        let _ = std::fs::remove_file(target);
    }
    let _ = std::fs::remove_file(wav);
}

#[test]
fn a_lossy_file_announces_the_floats_it_decodes_to() {
    // A lossy codec declares no bit depth, and the fallback announced 16 bit
    // integers. Negotiation then opened a 16 bit word with nothing to report,
    // the packing scaled the floats into it, and the verdict called an MP3
    // bit-perfect.
    if !have_ffmpeg() {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let d = dir();
    let wav = d.join("ref-announce.wav");
    write_wav(&wav, 16);
    for (name, args) in [("announce.m4a", ["-c:a", "aac"]), ("announce.ogg", ["-c:a", "libvorbis"])] {
        let target = d.join(name);
        if !convert(&wav, &target, &args) {
            eprintln!("{name}: ffmpeg could not, skipped");
            continue;
        }
        let mut decoded = Decoded::open(&target).unwrap();
        let first = decoded.next_block().unwrap().expect("there is audio");
        assert!(matches!(first, Block::Float(_)), "{name} decodes to floats");
        assert_eq!(
            decoded.stream_format().sample_format,
            chilpa_audio::format::SampleFormat::F32,
            "{name} says so"
        );
        let _ = std::fs::remove_file(target);
    }
    let _ = std::fs::remove_file(wav);
}

#[test]
fn opus_is_not_supported_yet_and_says_so_plainly() {
    if !have_ffmpeg() {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let d = dir();
    let wav = d.join("ref-opus.wav");
    write_wav(&wav, 16);
    let target = d.join("test.opus");
    if !convert(&wav, &target, &["-c:a", "libopus"]) {
        eprintln!("no opus encoder: skipped");
        return;
    }
    match Decoded::open(&target) {
        Err(e) => {
            let text = e.to_string();
            assert!(text.contains("not supported"), "the error must say what happened: {text}");
        }
        Ok(_) => panic!("opus can be opened now: drop this test and add it to the matrix"),
    }
    let _ = std::fs::remove_file(target);
    let _ = std::fs::remove_file(wav);
}
