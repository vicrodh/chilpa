//! DSDIFF, Philips' container.
//!
//! Chunks with big-endian sizes, each padded to an even length: `FRM8` with
//! the form type `DSD `; inside it a `PROP` chunk of type `SND ` whose own
//! chunks give the rate (`FS  `), the channels (`CHNL`) and the compression
//! (`CMPR`); and a `DSD ` chunk with the samples, one byte of each channel in
//! turn, the earliest bit in the most significant position. Unlike DSF there
//! is no padding at the end of the music and no bit order to reverse.
//!
//! A DSDIFF can carry DST instead (`CMPR` "DST ", the frames in a `DST `
//! chunk). QBZ refused those, and none of the files this was written against
//! are one; it is refused with the reason, not read as noise.

use crate::{is_supported_rate, DsdError, DsdInfo, DsdSource};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes of each channel handed out at a time.
const BLOCK: usize = 4096;

pub struct DffReader {
    file: BufReader<File>,
    info: DsdInfo,
    /// Where the samples start, and how many bytes of them there are, all
    /// channels together.
    data_start: u64,
    data_len: u64,
    /// Bytes of samples left, all channels together.
    remaining: u64,
}

impl DffReader {
    pub fn open(path: &Path) -> Result<Self, DsdError> {
        let mut file = BufReader::new(File::open(path)?);
        if &read_id(&mut file)? != b"FRM8" {
            return Err(DsdError::Corrupt(chilpa_i18n::mark("it does not start with the FRM8 chunk").into()));
        }
        let form_len = read_u64(&mut file)?;
        let form_end = 12u64.checked_add(form_len).filter(|_| form_len >= 4).ok_or_else(invalid_chunk)?;
        let file_len = file.get_ref().metadata()?.len();
        if &read_id(&mut file)? != b"DSD " {
            return Err(DsdError::Corrupt(chilpa_i18n::mark("the FRM8 chunk does not hold DSD").into()));
        }

        let (mut rate, mut channels, mut data) = (None, None, None);
        while let Some(chunk) = next_chunk(&mut file, form_end)? {
            if &chunk.id != b"DSD " && chunk.next > file_len {
                return Err(invalid_chunk());
            }
            match &chunk.id {
                b"PROP" => {
                    if chunk.len < 4 {
                        return Err(invalid_chunk());
                    }
                    if &read_id(&mut file)? != b"SND " {
                        return Err(DsdError::Corrupt(chilpa_i18n::mark("the PROP chunk is not of sound").into()));
                    }
                    while let Some(sub) = next_chunk(&mut file, chunk.end)? {
                        match &sub.id {
                            b"FS  " => {
                                if sub.len < 4 {
                                    return Err(invalid_chunk());
                                }
                                rate = Some(read_u32(&mut file)?);
                            }
                            b"CHNL" => {
                                if sub.len < 2 {
                                    return Err(invalid_chunk());
                                }
                                channels = Some(read_u16(&mut file)?);
                            }
                            b"CMPR" => {
                                if sub.len < 4 {
                                    return Err(invalid_chunk());
                                }
                                match &read_id(&mut file)? {
                                    b"DSD " => {}
                                    b"DST " => {
                                        return Err(DsdError::Unsupported(
                                            chilpa_i18n::mark("DSDIFF compressed with DST").into(),
                                        ))
                                    }
                                    other => {
                                        return Err(DsdError::Unsupported(format!(
                                            "DSDIFF compression {}",
                                            String::from_utf8_lossy(other)
                                        )))
                                    }
                                }
                            }
                            _ => {}
                        }
                        file.seek(SeekFrom::Start(sub.next))?;
                    }
                }
                b"DSD " => {
                    data = Some((chunk.start, chunk.len));
                    // Keep missing audio as a next_block error, not an open error.
                    if chunk.next > file_len {
                        break;
                    }
                }
                b"DST " => {
                    return Err(DsdError::Unsupported(chilpa_i18n::mark("DSDIFF compressed with DST").into()))
                }
                _ => {}
            }
            file.seek(SeekFrom::Start(chunk.next))?;
        }

        let rate = rate.ok_or_else(|| DsdError::Corrupt(chilpa_i18n::mark("the FS chunk is missing").into()))?;
        let channels =
            channels.ok_or_else(|| DsdError::Corrupt(chilpa_i18n::mark("the CHNL chunk is missing").into()))?;
        let (data_start, data_len) =
            data.ok_or_else(|| DsdError::Corrupt(chilpa_i18n::mark("the DSD chunk is missing").into()))?;
        if !is_supported_rate(rate) {
            return Err(DsdError::Unsupported(format!("{rate} Hz")));
        }
        if !(1..=6).contains(&channels) {
            return Err(DsdError::Unsupported(format!("{channels} channels")));
        }
        // Whole rounds of one byte per channel; a stray byte at the end is
        // not a sample of every channel.
        let data_len = data_len - data_len % u64::from(channels);
        let samples = (data_len / u64::from(channels)).checked_mul(8).ok_or_else(invalid_chunk)?;
        file.seek(SeekFrom::Start(data_start))?;
        Ok(Self {
            file,
            info: DsdInfo { dsd_rate_hz: rate, channels, samples },
            data_start,
            data_len,
            remaining: data_len,
        })
    }
}

impl DsdSource for DffReader {
    fn info(&self) -> &DsdInfo {
        &self.info
    }

    fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, DsdError> {
        let channels = usize::from(self.info.channels);
        let take = (BLOCK * channels).min(self.remaining as usize);
        if take == 0 {
            return Ok(None);
        }
        let mut interleaved = vec![0u8; take];
        // EOF inside the declared audio chunk is a read error. Returning
        // None here would advance the queue as if all the music had played.
        self.file.read_exact(&mut interleaved)?;
        self.remaining -= take as u64;
        let mut out = vec![Vec::with_capacity(take / channels); channels];
        for round in interleaved.chunks_exact(channels) {
            for (channel, byte) in out.iter_mut().zip(round) {
                channel.push(*byte);
            }
        }
        Ok(Some(out))
    }

    /// Lands on the byte the sample is in: the samples are not in blocks, so
    /// any byte of a channel can be reached.
    fn seek(&mut self, sample: u64) -> Result<Option<u64>, DsdError> {
        let channels = u64::from(self.info.channels);
        let byte = (sample / 8).min(self.data_len / channels);
        self.file.seek(SeekFrom::Start(self.data_start + byte * channels))?;
        self.remaining = self.data_len - byte * channels;
        Ok(Some(byte * 8))
    }
}

struct Chunk {
    id: [u8; 4],
    len: u64,
    start: u64,
    end: u64,
    next: u64,
}

fn invalid_chunk() -> DsdError {
    DsdError::Corrupt(chilpa_i18n::mark("invalid DSDIFF chunk bounds").into())
}

/// Read a complete header and bound its payload and padding to its container.
fn next_chunk(file: &mut BufReader<std::fs::File>, limit: u64) -> Result<Option<Chunk>, DsdError> {
    let position = file.stream_position()?;
    if position == limit {
        return Ok(None);
    }
    let start = position.checked_add(12).filter(|end| *end <= limit).ok_or_else(invalid_chunk)?;
    let mut head = [0u8; 12];
    file.read_exact(&mut head)?;
    let id = head[..4].try_into().unwrap();
    let len = u64::from_be_bytes(head[4..].try_into().unwrap());
    let end = start.checked_add(len).ok_or_else(invalid_chunk)?;
    let next = end.checked_add(len & 1).filter(|end| *end <= limit && *end > position)
        .ok_or_else(invalid_chunk)?;
    Ok(Some(Chunk { id, len, start, end, next }))
}

fn read_id(f: &mut impl Read) -> Result<[u8; 4], DsdError> {
    let mut id = [0u8; 4];
    f.read_exact(&mut id)?;
    Ok(id)
}

fn read_u16(f: &mut impl Read) -> Result<u16, DsdError> {
    let mut b = [0u8; 2];
    f.read_exact(&mut b)?;
    Ok(u16::from_be_bytes(b))
}

fn read_u32(f: &mut impl Read) -> Result<u32, DsdError> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}

fn read_u64(f: &mut impl Read) -> Result<u64, DsdError> {
    let mut b = [0u8; 8];
    f.read_exact(&mut b)?;
    Ok(u64::from_be_bytes(b))
}
