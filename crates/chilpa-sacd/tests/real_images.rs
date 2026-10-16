//! Every SACD image under a folder, opened for real.
//!
//! Ignored by default: it reads files somebody owns. The synthetic images in
//! `image.rs` say the parser follows the format as it was written down; this
//! says it reads the images people actually have, which were made by
//! whatever ripper was at hand.
//!
//! ```text
//! CHILPA_SMOKE_SACD=/path/to/music \
//!   cargo test -p chilpa-sacd --test real_images -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};

fn images_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("iso")) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
#[ignore = "reads real disc images; run it on purpose"]
fn every_sacd_image_under_a_folder_opens() {
    let Some(root) = std::env::var_os("CHILPA_SMOKE_SACD").map(PathBuf::from) else {
        eprintln!("smoke: set CHILPA_SMOKE_SACD to a folder to run this");
        return;
    };
    let mut failed = Vec::new();
    for path in images_under(&root) {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match chilpa_sacd::open(&path) {
            Ok(image) => eprintln!(
                "smoke: {:?}, disc {:?}/{:?}, {} tracks | {} / {} / {} | {name}",
                image.area.encoding,
                image.area.disc_number,
                image.area.disc_total,
                image.area.tracks.len(),
                image.area.artist.as_deref().unwrap_or("-"),
                image.area.album.as_deref().unwrap_or("-"),
                image.area.tracks[0].title.as_deref().unwrap_or("-"),
            ),
            // Not every ISO is a SACD; a DVD beside the music is not a failure.
            Err(chilpa_sacd::SacdError::MalformedToc(why)) if why.contains("SACDMTOC") => {
                eprintln!("smoke: not a SACD | {name}")
            }
            Err(e) => {
                eprintln!("smoke: FAILED {e} | {name}");
                failed.push(name);
            }
        }
    }
    assert!(failed.is_empty(), "these SACD images could not be read: {failed:#?}");
}

/// FNV-1a over 64 bits, the hash QBZ's `sacd_verify` prints per channel, so
/// the two readers can be compared without writing the audio anywhere.
fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[test]
#[ignore = "reads a real disc image; run it on purpose"]
fn a_real_track_hashes_the_way_qbz_s_reader_did() {
    // CHILPA_SMOKE_SACD_TRACK="/path/to/image.iso#2", or the image alone for
    // every track, then compare with
    // `cargo run -p qbz-dsd --example sacd_verify -- /path/to/image.iso [2]` in
    // QBZ. The lines are printed the way that example prints them.
    let Some(spec) = std::env::var("CHILPA_SMOKE_SACD_TRACK").ok() else {
        eprintln!("smoke: set CHILPA_SMOKE_SACD_TRACK to image.iso or image.iso#number to run this");
        return;
    };
    let (image, number) = match spec.rsplit_once('#') {
        Some((image, number)) => (image, Some(number.parse::<u8>().unwrap())),
        None => (spec.as_str(), None),
    };
    let img = chilpa_sacd::open(Path::new(image)).unwrap();
    let tracks: Vec<_> =
        img.area.tracks.iter().filter(|t| number.is_none_or(|n| t.number == n)).cloned().collect();
    assert!(!tracks.is_empty(), "the image has no such track");
    for track in tracks {
        let expected = u64::from(track.duration_frames) * 4704;
        let disc = chilpa_sacd::open(Path::new(image)).unwrap();
        let mut reader = chilpa_sacd::SacdTrackReader::open(disc, &track).unwrap();
        use chilpa_dsd::DsdSource as _;
        let middle = u64::from(track.duration_frames / 2);
        let (mut total, mut hashes, mut second_half) = (0u64, [FNV; 2], [FNV; 2]);
        let (mut first_frame, mut last_frame) = (None, [FNV; 2]);
        while let Some(block) = reader.next_block().unwrap() {
            let frame = total / 4704;
            for (c, channel) in block.iter().enumerate() {
                hashes[c] = fnv1a(hashes[c], channel);
                if frame >= middle {
                    second_half[c] = fnv1a(second_half[c], channel);
                }
                last_frame[c] = fnv1a(FNV, channel);
            }
            first_frame.get_or_insert([fnv1a(FNV, &block[0]), fnv1a(FNV, &block[1])]);
            total += block[0].len() as u64;
        }
        println!("track {:>3}: {total} bytes/channel fnv1a64={:016x}/{:016x}", track.number, hashes[0], hashes[1]);
        assert_eq!(total, expected, "track {}: as long as the disc says, no more and no less", track.number);

        // Seeking, when one track is asked for: DST frames are not the same
        // size, so the sector of a frame is searched for by timecode.
        if number.is_some() {
            let disc = chilpa_sacd::open(Path::new(image)).unwrap();
            let mut moved = chilpa_sacd::SacdTrackReader::open(disc, &track).unwrap();
            assert_eq!(moved.seek(middle * PER_FRAME).unwrap(), Some(middle * PER_FRAME));
            let mut after = [FNV; 2];
            while let Some(block) = moved.next_block().unwrap() {
                for (c, channel) in block.iter().enumerate() {
                    after[c] = fnv1a(after[c], channel);
                }
            }
            assert_eq!(after, second_half, "from the middle, the same as the second half read through");
            let last = u64::from(track.duration_frames - 1);
            assert_eq!(moved.seek(last * PER_FRAME).unwrap(), Some(last * PER_FRAME));
            let block = moved.next_block().unwrap().expect("the last frame");
            assert_eq!([fnv1a(FNV, &block[0]), fnv1a(FNV, &block[1])], last_frame, "the last frame");
            assert!(moved.next_block().unwrap().is_none(), "and nothing after it");
            assert_eq!(moved.seek(0).unwrap(), Some(0));
            let block = moved.next_block().unwrap().expect("the first frame");
            assert_eq!(Some([fnv1a(FNV, &block[0]), fnv1a(FNV, &block[1])]), first_frame, "and back to the first");
            println!("           seeks to the middle, the last frame and the start agree");
        }
    }
}

const FNV: u64 = 0xcbf2_9ce4_8422_2325;
/// One-bit samples of a channel in a frame.
const PER_FRAME: u64 = 2_822_400 / 75;
