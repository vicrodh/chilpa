//! DSDIFF, which the tag library underneath does not read either.
//!
//! Its length and format are in its own chunks, and its tag, when it has
//! one, is an ID3v2 tag in a chunk called `ID3 `, which the format does not
//! define but is where every program that tags a DSDIFF puts it.

use chilpa_tags::read;
use lofty::config::WriteOptions;
use lofty::tag::{Accessor, Tag, TagExt, TagType};
use std::path::PathBuf;

const RATE: u32 = 2_822_400;
mod support;

fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// A two channel DSDIFF holding `seconds` of DSD silence, with `tag` in an
/// `ID3 ` chunk after the samples, or with no tag.
fn dff(name: &str, seconds: u32, tag: Option<&Tag>) -> PathBuf {
    let mut extra = Vec::new();
    if let Some(tag) = tag {
        let mut id3 = Vec::new();
        tag.dump_to(&mut id3, WriteOptions::default()).unwrap();
        extra.extend(chunk(b"ID3 ", &id3));
    }
    dff_with(name, seconds, extra)
}

/// The same file with `extra` chunks after the samples.
fn dff_with(name: &str, seconds: u32, extra: Vec<u8>) -> PathBuf {
    let mut prop = b"SND ".to_vec();
    prop.extend(chunk(b"FS  ", &RATE.to_be_bytes()));
    let mut channel_ids = 2u16.to_be_bytes().to_vec();
    channel_ids.extend_from_slice(b"SLFTSRGT");
    prop.extend(chunk(b"CHNL", &channel_ids));
    prop.extend(chunk(b"CMPR", b"DSD \x0enot compressed"));
    let bytes = (RATE / 8 * seconds * 2) as usize;
    let mut form = b"DSD ".to_vec();
    form.extend(chunk(b"FVER", &0x0105_0000u32.to_be_bytes()));
    form.extend(chunk(b"PROP", &prop));
    form.extend(chunk(b"DSD ", &vec![0x69; bytes]));
    form.extend(extra);
    let path = std::env::temp_dir().join(format!("chilpa-tags-{name}.dff"));
    std::fs::write(&path, chunk(b"FRM8", &form)).unwrap();
    path
}

#[test]
fn a_dsdiff_file_has_its_tags_its_length_and_its_format_read() {
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_title("Proud Mary".into());
    tag.set_album("Bayou Country".into());
    let path = dff("tagged", 3, Some(&tag));
    let info = read(&path).expect("a DSDIFF file can be read");
    assert_eq!(info.tags.title.as_deref(), Some("Proud Mary"));
    assert_eq!(info.tags.album.as_deref(), Some("Bayou Country"));
    assert_eq!(info.duration.as_secs(), 3, "it lasted {:?}", info.duration);
    assert_eq!(info.sample_rate_hz, Some(RATE));
    assert_eq!(info.channels, Some(2));
    assert_eq!(info.bit_depth, Some(1));
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_dsdiff_id3_carries_disc_subtitle_and_work() {
    let path = dff_with("disc-work", 1, chunk(b"ID3 ", &support::id3_disc_work()));
    let info = read(&path).unwrap();
    assert_eq!(info.tags.disc_subtitle.as_deref(), Some("Scene One"));
    assert_eq!(info.tags.work.as_deref(), Some("Das Rheingold"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_dsdiff_file_without_tags_still_has_its_length() {
    let path = dff("untagged", 2, None);
    let info = read(&path).expect("a DSDIFF file without tags can be read");
    assert_eq!(info.tags.title, None);
    assert_eq!(info.duration.as_secs(), 2);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_dsdiff_file_without_id3_says_its_artist_and_title_the_formats_way() {
    // A count, then the text: what `DIAR` and `DITI` hold.
    let text = |s: &str| {
        let mut out = (s.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(s.as_bytes());
        out
    };
    let mut extra = chunk(b"DIAR", &text("Paul McCartney & Wings"));
    extra.extend(chunk(b"DITI", &text("Band On The Run")));
    let path = dff_with("comments", 1, extra);
    let info = read(&path).expect("the file can be read");
    assert_eq!(info.tags.artists, ["Paul McCartney & Wings"]);
    assert_eq!(info.tags.title.as_deref(), Some("Band On The Run"));
    assert_eq!(info.tags.album, None, "the format names no album");
    let _ = std::fs::remove_file(path);
}
