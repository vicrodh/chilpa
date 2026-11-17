//! MP4 sample entries, speaker order, EOF, seek and offline integrity.
use chilpa_audio::format::SampleFormat;
use chilpa_decode::{Block, Decoded};
use std::path::{Path, PathBuf};

fn fixture(name: &str, extension: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/a52/{name}-six-channel.{extension}"))
}
fn decode_all(decoder: &mut Decoded) -> Vec<f32> {
    let mut samples = Vec::new();
    while let Some(block) = decoder.next_block().unwrap() {
        let Block::Float(block) = block else {
            panic!("lossy audio must retain float samples")
        };
        assert!(block.iter().all(|sample| sample.is_finite()));
        samples.extend(block);
    }
    samples
}
fn check(name: &str) {
    let mut decoder =
        Decoded::open(&fixture(name, "m4a")).unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!(decoder.codec(), name);
    let format = decoder.stream_format();
    assert_eq!(format.sample_rate_hz, 48_000);
    assert_eq!(format.channels, 6);
    assert_eq!(format.sample_format, SampleFormat::F32);
    let samples = decode_all(&mut decoder);
    let frames = samples.len() / 6;
    assert!(
        (48_000..=50_688).contains(&frames),
        "{frames} decoded frames"
    );
    assert!(samples.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() > 1.0);
}
#[test]
fn ac3_in_mp4_opens_with_its_actual_layout() {
    check("ac3")
}
#[test]
fn eac3_in_mp4_opens_with_its_actual_layout() {
    check("eac3")
}

#[test]
fn complete_programs_match_the_reference_including_channel_order_and_final_packet() {
    for name in ["ac3", "eac3"] {
        let mut decoder = Decoded::open(&fixture(name, "m4a")).unwrap();
        let samples = decode_all(&mut decoder);
        let reference: Vec<_> = std::fs::read(fixture(name, "f32"))
            .unwrap()
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(
            samples.len(),
            reference.len(),
            "{name}: the final packet must be delivered"
        );
        for channel in 0..6 {
            let mut signal = 0.0;
            let mut error = 0.0;
            let mut cross = 0.0;
            for (&actual, &expected) in samples
                .iter()
                .skip(channel)
                .step_by(6)
                .zip(reference.iter().skip(channel).step_by(6))
            {
                let (actual, expected) = (f64::from(actual), f64::from(expected));
                signal += expected * expected;
                error += (actual - expected).powi(2);
                cross += actual * expected;
            }
            let snr = 10.0 * (signal / error.max(1e-30)).log10();
            let gain = cross / signal;
            assert!(
                snr > 60.0 && (gain - 1.0).abs() < 0.001,
                "{name} channel {channel}: SNR={snr}, gain={gain}"
            );
        }
        assert!(decoder.next_block().unwrap().is_none());
    }
}

#[test]
fn seek_clears_compressed_lookahead_and_primes_overlap_before_the_requested_sample() {
    for name in ["ac3", "eac3"] {
        let mut decoder = Decoded::open(&fixture(name, "m4a")).unwrap();
        let baseline = decode_all(&mut decoder);
        for target in [0u64, 1, 12_345, 36_789, 24_576] {
            let plan = decoder.seek_to_frame(target).unwrap();
            assert!(plan.request <= target, "{name} {target}: {plan:?}");
            let after = decode_all(&mut decoder);
            let offset = plan.discard as usize * 6;
            assert_eq!(
                after.len() - offset,
                baseline.len() - target as usize * 6,
                "{name} {target}: complete remainder"
            );
            let actual = &after[offset..offset + 96 * 6];
            let expected = &baseline[target as usize * 6..(target as usize + 96) * 6];
            let maximum = actual
                .iter()
                .zip(expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(maximum < 0.00002, "{name} {target}: sample error {maximum}");
        }
    }
}
fn atom(bytes: &[u8], tag: &[u8; 4]) -> usize {
    let matches: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter(|(_, value)| *value == tag)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(matches.len(), 1, "fixture atom must be unique");
    matches[0]
}
fn verify_bytes(bytes: &[u8]) -> Result<String, chilpa_decode::DecodeError> {
    let mut file = tempfile::tempfile().unwrap();
    use std::io::{Seek, Write};
    file.write_all(bytes).unwrap();
    file.rewind().unwrap();
    chilpa_decode::verification::file(file, Some("m4a"), || false)
}
#[test]
fn offline_verification_rejects_bad_checksums_and_oversized_access_units() {
    for name in ["ac3", "eac3"] {
        let bytes = std::fs::read(fixture(name, "m4a")).unwrap();
        assert_eq!(verify_bytes(&bytes).unwrap(), name);
        let mut corrupt = bytes.clone();
        let start = atom(&corrupt, b"mdat") + 4;
        corrupt[start + 100] ^= 0x80;
        assert!(
            verify_bytes(&corrupt).is_err(),
            "{name}: damaged first frame"
        );
        let mut oversized = bytes.clone();
        let size = atom(&oversized, b"stsz") + 8;
        oversized[size..size + 4].copy_from_slice(&65_537u32.to_be_bytes());
        let error = verify_bytes(&oversized).unwrap_err().to_string();
        assert!(error.contains("64 KiB"), "{name}: {error}");
        let mut corrupt = bytes;
        let end = start - 8
            + u32::from_be_bytes(corrupt[start - 8..start - 4].try_into().unwrap()) as usize;
        corrupt[end - 1] ^= 1;
        assert!(
            verify_bytes(&corrupt).is_err(),
            "{name}: damaged final frame"
        );
    }
}
#[test]
fn mismatched_or_oversized_configuration_never_opens_as_empty_pcm() {
    for (name, config, wrong) in [("ac3", b"dac3", b"dec3"), ("eac3", b"dec3", b"dac3")] {
        let bytes = std::fs::read(fixture(name, "m4a")).unwrap();
        let at = atom(&bytes, config);
        for oversized in [false, true] {
            let mut invalid = bytes.clone();
            if oversized {
                invalid[at - 4..at].copy_from_slice(&8192u32.to_be_bytes());
            } else {
                invalid[at..at + 4].copy_from_slice(wrong);
            }
            let mut file = tempfile::tempfile().unwrap();
            use std::io::{Seek, Write};
            file.write_all(&invalid).unwrap();
            file.rewind().unwrap();
            assert!(
                Decoded::open_file(file, Some("m4a")).is_err(),
                "{name}: invalid configuration"
            );
        }
    }
}

#[test]
fn valid_checksums_do_not_turn_an_unsupported_audio_block_into_successful_verification() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/a52/crc-valid-invalid-audio.m4a");
    let bytes = std::fs::read(path).unwrap();
    let start = atom(&bytes, b"mdat") + 4;
    assert!(
        oxideav_ac3::decoder::verify_packet_crc(&bytes[start..start + 1792])
            .unwrap()
            .all_ok()
    );
    assert!(
        verify_bytes(&bytes).is_err(),
        "an undecodable block with a valid CRC must not be silently accepted"
    );
}

#[test]
fn mp4_time_scale_does_not_change_pcm_duration_or_seek_position() {
    for name in ["ac3", "eac3"] {
        let mut normal = Decoded::open(&fixture(name, "m4a")).unwrap();
        let expected_count = normal.frame_count();
        let baseline = decode_all(&mut normal);
        let mut bytes = std::fs::read(fixture(name, "m4a")).unwrap();
        let mdhd = atom(&bytes, b"mdhd");
        assert_eq!(bytes[mdhd + 4], 0);
        for at in [mdhd + 16, mdhd + 20] {
            let value = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
            bytes[at..at + 4].copy_from_slice(&(value * 2).to_be_bytes());
        }
        let stts = atom(&bytes, b"stts");
        let entries = u32::from_be_bytes(bytes[stts + 8..stts + 12].try_into().unwrap());
        for index in 0..entries as usize {
            let at = stts + 16 + index * 8;
            let value = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
            bytes[at..at + 4].copy_from_slice(&(value * 2).to_be_bytes());
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scaled.m4a");
        std::fs::write(&path, &bytes).unwrap();
        let mut decoded = Decoded::open(&path).unwrap();
        assert_eq!(
            decoded.frame_count(),
            expected_count,
            "{name}: frames are PCM samples"
        );
        assert_eq!(decode_all(&mut decoded), baseline);
        for target in [0u64, 12_345, 36_789, 24_576] {
            let plan = decoded.seek_to_frame(target).unwrap();
            assert!(plan.request <= target, "{name} {target}: {plan:?}");
            let after = decode_all(&mut decoded);
            let offset = plan.discard as usize * 6;
            assert_eq!(after.len() - offset, baseline.len() - target as usize * 6);
            let maximum = after[offset..offset + 96 * 6]
                .iter()
                .zip(&baseline[target as usize * 6..(target as usize + 96) * 6])
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(
                maximum < 0.00002,
                "{name} {target}: scaled sample error {maximum}"
            );
        }
        assert_eq!(verify_bytes(&bytes).unwrap(), name);
    }
}

#[test]
fn legacy_half_rate_profiles_are_not_played_at_the_base_sample_rate() {
    for bsid in [9u8, 10] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/a52/half-rate-{bsid}.ac3"));
        let frame = std::fs::read(path).unwrap();
        assert!(
            oxideav_ac3::decoder::verify_packet_crc(&frame)
                .unwrap()
                .all_ok()
        );
        let original = std::fs::read(fixture("ac3", "m4a")).unwrap();
        let mut bytes = original.clone();
        let start = atom(&bytes, b"mdat") + 4;
        bytes[start..start + frame.len()].copy_from_slice(&frame);
        let mut file = tempfile::tempfile().unwrap();
        use std::io::{Seek, Write};
        file.write_all(&bytes).unwrap();
        file.rewind().unwrap();
        let error = match Decoded::open_file(file, Some("m4a")) {
            Ok(_) => panic!("bsid {bsid}: a half-rate program was accepted at the base rate"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("half-rate"), "bsid {bsid}: {error}");
    }
}
