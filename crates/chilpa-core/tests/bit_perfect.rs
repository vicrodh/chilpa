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
use chilpa_core::player::{Player, PlayerEvent};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// What a USB DAC that publishes only `S32_LE` looks like once measured.
/// ALSA's one format is both of Chilpa's 32 bit words, so both are listed.
fn only_32_bit_words() -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![44_100, 48_000],
        formats: vec![SampleFormat::S24In32, SampleFormat::S32],
        channels: vec![2],
        probed: true,
    }
}

#[test]
fn a_16_bit_wav_reaches_a_dac_that_only_takes_32_bit_words_with_every_sample_intact() {
    let originals = samples(16);
    let path = temp_file("16-in-32");
    write_wav(&path, &originals, 16, 44_100, 2);

    let mut cap = Capture { stream: None, bytes: Vec::new() };
    let played = play_file(
        &path,
        &only_32_bit_words(),
        Policy::BitPerfect,
        &StopSignal::new(),
        |format| {
            cap.stream = Some(format);
            Ok(Handle(&mut cap))
        },
    )
    .expect("16 bits fit in a wider word without losing one");

    assert!(played.delivery.nothing_was_converted());
    let words: Vec<i32> = cap
        .bytes
        .chunks_exact(4)
        .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    assert_eq!(words.len(), originals.len(), "one word per sample");
    let came_out: Vec<i32> = words.iter().map(|w| w >> 16).collect();
    assert_eq!(came_out, originals, "every sample sits on top of its word");
    assert!(words.iter().all(|w| w & 0xffff == 0), "with nothing but zeros below it");
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_24_bit_wav_reaches_a_dac_with_three_byte_words_with_every_sample_intact() {
    let originals = samples(24);
    let path = temp_file("24-in-3");
    write_wav(&path, &originals, 24, 96_000, 2);
    let only_packed = DeviceCaps {
        sample_rates: vec![96_000],
        formats: vec![SampleFormat::S16, SampleFormat::S24Packed],
        channels: vec![2],
        probed: true,
    };

    let mut cap = Capture { stream: None, bytes: Vec::new() };
    let played = play_file(&path, &only_packed, Policy::BitPerfect, &StopSignal::new(), |format| {
        cap.stream = Some(format);
        Ok(Handle(&mut cap))
    })
    .expect("24 bits fit in three bytes without losing one");

    assert!(played.delivery.nothing_was_converted());
    let came_out: Vec<i32> = cap
        .bytes
        .chunks_exact(3)
        .map(|c| i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8)
        .collect();
    assert_eq!(came_out, originals, "not one LSB of difference in three bytes");
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_lossy_track_plays_with_the_switch_on_and_is_not_called_bit_perfect() {
    let ffmpeg = std::process::Command::new("ffmpeg").arg("-version").output();
    if !ffmpeg.map(|o| o.status.success()).unwrap_or(false) {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let wav = temp_file("lossy-source");
    write_wav(&wav, &samples(16), 16, 44_100, 2);
    let aac = std::env::temp_dir().join("chilpa-bitperfect-lossy.m4a");
    let encoded = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&wav)
        .args(["-c:a", "aac"])
        .arg(&aac)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(encoded, "ffmpeg could not write AAC");

    let mut cap = Capture { stream: None, bytes: Vec::new() };
    let played = play_file(&aac, &only_32_bit_words(), Policy::BitPerfect, &StopSignal::new(), |format| {
        cap.stream = Some(format);
        Ok(Handle(&mut cap))
    })
    .expect("a lossy track plays with the switch on");
    assert!(!cap.bytes.is_empty(), "it played");
    assert!(
        !played.delivery.nothing_was_converted(),
        "its floats were rounded into integers, and that is not bit-perfect: {:?}",
        played.delivery
    );
    let _ = std::fs::remove_file(wav);
    let _ = std::fs::remove_file(aac);
}

#[test]
fn a_mono_wav_reaches_a_stereo_only_dac_with_each_sample_in_both_channels() {
    let originals: Vec<i32> = samples(16).into_iter().step_by(2).collect();
    let path = temp_file("mono");
    write_wav(&path, &originals, 16, 44_100, 1);
    let stereo_only = caps(SampleFormat::S16, 44_100);

    let mut cap = Capture { stream: None, bytes: Vec::new() };
    let played = play_file(&path, &stereo_only, Policy::BitPerfect, &StopSignal::new(), |format| {
        cap.stream = Some(format);
        Ok(Handle(&mut cap))
    })
    .expect("a mono track plays on a stereo DAC");

    assert!(played.delivery.nothing_was_converted());
    let words: Vec<i32> = cap
        .bytes
        .chunks_exact(2)
        .map(|c| i32::from(i16::from_le_bytes([c[0], c[1]])))
        .collect();
    let left: Vec<i32> = words.iter().step_by(2).copied().collect();
    let right: Vec<i32> = words.iter().skip(1).step_by(2).copied().collect();
    assert_eq!(left, originals, "every sample, in the left channel");
    assert_eq!(right, originals, "and the same one in the right");
    let _ = std::fs::remove_file(path);
}

#[test]
fn with_the_switch_off_a_rate_the_device_lacks_never_plays_at_the_wrong_speed() {
    // A 44.1 kHz track on a device that runs only at 48: a laptop codec, or
    // a JACK graph. Opening at 48 and handing over the 44.1 samples as they
    // are plays them 8.8 per cent fast and a semitone and a half high, and
    // the transform said "resampled" while nothing had been. Whatever
    // happens, it is not that: it plays resampled or it is refused.
    // A second of it: long enough that 8.8 per cent is thousands of frames
    // and not something a tolerance could swallow.
    let originals: Vec<i32> = (0..44_100).flat_map(|i| [i % 30_000, -(i % 30_000)]).collect();
    let path = temp_file("wrong-speed");
    write_wav(&path, &originals, 16, 44_100, 2);
    let only_48k = caps(SampleFormat::S16, 48_000);

    let mut cap = Capture { stream: None, bytes: Vec::new() };
    let played = play_file(&path, &only_48k, Policy::AllowAnything, &StopSignal::new(), |format| {
        cap.stream = Some(format);
        Ok(Handle(&mut cap))
    });
    if let Err(e) = &played {
        assert!(
            matches!(
                e,
                chilpa_core::playback::PlayError::Negotiation(
                    chilpa_audio::negotiate::NegotiationError::Rate { requested_hz: 44_100, .. }
                )
            ),
            "refused, but not for the rate: {e}"
        );
    }
    if let Ok(played) = played {
        let rate = cap.stream.expect("opened").sample_rate_hz;
        let frames_in = (originals.len() / 2) as f64;
        let frames_out = (cap.bytes.len() / 4) as f64;
        let expected = frames_in * f64::from(rate) / 44_100.0;
        assert!(
            (frames_out - expected).abs() <= expected * 0.01,
            "opened at {rate} Hz, {frames_in} frames in and {frames_out} out: the samples were \
             not resampled, so they play at the wrong speed ({:?})",
            played.delivery.transforms
        );
    }
    let _ = std::fs::remove_file(path);
}

/// A device that takes whatever it is given, at the format it was opened
/// with.
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
fn the_switch_plays_16_bits_on_a_32_bit_dac_and_still_takes_the_output_alone() {
    // The switch reaches the device as two things: which samples may go out,
    // and asking for the output exclusively. Letting widening through by
    // handing the player a looser policy would have dropped the second one
    // without a word, because the stream path read it off the policy.
    let path = temp_file("switch");
    write_wav(&path, &samples(16), 16, 44_100, 2);
    let asked: Arc<Mutex<Vec<(StreamFormat, bool)>>> = Arc::default();
    let seen = Arc::clone(&asked);
    let player = Player::spawn(
        Box::new(|_| Ok(only_32_bit_words())),
        Box::new(move |_, format, exclusive| {
            seen.lock().unwrap().push((format, exclusive));
            Ok(Box::new(Swallow(format)) as Box<dyn AudioSink + Send>)
        }),
    );

    // An identifier that names no stack, so the verdict does not go looking
    // for one on this machine.
    player.play(&path, "dac", true);
    let limit = Instant::now() + Duration::from_secs(5);
    let mut outcome = None;
    while outcome.is_none() && Instant::now() < limit {
        outcome = player
            .poll()
            .into_iter()
            .find(|e| matches!(e, PlayerEvent::Started { .. } | PlayerEvent::Failed { .. }));
        std::thread::sleep(Duration::from_millis(5));
    }
    match outcome {
        Some(PlayerEvent::Started { opened, bits, bit_perfect, .. }) => {
            assert_eq!(opened.sample_rate_hz, 44_100, "nothing was resampled");
            assert!(
                opened.sample_format.is_integer() && opened.sample_format.valid_bits() > 16,
                "it went out in the wider word the DAC takes: {opened:?}"
            );
            // What the window shows and the history keeps. The word is wider;
            // the music in it is still 16 bits.
            assert_eq!(bits, Some(16), "a 16 bit track is reported as 16 bits");
            assert!(!bit_perfect, "\"dac\" names no stack, so nothing vouches for the road");
        }
        other => panic!("bit-perfect on did not play a 16 bit file on this DAC: {other:?}"),
    }
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1, "the device was opened once: {asked:?}");
    assert!(asked[0].1, "with the switch on the output is asked for alone");
    drop(asked);
    drop(player);
    let _ = std::fs::remove_file(path);
}
