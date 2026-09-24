//! The project's invariant: what goes out the cable is, sample by sample,
//! what the file carried.
//!
//! The test needs no DAC: the device is an in-memory capture that keeps
//! every byte it receives. A WAV is generated, played, and what was captured
//! is compared against the original samples. If anyone puts a resample, a
//! float conversion or a software volume in the path, this test catches it.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::render::Outcome;
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_core::playback::play_file;
use std::io::Write;
use std::path::{Path, PathBuf};

/// A fake device that keeps everything written to it.
#[derive(Default)]
struct Capture {
    stream: Option<StreamFormat>,
    bytes: Vec<u8>,
}

struct Handle<'a>(&'a mut Capture);

impl AudioSink for Handle<'_> {
    fn format(&self) -> StreamFormat {
        self.0.stream.expect("opened")
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        self.0.bytes.extend_from_slice(frames);
        Ok(frames.len() / self.0.stream.expect("opened").frame_bytes())
    }

    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// Writes a PCM WAV with those integer samples.
fn write_wav(path: &Path, samples: &[i32], bits: u16, rate: u32, channels: u16) {
    let bytes_per_sample = u32::from(bits) / 8;
    let data_len = samples.len() as u32 * bytes_per_sample;
    let byte_rate = rate * u32::from(channels) * bytes_per_sample;
    let block_align = channels * bits / 8;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap(); // Integer PCM
    f.write_all(&channels.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    f.write_all(&block_align.to_le_bytes()).unwrap();
    f.write_all(&bits.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for &s in samples {
        match bits {
            16 => f.write_all(&(s as i16).to_le_bytes()).unwrap(),
            24 => f.write_all(&s.to_le_bytes()[..3]).unwrap(),
            _ => f.write_all(&s.to_le_bytes()).unwrap(),
        }
    }
}

fn temp_file(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-bitperfect-{name}.wav"));
    p
}

/// Samples that hit the edges: silence, +/-1 LSB, and the ends of the range,
/// which is exactly where float scaling used to get it wrong.
fn samples(bits: u8) -> Vec<i32> {
    let max = (1i64 << (bits - 1)) as i32 - 1;
    let min = -(1i64 << (bits - 1)) as i32;
    let mut v = vec![0, 1, -1, max, min, max - 1, min + 1];
    for i in 0..64 {
        v.push((max / 64) * i - (max / 2));
    }
    v.extend_from_slice(&[max, min]);
    // Stereo: the right channel is inverted. The minimum has no opposite
    // inside the range, so the maximum is used there.
    v.iter().flat_map(|&s| [s, if s == min { max } else { -s }]).collect()
}

fn caps(format: SampleFormat, rate: u32) -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![rate],
        formats: vec![format],
        channels: vec![2],
        probed: true,
    }
}

#[test]
fn a_16_bit_wav_goes_out_identical_over_the_wire() {
    let originals = samples(16);
    let path = temp_file("16");
    write_wav(&path, &originals, 16, 44_100, 2);

    let mut cap = Capture { stream: None, bytes: Vec::new() };
    let played = play_file(
        &path,
        &caps(SampleFormat::S16, 44_100),
        Policy::BitPerfect,
        &StopSignal::new(),
        |format| {
            cap.stream = Some(format);
            Ok(Handle(&mut cap))
        },
    )
    .expect("it must play");

    assert!(played.delivery.nothing_was_converted(), "the delivery is declared bit-perfect");
    assert!(matches!(played.outcome, Outcome::Finished { .. }));

    let came_out: Vec<i32> = cap
        .bytes
        .chunks_exact(2)
        .map(|c| i32::from(i16::from_le_bytes([c[0], c[1]])))
        .collect();
    assert_eq!(came_out.len(), originals.len(), "every sample went out");
    assert_eq!(came_out, originals, "not one LSB of difference");
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_24_bit_wav_goes_out_identical_over_the_wire() {
    let originals = samples(24);
    let path = temp_file("24");
    write_wav(&path, &originals, 24, 96_000, 2);

    let mut cap = Capture { stream: None, bytes: Vec::new() };
    let played = play_file(
        &path,
        &caps(SampleFormat::S24In32, 96_000),
        Policy::BitPerfect,
        &StopSignal::new(),
        |format| {
            cap.stream = Some(format);
            Ok(Handle(&mut cap))
        },
    )
    .expect("it must play");

    assert!(played.delivery.nothing_was_converted());
    // S24In32 is left justified: the 24 valid bits on top.
    let came_out: Vec<i32> = cap
        .bytes
        .chunks_exact(4)
        .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) >> 8)
        .collect();
    assert_eq!(came_out, originals, "not one LSB of difference at 24 bits");
    let _ = std::fs::remove_file(path);
}

#[test]
fn with_a_device_that_cannot_do_it_we_fail_instead_of_resampling() {
    let path = temp_file("refusal");
    write_wav(&path, &samples(16), 16, 44_100, 2);
    let only_48k = caps(SampleFormat::S16, 48_000);

    let mut cap = Capture { stream: None, bytes: Vec::new() };
    let r = play_file(&path, &only_48k, Policy::BitPerfect, &StopSignal::new(), |format| {
        cap.stream = Some(format);
        Ok(Handle(&mut cap))
    });
    assert!(r.is_err(), "nothing is resampled behind your back");
    assert!(cap.bytes.is_empty(), "nothing was written to the device");
    let _ = std::fs::remove_file(path);
}
