//! The tables of contents: what tracks the disc has and where they live.

use crate::{be16, be32, timecode_frames, Encoding, SacdError, SECTOR};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Where the master table lives, with its two backups.
const MASTER_TOC_LSN: [u64; 3] = [510, 520, 530];
const MAX_AREA_TOC_SECTORS: u16 = 96;

/// A track of the stereo area.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SacdTrack {
    pub number: u8,
    pub start_lsn: u32,
    pub length_lsn: u32,
    /// Duration in frames of 1/75 of a second.
    pub duration_frames: u32,
    pub encoding: Encoding,
    /// The title the disc gives the track, when it gives one.
    pub title: Option<String>,
    /// Who performs it, when the disc says: the artist of one track on a
    /// compilation.
    pub performer: Option<String>,
}

impl SacdTrack {
    pub fn duration_secs(&self) -> f64 {
        f64::from(self.duration_frames) / 75.0
    }
}

/// The disc's stereo area.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SacdArea {
    pub tracks: Vec<SacdTrack>,
    /// The album's title and artist, from the master table's text.
    pub album: Option<String>,
    pub artist: Option<String>,
    pub encoding: Encoding,
    /// First and last sector of the audio area.
    pub audio_start_lsn: u32,
    pub audio_end_lsn: u32,
}

/// How the image stores its sectors.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layout {
    /// The 2048 bytes that carry data, one sector after another.
    Logical,
    /// The whole 2064-byte physical sector: a 12-byte header, the 2048
    /// bytes, and a 4-byte trailer. Some rippers keep it, and every image
    /// made that way used to be taken for something that is not a SACD.
    Physical,
}

impl Layout {
    fn stride(self) -> u64 {
        match self {
            Layout::Logical => SECTOR as u64,
            Layout::Physical => 2064,
        }
    }

    fn offset(self) -> u64 {
        match self {
            Layout::Logical => 0,
            Layout::Physical => 12,
        }
    }
}

/// The open image, with its stereo area already read.
pub struct SacdImage {
    pub(crate) file: BufReader<File>,
    pub(crate) sectors: u64,
    layout: Layout,
    pub area: SacdArea,
}

impl SacdImage {
    pub(crate) fn read_sector(&mut self, lsn: u64, into: &mut [u8]) -> Result<(), SacdError> {
        if lsn >= self.sectors {
            return Err(SacdError::BadSector { lsn, why: chilpa_i18n::mark("outside the image").into() });
        }
        self.file.seek(SeekFrom::Start(lsn * self.layout.stride() + self.layout.offset()))?;
        self.file.read_exact(into)?;
        Ok(())
    }
}

/// Which layout the image was dumped in, from where the master table's
/// signature sits. `None` when it sits nowhere: then it is not a SACD, and
/// the search for the master table says so.
fn layout_of(file: &mut File, bytes: u64) -> Result<Option<Layout>, SacdError> {
    let mut found = None;
    for layout in [Layout::Logical, Layout::Physical] {
        for lsn in MASTER_TOC_LSN {
            let at = lsn * layout.stride() + layout.offset();
            if at + 8 > bytes {
                continue;
            }
            let mut signature = [0u8; 8];
            file.seek(SeekFrom::Start(at))?;
            file.read_exact(&mut signature)?;
            if &signature == b"SACDMTOC" {
                // A file that reads as both is one nobody can say which it
                // is, and guessing would read the wrong bytes as audio.
                if found.is_some_and(|other| other != layout) {
                    return Err(SacdError::MalformedToc(
                        chilpa_i18n::mark("the image reads as both a 2048 and a 2064-byte dump").into(),
                    ));
                }
                found = Some(layout);
                break;
            }
        }
    }
    Ok(found)
}

/// Opens a SACD image and reads its stereo area.
pub fn open(path: &Path) -> Result<SacdImage, SacdError> {
    let mut file = File::open(path)?;
    let bytes = file.metadata()?.len();
    let layout = layout_of(&mut file, bytes)?.unwrap_or(Layout::Logical);
    let sectors = bytes / layout.stride();
    if sectors <= MASTER_TOC_LSN[0] {
        return Err(SacdError::MalformedToc(chilpa_i18n::mark("the image is shorter than its master table").into()));
    }
    let mut image = SacdImage {
        file: BufReader::new(file),
        sectors,
        layout,
        area: SacdArea {
            tracks: Vec::new(),
            album: None,
            artist: None,
            encoding: Encoding::Dsd3In14,
            audio_start_lsn: 0,
            audio_end_lsn: 0,
        },
    };

    let mut sector = vec![0u8; SECTOR];
    let (master_lsn, master) = MASTER_TOC_LSN
        .iter()
        .find_map(|&lsn| {
            image.read_sector(lsn, &mut sector).ok()?;
            (&sector[..8] == b"SACDMTOC").then(|| (lsn, sector.clone()))
        })
        .ok_or_else(|| SacdError::MalformedToc(chilpa_i18n::mark("SACDMTOC was not found").into()))?;

    let version = (master[8], master[9]);
    if version.0 != 1 {
        return Err(SacdError::Unsupported(format!("Master TOC version {}.{}", version.0, version.1)));
    }

    // Two area slots: stereo and multichannel. Every copy is tried.
    let slots = [
        (be32(&master, 0x40), be32(&master, 0x44), be16(&master, 0x54)),
        (be32(&master, 0x48), be32(&master, 0x4c), be16(&master, 0x56)),
    ];
    let mut area_toc = None;
    for (primary, backup, size) in slots {
        if size == 0 || size > MAX_AREA_TOC_SECTORS {
            continue;
        }
        for start in [primary, backup].into_iter().filter(|s| *s != 0) {
            let mut toc = vec![0u8; SECTOR * size as usize];
            let mut ok = true;
            for (i, chunk) in toc.chunks_mut(SECTOR).enumerate() {
                if image.read_sector(u64::from(start) + i as u64, chunk).is_err() {
                    ok = false;
                    break;
                }
            }
            if ok && &toc[..8] == b"TWOCHTOC" {
                area_toc = Some(toc);
                break;
            }
        }
        if area_toc.is_some() {
            break;
        }
    }
    let toc = area_toc
        .ok_or_else(|| SacdError::MalformedToc(chilpa_i18n::mark("the stereo area (TWOCHTOC) was not found").into()))?;

    image.area = parse_area(&toc, image.sectors)?;
    // The text is optional: a disc whose text cannot be read still plays,
    // and is named after its file instead.
    if master[0x80] != 0 {
        let mut text = vec![0u8; SECTOR];
        if image.read_sector(master_lsn + 1, &mut text).is_ok() && &text[..8] == b"SACDText" {
            let charset = master[0x8a];
            image.area.album = master_string(&text, 0x10, charset);
            image.area.artist = master_string(&text, 0x12, charset);
        }
    }
    Ok(image)
}

/// A string of the master text sector, through the pointer at `at`. The
/// pointer is relative to the sector, and zero means there is none -- zero
/// would point at the sector's own signature.
fn master_string(text: &[u8], at: usize, charset: u8) -> Option<String> {
    let start = usize::from(be16(text, at));
    if start == 0 || start >= text.len() {
        return None;
    }
    let end = text[start..].iter().position(|b| *b == 0)? + start;
    decode(&text[start..end], charset)
}

/// Each track's title and performer, from the area's SACDTTxt block.
///
/// The block starts with a table of pointers, one per track and relative to
/// the block. Each points at a record: how many items it holds, then, four
/// bytes on, each item as a kind byte, a spare byte and a string ended by
/// zeros. Kind 1 is the title and kind 2 the performer.
fn track_text(block: &[u8], track: usize, charset: u8) -> (Option<String>, Option<String>) {
    let (mut title, mut performer) = (None, None);
    let Some(pointer) = block.get(8 + track * 2..10 + track * 2) else { return (None, None) };
    let record = usize::from(u16::from_be_bytes([pointer[0], pointer[1]]));
    let Some(&items) = block.get(record).filter(|_| record != 0) else { return (None, None) };
    let mut at = record + 4;
    for _ in 0..items {
        let Some(&kind) = block.get(at) else { break };
        at += 2;
        let Some(len) = block.get(at..).and_then(|rest| rest.iter().position(|b| *b == 0)) else {
            break;
        };
        match kind {
            1 => title = decode(&block[at..at + len], charset),
            2 => performer = decode(&block[at..at + len], charset),
            _ => {}
        }
        at += len;
        while block.get(at) == Some(&0) {
            at += 1;
        }
    }
    (title, performer)
}

/// One string of the disc's text, in the character set the disc declares.
///
/// A byte the declared set cannot read drops that one string rather than
/// turning into a replacement character: a name with a hole in it is worse
/// than the file name it falls back to. An undeclared set is not a licence to
/// guess.
fn decode(raw: &[u8], charset: u8) -> Option<String> {
    use encoding_rs::{BIG5, EUC_KR, GBK, SHIFT_JIS};
    let strict = |encoding: &'static encoding_rs::Encoding| {
        encoding.decode_without_bom_handling_and_without_replacement(raw).map(|s| s.into_owned())
    };
    let text = match charset & 0x07 {
        1 => raw.is_ascii().then(|| String::from_utf8_lossy(raw).into_owned()),
        // ISO-8859-1 is every byte as the character of the same number, and
        // code 7 without escape sequences is the same.
        2 => Some(raw.iter().map(|b| char::from(*b)).collect()),
        7 if !raw.contains(&0x1b) => Some(raw.iter().map(|b| char::from(*b)).collect()),
        3 => strict(SHIFT_JIS),
        4 => strict(EUC_KR),
        5 => strict(GBK),
        6 => strict(BIG5),
        _ => None,
    }?;
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn parse_area(toc: &[u8], sectors: u64) -> Result<SacdArea, SacdError> {
    let channels = toc[0x20];
    if channels != 2 || toc[0x21] & 0x1f != 0 {
        return Err(SacdError::Unsupported(format!("{channels} channels in the stereo area")));
    }
    if toc[0x14] != 4 {
        return Err(SacdError::Unsupported(format!("rate code {}", toc[0x14])));
    }
    let encoding = match toc[0x15] & 0x0f {
        0 => Encoding::Dst,
        2 => Encoding::Dsd3In14,
        3 => Encoding::Dsd3In16,
        other => return Err(SacdError::Unsupported(format!("frame format {other}"))),
    };
    let track_count = toc[0x45] as usize;
    if track_count == 0 {
        return Err(SacdError::MalformedToc(chilpa_i18n::mark("the area declares no tracks").into()));
    }
    let audio_start_lsn = be32(toc, 0x48);
    let audio_end_lsn = be32(toc, 0x4c);
    if audio_start_lsn > audio_end_lsn || u64::from(audio_end_lsn) >= sectors {
        return Err(SacdError::MalformedToc(chilpa_i18n::mark("the audio area falls outside the image").into()));
    }

    let trl1 = find_block(toc, b"SACDTRL1")
        .ok_or_else(|| SacdError::MalformedToc(chilpa_i18n::mark("SACDTRL1 is missing").into()))?;
    let trl2 = find_block(toc, b"SACDTRL2")
        .ok_or_else(|| SacdError::MalformedToc(chilpa_i18n::mark("SACDTRL2 is missing").into()))?;
    // Each block carries two arrays of 255 four-byte entries.
    const SECOND_ARRAY: usize = 8 + 255 * 4;

    // The titles, when the area carries them. A pointer to the block is
    // honoured when there is one; otherwise it is looked for.
    let text_at = usize::from(be16(toc, 0x80));
    let charset = toc[0x5a];
    let titles = if text_at != 0 {
        toc.get(text_at * SECTOR..).filter(|b| b.len() >= 8 && &b[..8] == b"SACDTTxt")
    } else {
        find_block(toc, b"SACDTTxt")
    };

    let mut tracks = Vec::with_capacity(track_count);
    let mut previous_end: Option<u32> = None;
    for i in 0..track_count {
        let start_lsn = be32(trl1, 8 + i * 4);
        let length_lsn = be32(trl1, SECOND_ARRAY + i * 4);
        let duration_frames = timecode_frames(trl2, SECOND_ARRAY + i * 4);
        if trl2[8 + i * 4 + 1] >= 60
            || trl2[8 + i * 4 + 2] >= 75
            || trl2[SECOND_ARRAY + i * 4 + 1] >= 60
            || trl2[SECOND_ARRAY + i * 4 + 2] >= 75
        {
            return Err(SacdError::MalformedToc(format!("invalid timecode in track {}", i + 1)));
        }
        let end = u64::from(start_lsn) + u64::from(length_lsn);
        if length_lsn == 0
            || start_lsn < audio_start_lsn
            || end > u64::from(audio_end_lsn) + 1
            || previous_end.is_some_and(|previous| start_lsn <= previous)
        {
            return Err(SacdError::MalformedToc(format!(
                "track {} falls outside the area, or out of order",
                i + 1
            )));
        }
        previous_end = Some(start_lsn);
        let (title, performer) = titles.map(|block| track_text(block, i, charset)).unwrap_or_default();
        tracks.push(SacdTrack {
            number: (i + 1) as u8,
            start_lsn,
            length_lsn,
            duration_frames,
            encoding,
            title,
            performer,
        });
    }

    Ok(SacdArea { tracks, album: None, artist: None, encoding, audio_start_lsn, audio_end_lsn })
}

/// Looks for a block with that signature inside the area's table.
/// A block of the area table, from the sector it starts at to the end of
/// the table. A block can span sectors, and the text block does: its
/// pointers reach past its first sector, 4096 bytes in on "Das Rheingold".
fn find_block<'a>(toc: &'a [u8], signature: &[u8; 8]) -> Option<&'a [u8]> {
    let at = toc.chunks(SECTOR).position(|s| s.len() >= 8 && &s[..8] == signature)?;
    toc.get(at * SECTOR..)
}
