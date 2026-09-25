//! DSF, which the tag library underneath does not read.
//!
//! A library with 166 DSF files recorded every one of them with no title, no
//! album, no length and no format: the scan asked lofty, lofty does not know
//! DSF, and the file went in under its name. The files are built here, byte
//! by byte, because nothing on an ordinary machine writes DSF.

use chilpa_tags::{extract_artwork, read};
use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::tag::{Accessor, Tag, TagExt, TagType};
use std::path::PathBuf;

const RATE: u32 = 2_822_400;

/// A two channel DSF holding `seconds` of DSD silence, with `tag` at the end
/// where the format keeps it, or with no tag at all.
fn dsf(name: &str, seconds: u32, tag: Option<&Tag>) -> PathBuf {
    let mut id3 = Vec::new();
    if let Some(tag) = tag {
        tag.dump_to(&mut id3, WriteOptions::default()).unwrap();
    }
    let channels = 2u32;
    let samples = u64::from(RATE) * u64::from(seconds);
    let block = 4096u32;
    let per_channel = ((samples / 8) as usize).div_ceil(block as usize) * block as usize;
    let data_len = per_channel * channels as usize;
    let data_chunk = 12 + data_len as u64;
    let metadata = if id3.is_empty() { 0 } else { 28 + 52 + data_chunk };
    let total = 28 + 52 + data_chunk + id3.len() as u64;

    let mut out = Vec::new();
    out.extend_from_slice(b"DSD ");
    out.extend_from_slice(&28u64.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&metadata.to_le_bytes());
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&52u64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // version
    out.extend_from_slice(&0u32.to_le_bytes()); // DSD raw
    out.extend_from_slice(&2u32.to_le_bytes()); // stereo
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // bits per sample
    out.extend_from_slice(&samples.to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_chunk.to_le_bytes());
    out.resize(out.len() + data_len, 0x69);
    out.extend_from_slice(&id3);

    let path = std::env::temp_dir().join(format!("chilpa-tags-{name}.dsf"));
    std::fs::write(&path, out).unwrap();
    path
}

fn tagged() -> Tag {
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_title("Born On The Bayou".into());
    tag.set_artist("Creedence Clearwater Revival".into());
    tag.set_album("Bayou Country".into());
    tag.set_track(1);
    tag
}

#[test]
fn a_dsf_file_has_its_tags_its_length_and_its_format_read() {
    let path = dsf("tagged", 3, Some(&tagged()));
    let info = read(&path).expect("a DSF file can be read");
    assert_eq!(info.tags.title.as_deref(), Some("Born On The Bayou"));
    assert_eq!(info.tags.artist_line(), "Creedence Clearwater Revival");
    assert_eq!(info.tags.album.as_deref(), Some("Bayou Country"));
    assert_eq!(info.tags.track_number, Some(1));
    assert_eq!(info.duration.as_secs(), 3, "it lasted {:?}", info.duration);
    assert_eq!(info.sample_rate_hz, Some(RATE), "DSD64 is 2,822,400 one-bit samples a second");
    assert_eq!(info.channels, Some(2));
    assert_eq!(info.bit_depth, Some(1));
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_dsf_file_without_tags_still_has_its_length() {
    let path = dsf("untagged", 2, None);
    let info = read(&path).expect("a DSF file without tags can be read");
    assert_eq!(info.tags.title, None);
    assert_eq!(info.duration.as_secs(), 2);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_dsf_file_gives_up_its_cover() {
    let cover = b"\x89PNG\r\n\x1a\nnot really a png, but bytes are bytes".to_vec();
    let mut tag = tagged();
    tag.push_picture(
        Picture::unchecked(cover.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Png)
            .build(),
    );
    let path = dsf("cover", 1, Some(&tag));
    let cache = std::env::temp_dir().join("chilpa-tags-dsf-covers");
    let written = extract_artwork(&path, &cache)
        .expect("the cover can be read")
        .expect("there is a cover");
    assert_eq!(std::fs::read(&written).unwrap(), cover);
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_dir_all(cache);
}
