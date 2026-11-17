//! Raw AC-3 / E-AC-3 elementary-stream container (`.ac3`, `.eac3`,
//! `.ec3`): a plain concatenation of syncframes with no framing of its
//! own.
//!
//! The demuxer walks the stream syncframe by syncframe and emits **one
//! packet per presentation period**:
//!
//! * AC-3 (`bsid ≤ 10`, §5.3.1 syncinfo): one syncframe = one packet of
//!   1536 samples; the length comes from `frmsizecod` / `fscod`
//!   (Table 5.18).
//! * E-AC-3 (`bsid` 11..=16, Annex E §E.2.3.1): the independent
//!   substream of program 0 (`strmtyp` 0 or 2, `substreamid` 0) opens a
//!   group and every following syncframe up to the next such frame —
//!   dependent substreams (`strmtyp` 1) and additional programs
//!   (`substreamid` > 0) — rides in the same packet, which is what
//!   [`crate::eac3::decode_eac3_packet`] consumes. The length is
//!   `(frmsiz + 1) · 2` bytes and the duration `numblkscod` blocks of
//!   256 samples (Table E2.4; 6 blocks when `fscod == 3` selects a
//!   reduced `fscod2` rate).
//!
//! Timestamps count samples (`TimeBase(1, sample_rate)`); the stream
//! duration is the constant-rate estimate `frames × samples_per_frame`
//! taken from the first packet's byte length, and seeking lands on the
//! packet boundary nearest `byte_offset = pts / samples_per_frame ×
//! first_packet_bytes`, resynchronised on the next program-0 syncframe.
//!
//! [`probe`] requires the 0x0B77 syncword plus a valid header at offset
//! 0 **and** at the frame boundary the first header announces.

use std::io::{Seek, SeekFrom};

use oxideav_core::{
    CodecId, CodecParameters, CodecResolver, ContainerRegistry, Demuxer, Error, Packet, ProbeData,
    ProbeScore, ReadSeek, Result, StreamInfo, TimeBase,
};

use crate::syncinfo::{self, SYNCWORD};

/// Container name registered with the [`ContainerRegistry`].
pub const FORMAT_NAME: &str = "ac3";

/// The file extensions that map to this container.
pub const EXTENSIONS: [&str; 3] = ["ac3", "eac3", "ec3"];

/// Minimal syncframe header: what the first six bytes of a syncframe
/// say about its length, rate and role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyncFrame {
    /// Syncframe length in bytes.
    pub len: usize,
    /// Sample rate in Hz (reduced E-AC-3 rates included).
    pub sample_rate: u32,
    /// Samples per channel in this syncframe (`blocks × 256`).
    pub samples: u32,
    /// `true` for Annex E (`bsid` 11..=16), `false` for base AC-3.
    pub eac3: bool,
    /// `true` when this frame opens a presentation period: every AC-3
    /// frame, and an E-AC-3 independent / AC-3-convert substream with
    /// `substreamid == 0`.
    pub program_start: bool,
    /// E-AC-3 `strmtyp` (0 for AC-3).
    pub strmtyp: u8,
    /// E-AC-3 `substreamid` (0 for AC-3).
    pub substreamid: u8,
}

/// Parse the syncframe header at the start of `data` (needs at least
/// six bytes). Returns `None` when there is no syncword, the `bsid` is
/// outside both syntaxes, or a rate / size code is reserved.
pub fn parse_sync_frame(data: &[u8]) -> Option<SyncFrame> {
    if data.len() < 6 || u16::from_be_bytes([data[0], data[1]]) != SYNCWORD {
        return None;
    }
    // §5.4.2.1 / §E.2.3.1.6: bsid is the top five bits of byte 5 in
    // both syntaxes.
    let bsid = data[5] >> 3;
    if bsid <= crate::bsi::MAX_BSID_BASE {
        let si = syncinfo::parse(data).ok()?;
        return Some(SyncFrame {
            len: si.frame_length as usize,
            sample_rate: si.sample_rate,
            samples: 1536,
            eac3: false,
            program_start: true,
            strmtyp: 0,
            substreamid: 0,
        });
    }
    if !(11..=16).contains(&bsid) {
        return None;
    }
    // §E.2.3.1: strmtyp(2) substreamid(3) frmsiz(11) fscod(2)
    // fscod2|numblkscod(2) acmod(3) lfeon(1).
    let strmtyp = data[2] >> 6;
    if strmtyp == 3 {
        return None;
    }
    let substreamid = (data[2] >> 3) & 0x07;
    let frmsiz = (u16::from(data[2] & 0x07) << 8) | u16::from(data[3]);
    let fscod = data[4] >> 6;
    let code2 = (data[4] >> 4) & 0x03;
    let (sample_rate, blocks) = if fscod == 3 {
        // Table E2.3 reduced rates: fscod2 selects half the Table 5.6
        // rate; frames are always six blocks.
        let base = crate::tables::sample_rate_hz(code2)?;
        (base / 2, 6u32)
    } else {
        let blocks = match code2 {
            0 => 1,
            1 => 2,
            2 => 3,
            _ => 6,
        };
        (crate::tables::sample_rate_hz(fscod)?, blocks)
    };
    Some(SyncFrame {
        len: (usize::from(frmsiz) + 1) * 2,
        sample_rate,
        samples: blocks * 256,
        eac3: true,
        program_start: strmtyp != 1 && substreamid == 0,
        strmtyp,
        substreamid,
    })
}

/// Content probe: 0x0B77 and a valid header at offset 0, and another at
/// the boundary the first header announces (when the buffer reaches
/// it) with the same sample rate.
pub fn probe(p: &ProbeData) -> ProbeScore {
    let Some(first) = parse_sync_frame(p.buf) else {
        return 0;
    };
    let ext_matches = p.ext.is_some_and(|e| EXTENSIONS.contains(&e));
    if p.buf.len() >= first.len + 6 {
        match parse_sync_frame(&p.buf[first.len..]) {
            Some(second) if second.sample_rate == first.sample_rate => {
                if ext_matches {
                    100
                } else {
                    90
                }
            }
            _ => 0,
        }
    } else if ext_matches {
        90
    } else {
        40
    }
}

/// Install the demuxer, the extensions and the probe into a
/// [`ContainerRegistry`]. Called by [`crate::register`].
pub fn register_container(reg: &mut ContainerRegistry) {
    reg.register_demuxer(FORMAT_NAME, open_demuxer);
    for ext in EXTENSIONS {
        reg.register_extension(ext, FORMAT_NAME);
    }
    reg.register_probe(FORMAT_NAME, probe);
}

fn open_demuxer(input: Box<dyn ReadSeek>, _codecs: &dyn CodecResolver) -> Result<Box<dyn Demuxer>> {
    Ok(Box::new(Ac3Demuxer::open(input)?))
}

/// Largest syncframe either syntax can declare (E-AC-3 `frmsiz` = 2047
/// → 4096 bytes; AC-3 tops out at 3840).
const MAX_FRAME_BYTES: usize = 4096;

/// The AC-3 / E-AC-3 elementary-stream demuxer.
pub struct Ac3Demuxer {
    input: Box<dyn ReadSeek>,
    streams: Vec<StreamInfo>,
    /// Byte offset of the first program-start syncframe.
    first_frame_offset: u64,
    /// Total input length in bytes.
    total_len: u64,
    /// Byte length of the first packet (its whole substream group) —
    /// the constant-rate unit for duration and seeking.
    first_packet_bytes: u64,
    cursor: u64,
    next_pts: i64,
    samples_per_frame: u32,
    sample_rate: u32,
    finished: bool,
}

impl Ac3Demuxer {
    /// Open an elementary stream.
    pub fn open(mut input: Box<dyn ReadSeek>) -> Result<Self> {
        let total_len = input.seek(SeekFrom::End(0))?;
        input.seek(SeekFrom::Start(0))?;
        let mut head = vec![0u8; (64 * 1024).min(total_len as usize)];
        read_up_to(input.as_mut(), &mut head)?;

        // First program-start syncframe (skip leading junk and orphan
        // dependent frames).
        let mut off = 0usize;
        let first = loop {
            if off + 6 > head.len() {
                return Err(Error::invalid(
                    "ac3: no AC-3 / E-AC-3 syncframe in the first 64 KiB",
                ));
            }
            match parse_sync_frame(&head[off..]) {
                Some(f) if f.program_start && off + f.len <= head.len() => break (off, f),
                Some(f) if !f.program_start => off += f.len,
                _ => off += 1,
            }
        };
        let (first_frame_offset, frame) = first;
        let group_len = group_len(&head[first_frame_offset..]);
        let group = &head[first_frame_offset..first_frame_offset + group_len];

        // Channel count: the base AC-3 BSI, or the E-AC-3 independent
        // substream plus every dependent substream spliced onto it.
        let (codec, channels) = if frame.eac3 {
            let mut chans = 0u16;
            let mut at = 0usize;
            while let Some(f) = parse_sync_frame(&group[at..]) {
                // The Annex E parser starts just past the syncword.
                if f.substreamid == 0
                    && let Ok(bsi) = crate::eac3::bsi::parse(&group[at + 2..at + f.len])
                {
                    chans += u16::from(bsi.nchans);
                }
                at += f.len;
                if at + 6 > group.len() {
                    break;
                }
            }
            (CodecId::new(crate::CODEC_ID_STR_EAC3), chans.max(1))
        } else {
            let bsi = crate::bsi::parse(&group[5..])?;
            (CodecId::new(crate::CODEC_ID_STR), u16::from(bsi.nchans))
        };

        let sample_rate = frame.sample_rate;
        let samples_per_frame = frame.samples;
        let first_packet_bytes = group_len as u64;
        let audio_bytes = total_len.saturating_sub(first_frame_offset as u64);
        let frames = audio_bytes / first_packet_bytes;
        let duration = Some((frames as i64).saturating_mul(i64::from(samples_per_frame)));

        let mut params = CodecParameters::audio(codec);
        params.sample_rate = Some(sample_rate);
        params.channels = Some(channels);
        params.bit_rate = Some(
            (first_packet_bytes as u64 * 8 * u64::from(sample_rate)) / u64::from(samples_per_frame),
        );
        let time_base = TimeBase::new(1, i64::from(sample_rate));
        let stream = StreamInfo {
            index: 0,
            time_base,
            duration,
            start_time: Some(0),
            params,
        };
        Ok(Self {
            input,
            streams: vec![stream],
            first_frame_offset: first_frame_offset as u64,
            total_len,
            first_packet_bytes,
            cursor: first_frame_offset as u64,
            next_pts: 0,
            samples_per_frame,
            sample_rate,
            finished: false,
        })
    }

    /// Read the header at `self.cursor`; `None` at end of input.
    fn header_at_cursor(&mut self) -> Result<Option<[u8; 6]>> {
        if self.cursor + 6 > self.total_len {
            return Ok(None);
        }
        self.input.seek(SeekFrom::Start(self.cursor))?;
        let mut hdr = [0u8; 6];
        if read_up_to(self.input.as_mut(), &mut hdr)? < 6 {
            return Ok(None);
        }
        Ok(Some(hdr))
    }
}

/// Byte length of the substream group starting at `data[0]` (a
/// program-start syncframe): that frame plus every following frame
/// that is not itself a program start. Stops at the end of `data`.
fn group_len(data: &[u8]) -> usize {
    let Some(first) = parse_sync_frame(data) else {
        return 0;
    };
    let mut len = first.len;
    while len + 6 <= data.len() {
        match parse_sync_frame(&data[len..]) {
            Some(f) if !f.program_start && f.eac3 && len + f.len <= data.len() => len += f.len,
            _ => break,
        }
    }
    len
}

fn read_up_to(r: &mut dyn ReadSeek, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0usize;
    while filled < buf.len() {
        let n = r.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

impl Demuxer for Ac3Demuxer {
    fn format_name(&self) -> &str {
        FORMAT_NAME
    }

    fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    fn next_packet(&mut self) -> Result<Packet> {
        if self.finished {
            return Err(Error::Eof);
        }
        loop {
            let Some(hdr) = self.header_at_cursor()? else {
                self.finished = true;
                return Err(Error::Eof);
            };
            let frame = match parse_sync_frame(&hdr) {
                Some(f) if f.sample_rate == self.sample_rate => f,
                Some(_) | None => {
                    // Junk or a foreign-rate frame: resynchronise.
                    self.cursor += 1;
                    continue;
                }
            };
            if self.cursor + frame.len as u64 > self.total_len {
                self.finished = true;
                return Err(Error::Eof);
            }
            if !frame.program_start {
                // Orphan dependent / additional-program frame (after a
                // seek, or a stream that opens mid-group): skip it.
                self.cursor += frame.len as u64;
                continue;
            }
            // Read the program-start frame plus its followers.
            let mut data = Vec::with_capacity(frame.len + MAX_FRAME_BYTES);
            data.resize(frame.len, 0);
            data[..6].copy_from_slice(&hdr);
            self.input.seek(SeekFrom::Start(self.cursor + 6))?;
            if read_up_to(self.input.as_mut(), &mut data[6..])? + 6 < frame.len {
                self.finished = true;
                return Err(Error::Eof);
            }
            let mut end = self.cursor + frame.len as u64;
            if frame.eac3 {
                loop {
                    if end + 6 > self.total_len {
                        break;
                    }
                    self.input.seek(SeekFrom::Start(end))?;
                    let mut h = [0u8; 6];
                    if read_up_to(self.input.as_mut(), &mut h)? < 6 {
                        break;
                    }
                    match parse_sync_frame(&h) {
                        Some(f)
                            if f.eac3
                                && !f.program_start
                                && end + f.len as u64 <= self.total_len =>
                        {
                            let start = data.len();
                            data.resize(start + f.len, 0);
                            data[start..start + 6].copy_from_slice(&h);
                            if read_up_to(self.input.as_mut(), &mut data[start + 6..])? + 6 < f.len
                            {
                                data.truncate(start);
                                break;
                            }
                            end += f.len as u64;
                        }
                        _ => break,
                    }
                }
            }
            let pts = self.next_pts;
            let dur = i64::from(frame.samples);
            self.next_pts = self.next_pts.saturating_add(dur);
            self.cursor = end;
            return Ok(Packet::new(0, self.streams[0].time_base, data)
                .with_pts(pts)
                .with_dts(pts)
                .with_duration(dur)
                .with_keyframe(true));
        }
    }

    fn seek_to(&mut self, _stream_index: u32, pts: i64) -> Result<i64> {
        if pts < 0 {
            return Err(Error::invalid("ac3: negative seek PTS"));
        }
        let spf = i64::from(self.samples_per_frame);
        let total = self.streams[0].duration.unwrap_or(i64::MAX);
        let frame_idx = (pts.min(total) / spf).max(0);
        let offset = self
            .first_frame_offset
            .saturating_add((frame_idx as u64).saturating_mul(self.first_packet_bytes))
            .min(self.total_len);
        self.cursor = offset;
        self.next_pts = frame_idx.saturating_mul(spf);
        self.finished = false;
        Ok(self.next_pts)
    }

    fn duration_micros(&self) -> Option<i64> {
        let dur = self.streams[0].duration?;
        Some(((dur as i128) * 1_000_000 / i128::from(self.sample_rate)) as i64)
    }
}

/// Open a file as an [`Ac3Demuxer`] without going through a registry.
pub fn open_file_demuxer<P: AsRef<std::path::Path>>(path: P) -> Result<Ac3Demuxer> {
    let f = std::fs::File::open(path)?;
    Ac3Demuxer::open(Box::new(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/sine440_stereo.ac3");

    #[test]
    fn parse_ac3_header() {
        let f = parse_sync_frame(FIXTURE).expect("ac3 header");
        assert_eq!(
            f,
            SyncFrame {
                len: 768,
                sample_rate: 48_000,
                samples: 1536,
                eac3: false,
                program_start: true,
                strmtyp: 0,
                substreamid: 0,
            }
        );
        assert!(parse_sync_frame(&FIXTURE[1..]).is_none(), "no syncword");
        assert!(parse_sync_frame(&FIXTURE[..5]).is_none(), "too short");
    }

    #[test]
    fn parse_eac3_header_fields() {
        // strmtyp 1, substreamid 2, frmsiz 0x0BF (→ 384 bytes), fscod 0
        // (48 kHz), numblkscod 2 (3 blocks), bsid 16.
        // byte 2 = strmtyp 01 | substreamid 010 | frmsiz hi 000; byte 4 =
        // fscod 00 | numblkscod 10 | acmod 010 | lfeon 0.
        let hdr = [0x0B, 0x77, 0x50, 0xBF, 0x24, 16 << 3];
        let f = parse_sync_frame(&hdr).expect("eac3 header");
        assert_eq!(f.len, 384);
        assert_eq!(f.sample_rate, 48_000);
        assert_eq!(f.samples, 768);
        assert!(f.eac3);
        assert_eq!(f.strmtyp, 1);
        assert_eq!(f.substreamid, 2);
        assert!(!f.program_start);
        // fscod 3 + fscod2 1 → 22.05 kHz, six blocks.
        let hdr = [0x0B, 0x77, 0x00, 0xBF, 0xD4, 16 << 3]; // fscod 11, fscod2 01
        let f = parse_sync_frame(&hdr).expect("reduced-rate header");
        assert_eq!((f.sample_rate, f.samples), (22_050, 1536));
        assert!(f.program_start);
        // fscod 3 + fscod2 3 is reserved; strmtyp 3 is reserved.
        assert!(parse_sync_frame(&[0x0B, 0x77, 0x00, 0xBF, 0xF4, 16 << 3]).is_none());
        assert!(parse_sync_frame(&[0x0B, 0x77, 0xC0, 0xBF, 0x00, 16 << 3]).is_none());
        // bsid 17 belongs to neither syntax.
        assert!(parse_sync_frame(&[0x0B, 0x77, 0x00, 0xBF, 0x00, 17 << 3]).is_none());
    }

    #[test]
    fn probe_scores() {
        let two_frames = &FIXTURE[..768 + 6];
        assert_eq!(
            probe(&ProbeData {
                buf: two_frames,
                ext: Some("ac3")
            }),
            100
        );
        assert_eq!(
            probe(&ProbeData {
                buf: two_frames,
                ext: None
            }),
            90
        );
        // Only the first frame visible.
        assert_eq!(
            probe(&ProbeData {
                buf: &FIXTURE[..700],
                ext: Some("eac3")
            }),
            90
        );
        assert_eq!(
            probe(&ProbeData {
                buf: &FIXTURE[..700],
                ext: None
            }),
            40
        );
        // Second boundary does not carry a syncword → not this container.
        let mut broken = two_frames.to_vec();
        broken[768] = 0x00;
        assert_eq!(
            probe(&ProbeData {
                buf: &broken,
                ext: Some("ac3")
            }),
            0
        );
        assert_eq!(
            probe(&ProbeData {
                buf: b"RIFF....WAVE",
                ext: Some("ac3")
            }),
            0
        );
    }

    #[test]
    fn demuxes_one_packet_per_syncframe() {
        let mut d = Ac3Demuxer::open(Box::new(std::io::Cursor::new(FIXTURE.to_vec()))).unwrap();
        let s = &d.streams()[0];
        assert_eq!(s.params.codec_id, CodecId::new("ac3"));
        assert_eq!(s.params.sample_rate, Some(48_000));
        assert_eq!(s.params.channels, Some(2));
        assert_eq!(s.params.bit_rate, Some(192_000));
        assert_eq!(s.time_base, TimeBase::new(1, 48_000));
        let frames = FIXTURE.len() / 768;
        assert_eq!(s.duration, Some(frames as i64 * 1536));
        assert_eq!(
            d.duration_micros(),
            Some(frames as i64 * 1536 * 1_000_000 / 48_000)
        );
        let mut n = 0;
        let mut expect_pts = 0;
        while let Ok(p) = d.next_packet() {
            assert_eq!(p.data.len(), 768);
            assert_eq!(p.pts, Some(expect_pts));
            assert_eq!(p.duration, Some(1536));
            assert!(p.flags.keyframe);
            expect_pts += 1536;
            n += 1;
        }
        assert_eq!(n, frames);
        // Seek back to the third frame.
        assert_eq!(d.seek_to(0, 2 * 1536 + 100).unwrap(), 2 * 1536);
        let p = d.next_packet().unwrap();
        assert_eq!(p.pts, Some(2 * 1536));
        assert_eq!(&p.data[..], &FIXTURE[2 * 768..3 * 768]);
    }

    #[test]
    fn resyncs_over_leading_junk_and_truncated_tail() {
        let mut data = b"junk\x0B\x77\x00".to_vec();
        data.extend_from_slice(&FIXTURE[..768 * 3 + 100]);
        let mut d = Ac3Demuxer::open(Box::new(std::io::Cursor::new(data))).unwrap();
        let mut n = 0;
        while let Ok(p) = d.next_packet() {
            assert_eq!(p.data.len(), 768);
            n += 1;
        }
        assert_eq!(n, 3);
    }
}
