//! A synthetic DSF read end to end, with its bits intact.

use chilpa_audio::dop::DopPacker;
use chilpa_dsd::{DsdSource, DsfReader};
use std::io::Write;
use std::path::PathBuf;

const BLOCK: u32 = 4096;
const RATE: u32 = 2_822_400; // DSD64

/// Writes a two-channel DSF with `blocks` blocks per channel.
///
/// Byte n of the left channel is n and the right one its complement, so any
/// shift or mixing between channels shows.
fn write_dsf(path: &PathBuf, blocks: u32, bits_per_sample: u32, samples: u64) {
    let data = u64::from(blocks) * u64::from(BLOCK) * 2;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"DSD ").unwrap();
    f.write_all(&28u64.to_le_bytes()).unwrap();
    f.write_all(&(28u64 + 52 + 12 + data).to_le_bytes()).unwrap();
    f.write_all(&0u64.to_le_bytes()).unwrap(); // no tags
    f.write_all(b"fmt ").unwrap();
    f.write_all(&52u64.to_le_bytes()).unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap(); // version
    f.write_all(&0u32.to_le_bytes()).unwrap(); // format id: raw DSD
    f.write_all(&2u32.to_le_bytes()).unwrap(); // stereo
    f.write_all(&2u32.to_le_bytes()).unwrap(); // channels
    f.write_all(&RATE.to_le_bytes()).unwrap();
    f.write_all(&bits_per_sample.to_le_bytes()).unwrap();
    f.write_all(&samples.to_le_bytes()).unwrap();
    f.write_all(&BLOCK.to_le_bytes()).unwrap();
    f.write_all(&0u32.to_le_bytes()).unwrap(); // reserved
    f.write_all(b"data").unwrap();
    f.write_all(&(12 + data).to_le_bytes()).unwrap();
    for b in 0..blocks {
        for channel in 0..2u32 {
            for i in 0..BLOCK {
                let v = ((b * BLOCK + i) % 251) as u8;
                f.write_all(&[if channel == 0 { v } else { !v }]).unwrap();
            }
        }
    }
}

fn temp_file(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-dsd-{name}.dsf"));
    p
}

#[test]
fn the_header_and_every_block_are_read() {
    let path = temp_file("basic");
    let samples = u64::from(BLOCK) * 3 * 8;
    write_dsf(&path, 3, 1, samples);

    let mut r = DsfReader::open(&path).unwrap();
    assert_eq!(r.info().channels, 2);
    assert_eq!(r.info().dsd_rate_hz, RATE);
    assert_eq!(r.info().multiple(), 64, "DSD64");
    assert_eq!(r.info().samples, samples);

    let mut blocks = 0;
    let mut left_bytes = 0;
    while let Some(channels) = r.next_block().unwrap() {
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].len(), channels[1].len());
        left_bytes += channels[0].len();
        blocks += 1;
    }
    assert_eq!(blocks, 3);
    assert_eq!(left_bytes as u64, samples / 8, "every valid byte came out");
    let _ = std::fs::remove_file(path);
}

#[test]
fn bits_are_delivered_with_the_earliest_one_on_top() {
    // DSF stores the earliest bit low; here it has to come out reversed.
    let path = temp_file("order");
    write_dsf(&path, 1, 1, u64::from(BLOCK) * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let channels = r.next_block().unwrap().unwrap();
    assert_eq!(channels[0][0], 0u8.reverse_bits());
    assert_eq!(channels[0][1], 1u8.reverse_bits());
    assert_eq!(channels[1][1], (!1u8).reverse_bits(), "the right channel is the complement");
    let _ = std::fs::remove_file(path);

    // With bits_per_sample = 8 the file already has the earliest bit on top
    // and nothing is touched.
    let path = temp_file("order-msb");
    write_dsf(&path, 1, 8, u64::from(BLOCK) * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let channels = r.next_block().unwrap().unwrap();
    assert_eq!(channels[0][1], 1u8, "not inverted");
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_padding_of_the_last_block_does_not_come_out() {
    let path = temp_file("relleno");
    // Two blocks in the file, but only 100 bytes per channel are music.
    write_dsf(&path, 2, 1, 100 * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let first = r.next_block().unwrap().unwrap();
    assert_eq!(first[0].len(), 100, "only what is valid");
    assert!(r.next_block().unwrap().is_none(), "the rest was padding");
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_file_bits_reach_the_dop_packing_intact() {
    // The DSD equivalent of bit-perfect: the payload of each DoP word is
    // exactly the byte pair the file carried.
    let path = temp_file("dop");
    write_dsf(&path, 1, 1, u64::from(BLOCK) * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let channels = r.next_block().unwrap().unwrap();

    let mut packer = DopPacker::new();
    let mut words = Vec::new();
    packer.pack(&[&channels[0], &channels[1]], &mut words);

    assert_eq!(words.len(), channels[0].len() / 2 * 2, "one word per channel and per frame");
    for (frame, pair) in words.chunks(2).enumerate().take(8) {
        let marker = if frame % 2 == 0 { 0x05 } else { 0xFA };
        for (channel, word) in pair.iter().enumerate() {
            let unjustified = word >> 8;
            assert_eq!((unjustified >> 16) & 0xFF, marker, "the marker alternates per frame");
            let payload = (unjustified & 0xFFFF) as u16;
            let expected =
                u16::from_be_bytes([channels[channel][frame * 2], channels[channel][frame * 2 + 1]]);
            assert_eq!(payload, expected, "the payload is the file's DSD, untouched");
        }
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn seeking_lands_at_the_start_of_a_block() {
    let path = temp_file("seek");
    write_dsf(&path, 4, 1, u64::from(BLOCK) * 4 * 8);
    let mut r = DsfReader::open(&path).unwrap();
    let dropped = r.seek_to_sample(u64::from(BLOCK) * 8 * 2 + 40).unwrap();
    assert_eq!(dropped, u64::from(BLOCK) * 8 * 2, "it aligns to the block, backwards");
    let channels = r.next_block().unwrap().unwrap();
    let expected = ((BLOCK * 2) % 251) as u8;
    assert_eq!(channels[0][0], expected.reverse_bits(), "it starts at the right block");
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_audio_is_an_error_instead_of_the_end_of_the_track() {
    let path = temp_file("truncated");
    write_dsf(&path, 2, 1, u64::from(BLOCK) * 2 * 8);
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(file.metadata().unwrap().len() - 1).unwrap();
    let mut reader = DsfReader::open(&path).unwrap();
    assert!(reader.next_block().unwrap().is_some(), "the first block is complete");
    assert!(
        matches!(reader.next_block(), Err(chilpa_dsd::DsdError::Open(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof),
        "missing audio must reach the playback error path"
    );
    std::fs::remove_file(path).unwrap();
}
