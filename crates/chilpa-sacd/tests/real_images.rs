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
                "smoke: {:?}, {} tracks | {} / {} / {} | {name}",
                image.area.encoding,
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
    // CHILPA_SMOKE_SACD_TRACK="/path/to/image.iso#2", then compare with
    // `cargo run -p qbz-dsd --example sacd_verify -- /path/to/image.iso 2` in QBZ.
    let Some(spec) = std::env::var("CHILPA_SMOKE_SACD_TRACK").ok() else {
        eprintln!("smoke: set CHILPA_SMOKE_SACD_TRACK to image.iso#number to run this");
        return;
    };
    let (image, number) = spec.rsplit_once('#').expect("image.iso#number");
    let number: u8 = number.parse().unwrap();
    let img = chilpa_sacd::open(Path::new(image)).unwrap();
    let track = img.area.tracks.iter().find(|t| t.number == number).unwrap().clone();
    let expected = u64::from(track.duration_frames) * 4704;
    let mut reader = chilpa_sacd::SacdTrackReader::open(img, &track).unwrap();
    use chilpa_dsd::DsdSource as _;
    let (mut total, mut hashes) = (0u64, [0xcbf2_9ce4_8422_2325u64; 2]);
    while let Some(block) = reader.next_block().unwrap() {
        for (hash, channel) in hashes.iter_mut().zip(&block) {
            *hash = fnv1a(*hash, channel);
        }
        total += block[0].len() as u64;
    }
    eprintln!(
        "smoke: track {number}: {total} bytes/channel (the track is {expected}) fnv1a64={:016x}/{:016x}",
        hashes[0], hashes[1]
    );
    assert_eq!(total, expected, "a track is as long as the disc says, no more and no less");
}
