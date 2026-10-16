//! DSDIFF, which lofty does not read.
//!
//! Chunks with big-endian sizes, padded to even lengths: `FRM8` holding
//! `PROP` (the rate in `FS  `, the channels in `CHNL`), the samples in
//! `DSD `, and the tag, when there is one, as an ID3v2 tag in a chunk called
//! `ID3 ` -- not in the format, but where every program that tags a DSDIFF
//! puts it. A file with no ID3 may still say its artist and title the way
//! the format does, in `DIAR` and `DITI`.

use crate::{Opened, TagError};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

/// A tag larger than this is not a tag.
const LARGEST_TAG: u64 = 64 * 1024 * 1024;

pub(crate) fn is_dff(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("dff"))
}

pub(crate) fn open(path: &Path) -> Result<Opened, TagError> {
    let mut file = BufReader::new(std::fs::File::open(path).map_err(TagError::Open)?);
    let not_dsdiff = || TagError::Unreadable("not a DSDIFF file".into());
    let mut head = [0u8; 16];
    file.read_exact(&mut head).map_err(TagError::Open)?;
    if &head[..4] != b"FRM8" || &head[12..16] != b"DSD " {
        return Err(not_dsdiff());
    }
    let (mut rate, mut channels, mut samples_len, mut tag) = (None, None, None, None);
    let (mut artist, mut title) = (None, None);
    while let Some((id, len, start)) = next_chunk(&mut file) {
        match &id {
            b"PROP" => {
                let mut kind = [0u8; 4];
                file.read_exact(&mut kind).map_err(TagError::Open)?;
                let end = start + len;
                while let Some((sub, sub_len, sub_start)) = next_chunk(&mut file).filter(|(_, _, at)| *at < end) {
                    let mut value = [0u8; 4];
                    match &sub {
                        b"FS  " => {
                            file.read_exact(&mut value).map_err(TagError::Open)?;
                            rate = Some(u32::from_be_bytes(value));
                        }
                        b"CHNL" => {
                            file.read_exact(&mut value[..2]).map_err(TagError::Open)?;
                            channels = Some(u16::from_be_bytes([value[0], value[1]]));
                        }
                        _ => {}
                    }
                    file.seek(SeekFrom::Start(sub_start + sub_len + (sub_len & 1))).map_err(TagError::Open)?;
                }
            }
            b"DSD " | b"DST " => samples_len = Some(len),
            b"ID3 " if len <= LARGEST_TAG => {
                let mut bytes = vec![0u8; len as usize];
                file.read_exact(&mut bytes).map_err(TagError::Open)?;
                tag = crate::dsf::id3(bytes);
            }
            b"DIAR" if len <= LARGEST_TAG => artist = comment(&mut file, len),
            b"DITI" if len <= LARGEST_TAG => title = comment(&mut file, len),
            _ => {}
        }
        file.seek(SeekFrom::Start(start + len + (len & 1))).map_err(TagError::Open)?;
    }
    // The format's own artist and title, when no ID3 said more.
    if tag.is_none() && (artist.is_some() || title.is_some()) {
        let mut own = lofty::tag::Tag::new(lofty::tag::TagType::Id3v2);
        if let Some(artist) = artist {
            own.insert_text(lofty::prelude::ItemKey::TrackArtist, artist);
        }
        if let Some(title) = title {
            own.insert_text(lofty::prelude::ItemKey::TrackTitle, title);
        }
        tag = Some(own);
    }
    let rate = rate.filter(|r| *r > 0);
    // Eight one-bit samples a byte, one byte of each channel in turn. A DST
    // file's chunk holds frames and not bytes of samples, so its length is
    // not known here.
    let duration = match (rate, channels, samples_len) {
        (Some(rate), Some(channels), Some(len)) if channels > 0 => {
            Duration::from_secs_f64((len / u64::from(channels) * 8) as f64 / f64::from(rate))
        }
        _ => Duration::ZERO,
    };
    Ok(Opened {
        tag,
        duration,
        sample_rate_hz: rate,
        bit_depth: Some(1),
        channels: channels.and_then(|c| u8::try_from(c).ok()),
    })
}

/// The next chunk: its id, its length and where its payload starts. `None`
/// at the end of the file.
fn next_chunk(file: &mut BufReader<std::fs::File>) -> Option<([u8; 4], u64, u64)> {
    let mut head = [0u8; 12];
    file.read_exact(&mut head).ok()?;
    let id = head[..4].try_into().ok()?;
    let len = u64::from_be_bytes(head[4..].try_into().ok()?);
    Some((id, len, file.stream_position().ok()?))
}

/// A `DIAR` or `DITI` chunk: a count, then that many bytes of text. Read as
/// UTF-8 when it is, and as Latin-1 otherwise, as the format leaves it open.
fn comment(file: &mut BufReader<std::fs::File>, len: u64) -> Option<String> {
    let mut count = [0u8; 4];
    file.read_exact(&mut count).ok()?;
    let count = u64::from(u32::from_be_bytes(count)).min(len.saturating_sub(4));
    let mut bytes = vec![0u8; count as usize];
    file.read_exact(&mut bytes).ok()?;
    let text = String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|&b| char::from(b)).collect());
    let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string();
    (!text.is_empty()).then_some(text)
}
