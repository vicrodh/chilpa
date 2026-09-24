//! A SACD image built by hand: master table, stereo area with two tracks,
//! and audio sectors with their real packing.

use chilpa_dsd::DsdSource;
use chilpa_sacd::{open, Encoding, SacdError, SacdTrackReader, DATA_TYPE_AUDIO, SECTOR};
use std::path::PathBuf;

const AREA_TOC_LSN: u32 = 540;
const AUDIO_START: u32 = 550;
const AUDIO_END: u32 = 569;
const SECTORS: usize = 600;

struct TestImage(Vec<u8>);

impl TestImage {
    fn new() -> Self {
        Self(vec![0u8; SECTORS * SECTOR])
    }

    fn sector(&mut self, lsn: u32) -> &mut [u8] {
        let at = lsn as usize * SECTOR;
        &mut self.0[at..at + SECTOR]
    }

    fn write(&self, name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-sacd-{name}.iso"));
        std::fs::write(&p, &self.0).unwrap();
        p
    }
}

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// Builds an image with two tracks; `dst` marks the area as compressed.
fn build(dst: bool) -> TestImage {
    let mut img = TestImage::new();

    {
        let m = img.sector(510);
        m[..8].copy_from_slice(b"SACDMTOC");
        m[8] = 1; // version 1.20
        m[9] = 20;
        // The stereo area slot: primary, backup and size in sectors.
        m[0x40..0x44].copy_from_slice(&be32(AREA_TOC_LSN));
        m[0x44..0x48].copy_from_slice(&be32(0));
        m[0x54..0x56].copy_from_slice(&3u16.to_be_bytes());
    }

    {
        let a = img.sector(AREA_TOC_LSN);
        a[..8].copy_from_slice(b"TWOCHTOC");
        a[0x14] = 4; // DSD64
        a[0x15] = if dst { 0 } else { 2 };
        a[0x20] = 2; // two channels
        a[0x21] = 0;
        a[0x40] = 0; // area duration: 0:02:00
        a[0x41] = 2;
        a[0x42] = 0;
        a[0x45] = 2; // two tracks
        a[0x48..0x4c].copy_from_slice(&be32(AUDIO_START));
        a[0x4c..0x50].copy_from_slice(&be32(AUDIO_END));
    }

    const SECOND_ARRAY: usize = 8 + 255 * 4;
    {
        let t1 = img.sector(AREA_TOC_LSN + 1);
        t1[..8].copy_from_slice(b"SACDTRL1");
        // Track 1: sectors 550..559. Track 2: 560..569.
        t1[8..12].copy_from_slice(&be32(AUDIO_START));
        t1[12..16].copy_from_slice(&be32(AUDIO_START + 10));
        t1[SECOND_ARRAY..SECOND_ARRAY + 4].copy_from_slice(&be32(10));
        t1[SECOND_ARRAY + 4..SECOND_ARRAY + 8].copy_from_slice(&be32(10));
    }
    {
        let t2 = img.sector(AREA_TOC_LSN + 2);
        t2[..8].copy_from_slice(b"SACDTRL2");
        // Starts at 0:00:00 and 0:01:00; one second each.
        t2[8..12].copy_from_slice(&[0, 0, 0, 0]);
        t2[12..16].copy_from_slice(&[0, 1, 0, 0]);
        t2[SECOND_ARRAY..SECOND_ARRAY + 4].copy_from_slice(&[0, 1, 0, 0]);
        t2[SECOND_ARRAY + 4..SECOND_ARRAY + 4 + 4].copy_from_slice(&[0, 1, 0, 0]);
    }

    // Audio sectors: one per sector, with its frame-info record.
    for (i, lsn) in (AUDIO_START..=AUDIO_END).enumerate() {
        let s = img.sector(lsn);
        s[0] = (1 << 5) | (1 << 2) | u8::from(dst);
        let info = if dst { 4 } else { 3 };
        let header_len = 1 + 2 + info;
        let len = SECTOR - header_len;
        let w: u16 = 0x8000 | (DATA_TYPE_AUDIO << 11) | len as u16;
        s[1..3].copy_from_slice(&w.to_be_bytes());
        s[3] = 0;
        s[4] = (i / 75) as u8;
        s[5] = (i % 75) as u8;
        // The payload: byte n of the left channel is n and the right one its
        // complement, interleaved byte by byte.
        for (k, b) in s[header_len..].iter_mut().enumerate() {
            let sample = ((i * len + k) / 2 % 251) as u8;
            *b = if k % 2 == 0 { sample } else { !sample };
        }
    }
    img
}

#[test]
fn both_tracks_of_the_stereo_area_are_read() {
    let path = build(false).write("dos-pistas");
    let img = open(&path).unwrap();
    assert_eq!(img.area.encoding, Encoding::Dsd3In14);
    assert_eq!(img.area.tracks.len(), 2);
    assert_eq!(img.area.tracks[0].number, 1);
    assert_eq!(img.area.tracks[0].start_lsn, AUDIO_START);
    assert_eq!(img.area.tracks[1].start_lsn, AUDIO_START + 10);
    assert_eq!(img.area.tracks[0].duration_secs(), 1.0);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_track_delivers_stereo_dsd64_with_the_channels_split() {
    let path = build(false).write("audio");
    let img = open(&path).unwrap();
    let track = img.area.tracks[0].clone();
    let mut lector = SacdTrackReader::open(img, &track).unwrap();

    assert_eq!(lector.info().dsd_rate_hz, 2_822_400, "DSD64");
    assert_eq!(lector.info().channels, 2);

    let block = lector.next_block().unwrap().expect("hay audio");
    assert_eq!(block.len(), 2);
    assert_eq!(block[0].len(), 9408 / 2, "medio frame por canal");
    // The left one carries the sample and the right one its complement.
    for i in 0..16 {
        assert_eq!(block[1][i], !block[0][i], "los canales no se revolvieron");
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_compressed_track_is_refused_with_a_clear_message() {
    let path = build(true).write("dst");
    let img = open(&path).unwrap();
    assert_eq!(img.area.encoding, Encoding::Dst);
    let track = img.area.tracks[0].clone();
    match SacdTrackReader::open(img, &track) {
        Err(SacdError::Unsupported(msg)) => assert!(msg.contains("DST"), "{msg}"),
        Err(other) => panic!("debía rechazarse por DST: {other:?}"),
        Ok(_) => panic!("una pista con DST no se puede reproducir todavía"),
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn an_image_that_is_not_a_sacd_is_refused() {
    let img = TestImage::new();
    let path = img.write("vacia");
    assert!(matches!(open(&path), Err(SacdError::MalformedToc(_))));
    let _ = std::fs::remove_file(path);
}
