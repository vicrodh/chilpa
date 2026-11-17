//! Explicit stereo conversion preserves frames, identity, seek and headroom.
use chilpa_audio::format::SampleFormat;
use chilpa_decode::{Block, Decoded};
use std::path::{Path, PathBuf};
fn fixture(channels: usize, extension: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "tests/fixtures/channels/layout-{channels}{extension}"
    ))
}
fn all(decoded: &mut Decoded) -> Vec<f32> {
    let mut result = Vec::new();
    while let Some(block) = decoded.next_block().unwrap() {
        let Block::Float(data) = block else {
            panic!("explicit downmix must produce float")
        };
        result.extend(data);
    }
    result
}
#[test]
fn multichannel_output_is_preserved_until_stereo_is_explicitly_requested() {
    for channels in [6, 8] {
        let mut original = Decoded::open(&fixture(channels, ".wav")).unwrap();
        assert_eq!(usize::from(original.stream_format().channels), channels);
        assert_eq!(original.stream_format().sample_format, SampleFormat::S16);
        let mut frames = 0;
        while let Some(block) = original.next_block().unwrap() {
            assert!(matches!(&block, Block::Int { bits: 16, .. }));
            frames += block.frames(channels as u16);
        }
        assert_eq!(frames, 4096);
    }
}
#[test]
fn explicit_stereo_matches_the_reference_without_losing_the_pending_block() {
    for channels in [6, 8] {
        let mut decoded = Decoded::open(&fixture(channels, ".wav")).unwrap();
        let codec = decoded.codec();
        let count = decoded.frame_count();
        decoded.downmix_to_stereo().unwrap();
        decoded.downmix_to_stereo().unwrap();
        assert_eq!(decoded.codec(), codec);
        assert_eq!(decoded.frame_count(), count);
        assert_eq!(
            usize::from(decoded.source_stream_format().channels),
            channels
        );
        assert_eq!(decoded.stream_format().channels, 2);
        assert_eq!(decoded.stream_format().sample_format, SampleFormat::F32);
        let actual = all(&mut decoded);
        let expected: Vec<_> = std::fs::read(fixture(channels, "-stereo.f32"))
            .unwrap()
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        assert_eq!(actual.len(), expected.len());
        let error = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(error < 0.000001, "{channels}: maximum stereo error {error}");
        assert!(actual.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        for target in [0u64, 1, 1234, 3072] {
            let plan = decoded.seek_to_frame(target).unwrap();
            let after = all(&mut decoded);
            let offset = plan.discard as usize * 2;
            assert_eq!(after.len() - offset, actual.len() - target as usize * 2);
            assert_eq!(&after[offset..], &actual[target as usize * 2..]);
        }
    }
}
