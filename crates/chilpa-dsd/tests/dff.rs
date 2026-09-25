//! A synthetic DSDIFF read end to end.

use chilpa_dsd::{DffReader, DsdError, DsdSource};
use std::path::PathBuf;

const RATE: u32 = 2_822_400; // DSD64

/// A chunk: id, big-endian size, payload, and a pad byte when the size is
/// odd, as DSDIFF wants.
fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// A two channel DSDIFF with `bytes` bytes of each channel: byte `i` of the
/// left one is `i % 251` and the right one its complement. `compression` is
/// what CMPR says; its name is odd-sized on purpose, so its chunk is padded.
fn write_dff(name: &str, bytes: usize, compression: &[u8; 4]) -> PathBuf {
    let mut prop = b"SND ".to_vec();
    prop.extend(chunk(b"FS  ", &RATE.to_be_bytes()));
    let mut channel_ids = 2u16.to_be_bytes().to_vec();
    channel_ids.extend_from_slice(b"SLFTSRGT");
    prop.extend(chunk(b"CHNL", &channel_ids));
    let mut compression_chunk = compression.to_vec();
    compression_chunk.push(3);
    compression_chunk.extend_from_slice(b"raw");
    prop.extend(chunk(b"CMPR", &compression_chunk));

    let mut samples = Vec::with_capacity(bytes * 2);
    for i in 0..bytes {
        let v = (i % 251) as u8;
        samples.extend_from_slice(&[v, !v]);
    }
    let mut form = b"DSD ".to_vec();
    form.extend(chunk(b"FVER", &0x0105_0000u32.to_be_bytes()));
    form.extend(chunk(b"PROP", &prop));
    form.extend(chunk(b"DSD ", &samples));
    form.extend(chunk(b"DIIN", b"x"));

    let path = std::env::temp_dir().join(format!("chilpa-dff-{name}.dff"));
    std::fs::write(&path, chunk(b"FRM8", &form)).unwrap();
    path
}

#[test]
fn the_properties_and_every_byte_are_read_in_order() {
    let path = write_dff("read", 10_000, b"DSD ");
    let mut reader = DffReader::open(&path).unwrap();
    assert_eq!(reader.info().dsd_rate_hz, RATE);
    assert_eq!(reader.info().channels, 2);
    assert_eq!(reader.info().samples, 10_000 * 8, "eight one-bit samples a byte");

    let (mut left, mut right) = (Vec::new(), Vec::new());
    while let Some(block) = reader.next_block().unwrap() {
        assert_eq!(block[0].len(), block[1].len());
        left.extend_from_slice(&block[0]);
        right.extend_from_slice(&block[1]);
    }
    assert_eq!(left.len(), 10_000, "nothing after the DSD chunk is taken for music");
    for (i, (l, r)) in left.iter().zip(&right).enumerate() {
        assert_eq!((*l, *r), ((i % 251) as u8, !((i % 251) as u8)), "byte {i}, as it was, in its channel");
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_seek_lands_on_the_byte_it_asks_for() {
    let path = write_dff("seek", 10_000, b"DSD ");
    let mut reader = DffReader::open(&path).unwrap();
    assert_eq!(reader.seek(8 * 7_000 + 3).unwrap(), Some(8 * 7_000), "to the byte the sample is in");
    let block = reader.next_block().unwrap().unwrap();
    assert_eq!((block[0][0], block[1][0]), ((7_000 % 251) as u8, !((7_000 % 251) as u8)));
    assert_eq!(reader.seek(u64::MAX).unwrap(), Some(10_000 * 8), "past the end is the end");
    assert!(reader.next_block().unwrap().is_none());
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_dst_compressed_dsdiff_is_refused_with_the_reason() {
    let path = write_dff("dst", 100, b"DST ");
    match DffReader::open(&path) {
        Err(DsdError::Unsupported(why)) => assert!(why.contains("DST"), "{why}"),
        Err(other) => panic!("refused for another reason: {other}"),
        Ok(_) => panic!("a DST DSDIFF was opened as if it were plain DSD"),
    }
    let _ = std::fs::remove_file(path);
}
