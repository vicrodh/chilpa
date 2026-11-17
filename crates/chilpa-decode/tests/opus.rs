//! Local and server-style file handles share the same Opus decoder registry.
use chilpa_audio::format::SampleFormat;
use chilpa_decode::{Block, Decoded};
use std::path::{Path, PathBuf};

fn fixture(channels: u16) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/opus/silence-{channels}.opus"))
}

#[test]
fn opus_files_decode_at_48_khz_as_float_with_their_original_channels() {
    for channels in [1, 2, 6] {
        for extension in ["opus", "ogg"] {
            let file = std::fs::File::open(fixture(channels)).unwrap();
            let mut decoded = Decoded::open_file(file, Some(extension)).unwrap();
            assert_eq!(decoded.codec(), "opus");
            let format = decoded.stream_format();
            assert_eq!(format.sample_rate_hz, 48_000);
            assert_eq!(format.channels, channels);
            assert_eq!(format.sample_format, SampleFormat::F32);
            let mut frames = 0;
            while let Some(block) = decoded.next_block().unwrap() {
                let Block::Float(samples) = block else {
                    panic!("lossy audio must retain float samples")
                };
                assert!(
                    samples
                        .iter()
                        .all(|sample| sample.is_finite() && sample.abs() < 0.001)
                );
                frames += samples.len() / usize::from(channels);
            }
            assert!(
                (48_000..=53_760).contains(&frames),
                "{channels} channels: {frames} frames"
            );
        }
    }
}

#[test]
fn complete_opus_originals_verify_without_playback() {
    for channels in [1, 2, 6] {
        let file = std::fs::File::open(fixture(channels)).unwrap();
        assert_eq!(
            chilpa_decode::verification::file(file, Some("opus"), || false).unwrap(),
            "opus"
        );
    }
}

#[test]
fn an_opus_file_seeks_after_reading_some_packets() {
    let mut decoded = Decoded::open(&fixture(2)).unwrap();
    for _ in 0..4 {
        assert!(decoded.next_block().unwrap().is_some());
    }
    for target in [0, 12_000, 24_000, 36_000] {
        let plan = decoded.seek_to_frame(target).unwrap();
        assert!(plan.request <= target);
        assert!(decoded.next_block().unwrap().is_some());
    }
}

fn decode_all(decoded: &mut Decoded) -> Vec<f32> {
    let mut samples = Vec::new();
    while let Some(block) = decoded.next_block().unwrap() {
        let Block::Float(block) = block else {
            panic!("Opus must decode to float")
        };
        samples.extend(block);
    }
    samples
}

#[test]
fn multichannel_samples_match_the_reference_decoder_without_double_trimming() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opus");
    let reference: Vec<f32> = std::fs::read(root.join("channel-markers-prefix.f32"))
        .unwrap()
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert!(reference.iter().any(|sample| sample.abs() > 0.001));
    for (name, gain) in [
        ("channel-markers.opus", 1.0f32),
        ("channel-markers-gain.opus", 10.0f32.powf(6.0 / 20.0)),
    ] {
        let mut decoded = Decoded::open(&root.join(name)).unwrap();
        let delay = decoded.encoder_delay() as usize;
        let padding = decoded.encoder_padding() as usize;
        let samples = decode_all(&mut decoded);
        let audible = &samples[delay * 6..samples.len() - padding * 6];
        assert_eq!(
            audible.len(),
            96_000 * 6,
            "{name}: delay={delay}, padding={padding}"
        );
        for (index, (&actual, &expected)) in audible.iter().zip(&reference).enumerate() {
            assert!(
                (actual / gain - expected).abs() < 0.00002,
                "{name} sample {index}: {actual} versus {}",
                expected * gain
            );
        }
    }
}

#[test]
fn seeking_uses_the_opus_preroll_before_delivering_audible_samples() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opus/channel-markers.opus");
    let mut decoded = Decoded::open(&path).unwrap();
    let samples = decode_all(&mut decoded);
    for target in [0u64, 48_312, 72_312] {
        let plan = decoded.seek_to_frame(target).unwrap();
        let after = decode_all(&mut decoded);
        let offset = plan.discard as usize * 6;
        let actual = &after[offset..offset + 6 * 96];
        let raw = target as usize + decoded.encoder_delay() as usize;
        assert_eq!(
            after.len() - offset,
            samples.len() - raw * 6,
            "seek {target}: complete remainder, plan={plan:?}"
        );
        let expected = &samples[raw * 6..(raw + 96) * 6];
        let error = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(
            error < 0.00002,
            "seek to {target}: maximum sample error {error}"
        );
    }
}

#[test]
fn every_supported_channel_layout_matches_the_reference_planes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opus");
    for channels in 1..=8 {
        let mut decoded = Decoded::open(&root.join(format!("layout-{channels}.opus"))).unwrap();
        assert_eq!(usize::from(decoded.stream_format().channels), channels);
        let delay = decoded.encoder_delay() as usize;
        let padding = decoded.encoder_padding() as usize;
        let samples = decode_all(&mut decoded);
        let audible = &samples[delay * channels..samples.len() - padding * channels];
        assert_eq!(audible.len(), 48_000 * channels, "{channels} channels");
        let reference: Vec<_> = std::fs::read(root.join(format!("layout-{channels}-prefix.f32")))
            .unwrap()
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        let maximum = audible
            .iter()
            .zip(reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(
            maximum < 0.00002,
            "{channels} channels: maximum reference error {maximum}"
        );
    }
}

#[test]
fn short_streams_and_large_code3_packets_keep_the_complete_audible_timeline() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opus");
    for (name, channels) in [("short-stereo", 2), ("code3-32", 1), ("code3-48", 1)] {
        let reference: Vec<f32> = std::fs::read(root.join(format!("{name}.f32")))
            .unwrap()
            .chunks_exact(4)
            .map(|sample| f32::from_le_bytes(sample.try_into().unwrap()))
            .collect();
        for forward in [false, true] {
            let path = root.join(format!("{name}.opus"));
            let mut decoded = if forward {
                forward_file(&path)
            } else {
                Decoded::open(&path).unwrap()
            };
            let delay = decoded.encoder_delay() as usize;
            let padding = decoded.encoder_padding() as usize;
            let samples = decode_all(&mut decoded);
            if !forward {
                assert_eq!(
                    decoded.frame_count(),
                    Some((samples.len() / channels) as u64),
                    "{name}: container timeline"
                );
            }
            let audible = &samples[delay * channels..samples.len() - padding * channels];
            assert_eq!(
                audible.len(),
                reference.len(),
                "{name}: audible length, delay={delay}, padding={padding}"
            );
            let error = audible
                .iter()
                .zip(&reference)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(error < 0.00002, "{name}: sample error {error}");
            assert_eq!(
                chilpa_decode::verification::file(
                    std::fs::File::open(root.join(format!("{name}.opus"))).unwrap(),
                    Some("opus"),
                    || false
                )
                .unwrap(),
                "opus"
            );
        }
    }
}

fn forward_file(path: &Path) -> Decoded {
    let info = chilpa_stream::StreamInfo {
        content_type: Some("audio/ogg".into()),
        ..Default::default()
    };
    let input = std::io::Cursor::new(std::fs::read(path).unwrap());
    Decoded::open_stream(chilpa_stream::HttpStream::new(
        Box::new(input),
        info,
        chilpa_stream::NowPlaying::new(),
    ))
    .unwrap()
}

#[test]
fn final_padding_survives_large_pages_and_progressive_reading() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opus");
    let expected: Vec<f32> = std::fs::read(root.join("large-pages-ends.f32"))
        .unwrap()
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    for forward in [false, true] {
        let mut decoded = if forward {
            forward_file(&root.join("large-pages.opus"))
        } else {
            Decoded::open(&root.join("large-pages.opus")).unwrap()
        };
        let delay = decoded.encoder_delay() as usize * 6;
        let padding = decoded.encoder_padding() as usize * 6;
        let samples = decode_all(&mut decoded);
        let audible = &samples[delay..samples.len() - padding];
        assert_eq!(audible.len(), 72000 * 6, "forward={forward}: final padding");
        let ends: Vec<_> = audible[..1024 * 6]
            .iter()
            .chain(&audible[audible.len() - 1024 * 6..])
            .copied()
            .collect();
        let error = ends
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(error < 0.00002, "forward={forward}: sample error {error}");
    }
}
