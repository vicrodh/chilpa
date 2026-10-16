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
mod support;

/// A two channel DSF holding `seconds` of DSD silence, with `tag` at the end
/// where the format keeps it, or with no tag at all.
fn dsf(name: &str, seconds: u32, tag: Option<&Tag>) -> PathBuf {
    let mut id3 = Vec::new();
    if let Some(tag) = tag {
        tag.dump_to(&mut id3, WriteOptions::default()).unwrap();
    }
    dsf_with(name, seconds, id3)
}

/// The same file with `id3` written as it is, for tags lofty would not write.
fn dsf_with(name: &str, seconds: u32, id3: Vec<u8>) -> PathBuf {
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
fn a_dsf_id3_carries_disc_subtitle_and_work() {
    let path = dsf_with("disc-work", 1, support::id3_disc_work());
    let info = read(&path).unwrap();
    assert_eq!(info.tags.disc_subtitle.as_deref(), Some("Scene One"));
    assert_eq!(info.tags.work.as_deref(), Some("Das Rheingold"));
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

/// An ID3v2.3 tag as KORG AudioGate writes it: text frames in Latin-1, and
/// the year, the date and the time as four blanks each.
fn korg_tag() -> Vec<u8> {
    let frame = |id: &[u8; 4], text: &str| {
        let mut out = id.to_vec();
        out.extend_from_slice(&(text.len() as u32 + 1).to_be_bytes());
        out.extend_from_slice(&[0, 0, 0]);
        out.extend_from_slice(text.as_bytes());
        out
    };
    let mut frames = Vec::new();
    frames.extend(frame(b"TALB", "All Things Must Pass, 1970"));
    frames.extend(frame(b"TPE1", "George Harrison"));
    frames.extend(frame(b"TIT2", "My Sweet Lord"));
    frames.extend(frame(b"TCON", "Rock, Pop Rock"));
    frames.extend(frame(b"TYER", "    "));
    frames.extend(frame(b"TDAT", "    "));
    frames.extend(frame(b"TIME", "    "));
    let size = frames.len() as u32;
    let syncsafe = [(size >> 21) as u8 & 0x7f, (size >> 14) as u8 & 0x7f, (size >> 7) as u8 & 0x7f, size as u8 & 0x7f];
    let mut tag = b"ID3\x03\x00\x00".to_vec();
    tag.extend_from_slice(&syncsafe);
    tag.extend(frames);
    tag
}

#[test]
fn a_tag_with_a_blank_year_keeps_everything_else() {
    // lofty refused the whole tag over the year, and the album came in as
    // 23 files named by their file names, with no album to be found under.
    let path = dsf_with("korg", 1, korg_tag());
    let info = read(&path).expect("the file can be read");
    assert_eq!(info.tags.title.as_deref(), Some("My Sweet Lord"));
    assert_eq!(info.tags.album.as_deref(), Some("All Things Must Pass, 1970"));
    assert_eq!(info.tags.artists, ["George Harrison"]);
    assert_eq!(info.tags.year(), None, "a year of blanks is no year");
    let _ = std::fs::remove_file(path);
}
