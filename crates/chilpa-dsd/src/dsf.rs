//! DSF, Sony's container.
//!
//! The layout is simple: a `DSD ` chunk with the pointer to the tags, a
//! `fmt ` chunk describing the stream, and a `data` chunk with the bits.
//! The data comes in BLOCKS per channel (4096 bytes each, one after
//! another), and the last block may carry padding that is not music, so the
//! valid bytes are counted from the sample count.
//!
//! `bits_per_sample` does not say how wide a sample is (always one bit): it
//! says the order inside the byte. 1 = the earliest one goes in the least
//! significant bit, which is the usual case; 8 = the other way round.

use crate::{is_supported_rate, reverse_bits, DsdError, DsdInfo, DsdSource};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub struct DsfReader {
    file: BufReader<File>,
    info: DsdInfo,
    block_size: usize,
    /// Valid bytes left, per channel.
    remaining_per_channel: u64,
    /// Where the data starts. Not assumed: taken from the real position after
    /// reading the header, because the fmt chunk may carry more fields than we
    /// read.
    data_start: u64,
    lsb_first: bool,
}

impl DsfReader {
    pub fn open(path: &Path) -> Result<Self, DsdError> {
        let mut file = BufReader::new(File::open(path)?);

        if &read_id(&mut file)? != b"DSD " {
            return Err(DsdError::Corrupt("no empieza con el chunk DSD".into()));
        }
        let dsd_chunk = read_u64(&mut file)?;
        if dsd_chunk != 28 {
            return Err(DsdError::Corrupt(format!("el chunk DSD mide {dsd_chunk}, no 28")));
        }
        let _total_size = read_u64(&mut file)?;
        let _metadata_ptr = read_u64(&mut file)?;

        if &read_id(&mut file)? != b"fmt " {
            return Err(DsdError::Corrupt("falta el chunk fmt".into()));
        }
        let fmt_size = read_u64(&mut file)?;
        if fmt_size < 52 {
            return Err(DsdError::Corrupt(format!("el chunk fmt mide {fmt_size}, mínimo 52")));
        }
        let version = read_u32(&mut file)?;
        let format_id = read_u32(&mut file)?;
        let _channel_type = read_u32(&mut file)?;
        let channels = read_u32(&mut file)?;
        let dsd_rate_hz = read_u32(&mut file)?;
        let bits_per_sample = read_u32(&mut file)?;
        let samples = read_u64(&mut file)?;
        let block_size = read_u32(&mut file)?;
        let _reserved = read_u32(&mut file)?;

        if version != 1 || format_id != 0 {
            return Err(DsdError::Unsupported(format!("DSF versión {version}, id {format_id}")));
        }
        if !(1..=6).contains(&channels) {
            return Err(DsdError::Unsupported(format!("{channels} canales")));
        }
        if !is_supported_rate(dsd_rate_hz) {
            return Err(DsdError::Unsupported(format!("{dsd_rate_hz} Hz")));
        }
        let lsb_first = match bits_per_sample {
            1 => true,
            8 => false,
            other => return Err(DsdError::Corrupt(format!("bits_per_sample {other}"))),
        };
        if block_size == 0 || block_size > (1 << 20) {
            return Err(DsdError::Corrupt(format!("bloque de {block_size} bytes")));
        }

        if &read_id(&mut file)? != b"data" {
            return Err(DsdError::Corrupt("falta el chunk data".into()));
        }
        let _data_size = read_u64(&mut file)?;
        let data_start = file.stream_position()?;

        Ok(Self {
            file,
            block_size: block_size as usize,
            remaining_per_channel: samples.div_ceil(8),
            data_start,
            lsb_first,
            info: DsdInfo { dsd_rate_hz, channels: channels as u16, samples },
        })
    }

    /// Lands on the requested bit, aligned to the container's block. Returns the
    /// bit it actually landed on, which may be earlier than the one asked for.
    pub fn seek_to_sample(&mut self, sample: u64) -> Result<u64, DsdError> {
        let total = self.info.samples.div_ceil(8);
        let byte = (sample / 8).min(total);
        let block = byte / self.block_size as u64;
        let byte_aligned = block * self.block_size as u64;
        let skip = self.data_start + byte_aligned * u64::from(self.info.channels);
        self.file.seek(SeekFrom::Start(skip))?;
        self.remaining_per_channel = total - byte_aligned;
        Ok(byte_aligned * 8)
    }
}

impl DsdSource for DsfReader {
    fn info(&self) -> &DsdInfo {
        &self.info
    }

    fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, DsdError> {
        if self.remaining_per_channel == 0 {
            return Ok(None);
        }
        let valid = self.block_size.min(self.remaining_per_channel as usize);
        let mut channels = Vec::with_capacity(self.info.channels as usize);
        for _ in 0..self.info.channels {
            let mut block = vec![0u8; self.block_size];
            match self.file.read_exact(&mut block) {
                Ok(()) => {}
                // A truncated file is not an error: what is there is what is handed over.
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.remaining_per_channel = 0;
                    return Ok(None);
                }
                Err(e) => return Err(DsdError::Open(e)),
            }
            block.truncate(valid);
            if self.lsb_first {
                for b in &mut block {
                    *b = reverse_bits(*b);
                }
            }
            channels.push(block);
        }
        self.remaining_per_channel -= valid as u64;
        Ok(Some(channels))
    }
}

fn read_id(f: &mut impl Read) -> Result<[u8; 4], DsdError> {
    let mut id = [0u8; 4];
    f.read_exact(&mut id)?;
    Ok(id)
}

fn read_u32(f: &mut impl Read) -> Result<u32, DsdError> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(f: &mut impl Read) -> Result<u64, DsdError> {
    let mut b = [0u8; 8];
    f.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}
