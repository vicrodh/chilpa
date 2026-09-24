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
    pub encoding: Encoding,
    /// First and last sector of the audio area.
    pub audio_start_lsn: u32,
    pub audio_end_lsn: u32,
}

/// The open image, with its stereo area already read.
pub struct SacdImage {
    pub(crate) file: BufReader<File>,
    pub(crate) sectors: u64,
    pub area: SacdArea,
}

impl SacdImage {
    pub(crate) fn read_sector(&mut self, lsn: u64, into: &mut [u8]) -> Result<(), SacdError> {
        if lsn >= self.sectors {
            return Err(SacdError::BadSector { lsn, why: chilpa_i18n::mark("outside the image").into() });
        }
        self.file.seek(SeekFrom::Start(lsn * SECTOR as u64))?;
        self.file.read_exact(into)?;
        Ok(())
    }
}

/// Opens a SACD image and reads its stereo area.
pub fn open(path: &Path) -> Result<SacdImage, SacdError> {
    let file = File::open(path)?;
    let sectors = file.metadata()?.len() / SECTOR as u64;
    if sectors <= MASTER_TOC_LSN[0] {
        return Err(SacdError::MalformedToc(chilpa_i18n::mark("the image is shorter than its master table").into()));
    }
    let mut image = SacdImage {
        file: BufReader::new(file),
        sectors,
        area: SacdArea {
            tracks: Vec::new(),
            encoding: Encoding::Dsd3In14,
            audio_start_lsn: 0,
            audio_end_lsn: 0,
        },
    };

    let mut sector = vec![0u8; SECTOR];
    let master = MASTER_TOC_LSN
        .iter()
        .find_map(|&lsn| {
            image.read_sector(lsn, &mut sector).ok()?;
            (&sector[..8] == b"SACDMTOC").then(|| sector.clone())
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
    Ok(image)
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
        tracks.push(SacdTrack {
            number: (i + 1) as u8,
            start_lsn,
            length_lsn,
            duration_frames,
            encoding,
        });
    }

    Ok(SacdArea { tracks, encoding, audio_start_lsn, audio_end_lsn })
}

/// Looks for a block with that signature inside the area's table.
fn find_block<'a>(toc: &'a [u8], signature: &[u8; 8]) -> Option<&'a [u8]> {
    toc.chunks(SECTOR).find(|s| s.len() >= 8 && &s[..8] == signature)
}
