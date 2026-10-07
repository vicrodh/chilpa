//! DSF, which lofty does not read.
//!
//! A library with 166 DSF files recorded every one of them with no title, no
//! album, no length and no format: lofty refused the file and the scan filed
//! it under its name.
//!
//! The format is three chunks and a tag: `DSD ` with the file's size and
//! where the tag starts, `fmt ` with the stream, `data`, and an ID3v2 tag at
//! the end. The stream is read here. The tag is handed to lofty as the ID3v2
//! it is, dressed as an MPEG file with nothing after it, which lofty reads
//! without looking for audio once it is told not to read properties.

use crate::{Opened, TagError};
use lofty::config::{ParseOptions, ParsingMode};
use lofty::file::{FileType, TaggedFileExt};
use lofty::probe::Probe;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

/// A tag larger than this is not a tag: it is a file that points at the
/// wrong place for one.
const LARGEST_TAG: u64 = 64 * 1024 * 1024;

pub(crate) fn is_dsf(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("dsf"))
}

pub(crate) fn open(path: &Path) -> Result<Opened, TagError> {
    let mut file = std::fs::File::open(path).map_err(TagError::Open)?;
    // The `DSD ` chunk is 28 bytes and the `fmt ` chunk after it 52.
    let mut head = [0u8; 80];
    file.read_exact(&mut head).map_err(TagError::Open)?;
    if &head[..4] != b"DSD " || &head[28..32] != b"fmt " {
        return Err(TagError::Unreadable("not a DSF file".into()));
    }
    let u32_at = |at: usize| u32::from_le_bytes(head[at..at + 4].try_into().expect("four bytes"));
    let u64_at = |at: usize| u64::from_le_bytes(head[at..at + 8].try_into().expect("eight bytes"));
    let tag_starts = u64_at(20);
    let channels = u32_at(28 + 24);
    let rate = u32_at(28 + 28);
    let bits = u32_at(28 + 32);
    let samples = u64_at(28 + 36);
    let duration = if rate > 0 {
        Duration::from_secs_f64(samples as f64 / f64::from(rate))
    } else {
        Duration::ZERO
    };
    // A pointer of zero is the format's way of saying there is no tag.
    let tag = if tag_starts == 0 { None } else { tag_at(&mut file, tag_starts) };
    Ok(Opened {
        tag,
        duration,
        sample_rate_hz: (rate > 0).then_some(rate),
        bit_depth: u8::try_from(bits).ok(),
        channels: u8::try_from(channels).ok(),
    })
}

/// The ID3v2 tag that starts at `at` and runs to the end of the file.
fn tag_at(file: &mut std::fs::File, at: u64) -> Option<lofty::tag::Tag> {
    let end = file.seek(SeekFrom::End(0)).ok()?;
    if at >= end || end - at > LARGEST_TAG {
        return None;
    }
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut bytes = Vec::with_capacity((end - at) as usize);
    file.read_to_end(&mut bytes).ok()?;
    id3(bytes)
}

/// An ID3v2 tag on its own, as lofty reads one: dressed as an MPEG file with
/// nothing after it, and told not to look for the audio. A frame lofty
/// cannot parse fails the whole tag in its default mode; read again,
/// relaxed, only that frame is lost.
pub(crate) fn id3(bytes: Vec<u8>) -> Option<lofty::tag::Tag> {
    let read = |mode: ParsingMode| {
        let options = ParseOptions::new().read_properties(false).parsing_mode(mode);
        Probe::new(Cursor::new(bytes.as_slice())).options(options).set_file_type(FileType::Mpeg).read().ok()
    };
    let mut tagged = read(ParsingMode::BestAttempt).or_else(|| read(ParsingMode::Relaxed))?;
    let primary = tagged.primary_tag_type();
    tagged.remove(primary).or_else(|| {
        let first = tagged.first_tag()?.tag_type();
        tagged.remove(first)
    })
}
