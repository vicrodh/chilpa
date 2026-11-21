//! A DSD track handed to a network renderer as a DSF file.
//!
//! A renderer that cannot open a SACD image, and many that cannot open
//! DSDIFF, read DSF. The one-bit samples stay the ones the disc or the file
//! carries; only their layout changes: blocks of 4096 bytes per channel, one
//! channel after the other, with the earliest bit in the least significant
//! position -- the exact inverse of what `chilpa_dsd` undoes when it reads a
//! DSF. A DST track is decoded back to those bits first, as local playback
//! does. The length is known before the first byte, and any byte range can be
//! read on its own, so the media server can answer HEAD and Range requests.
//!
//! Nothing here plays: local playback never goes through this module.
use chilpa_dsd::{DsdInfo, DsdSource};
use std::{io::{self, Read}, path::Path, sync::Arc};

/// Bytes per channel in one DSF block.
const BLOCK: usize = 4096;
/// The DSD, fmt and data chunk headers, in that order.
pub const HEADER: u64 = 28 + 52 + 12;

type Opener = Arc<dyn Fn() -> Result<Box<dyn DsdSource>, String> + Send + Sync>;

/// A DSD track as a DSF file of known length.
pub struct DsfStream {
    info: DsdInfo,
    open: Opener,
    tags: Option<chilpa_tags::Tags>,
}

impl DsfStream {
    /// Opens a track of a SACD image (`sacd:<image>#<number>`) or a DSDIFF
    /// file as DSF. A DSDIFF that carries DST is refused, with the reason the
    /// DSDIFF reader gives.
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some((image, number)) = crate::sacd_tracks::parse(path) {
            let (tags, _) = crate::sacd_tracks::about(&image, number)
                .ok_or_else(|| chilpa_i18n::t_args("the image has no track {}", &[&number.to_string()]))?;
            let info = crate::sacd_tracks::open(&image, number)?.info().clone();
            let open: Opener = Arc::new(move || crate::sacd_tracks::open(&image, number).map(|r| Box::new(r) as Box<dyn DsdSource>));
            return Self::from_source(info, open, Some(tags));
        }
        if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("dff")) {
            let info = crate::dsd_playback::open_dsd_file(path).map_err(|e| e.to_string())?.info().clone();
            // The DSDIFF reader keeps how many channels there are, not which
            // speakers they are, and DSF 1.01 states DSD64 and DSD128 only: a
            // file beyond mono or stereo at those rates is left as it is.
            if info.channels > 2 || !matches!(info.dsd_rate_hz, 2_822_400 | 5_644_800) {
                return Err("a DSDIFF whose layout or rate DSF does not state".into());
            }
            let file = path.to_path_buf();
            let open: Opener = Arc::new(move || crate::dsd_playback::open_dsd_file(&file).map_err(|e| e.to_string()));
            return Self::from_source(info, open, None);
        }
        Err("neither a disc track nor a DSDIFF file".into())
    }

    /// A stream over any DSD source; `open` gives a fresh reader for each
    /// range asked for.
    pub(crate) fn from_source(info: DsdInfo, open: Opener, tags: Option<chilpa_tags::Tags>) -> Result<Self, String> {
        if info.channels == 0 || info.channels > 6 || info.dsd_rate_hz == 0 {
            return Err("a DSD layout DSF cannot carry".into());
        }
        Ok(Self { info, open, tags })
    }

    /// Bytes of the whole file.
    pub fn length(&self) -> u64 { HEADER + self.data_len() }
    pub fn seconds(&self) -> f64 { self.info.seconds() }
    pub fn rate_hz(&self) -> u32 { self.info.dsd_rate_hz }
    /// What the disc says about the track, when it is a disc's.
    pub fn tags(&self) -> Option<&chilpa_tags::Tags> { self.tags.as_ref() }

    fn channels(&self) -> usize { usize::from(self.info.channels) }
    fn group(&self) -> u64 { (BLOCK * self.channels()) as u64 }
    fn groups(&self) -> u64 { self.info.samples.div_ceil(8).div_ceil(BLOCK as u64) }
    fn data_len(&self) -> u64 { self.groups() * self.group() }

    fn header(&self) -> Vec<u8> {
        let (channels, data) = (u32::from(self.info.channels), self.data_len());
        let mut h = Vec::with_capacity(HEADER as usize);
        h.extend_from_slice(b"DSD ");
        h.extend_from_slice(&28u64.to_le_bytes());
        h.extend_from_slice(&(HEADER + data).to_le_bytes());
        h.extend_from_slice(&0u64.to_le_bytes()); // no metadata chunk
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&52u64.to_le_bytes());
        h.extend_from_slice(&1u32.to_le_bytes()); // format version
        h.extend_from_slice(&0u32.to_le_bytes()); // DSD raw
        h.extend_from_slice(&channel_type(channels).to_le_bytes());
        h.extend_from_slice(&channels.to_le_bytes());
        h.extend_from_slice(&self.info.dsd_rate_hz.to_le_bytes());
        h.extend_from_slice(&1u32.to_le_bytes()); // earliest bit least significant
        h.extend_from_slice(&self.info.samples.to_le_bytes());
        h.extend_from_slice(&(BLOCK as u32).to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&(12 + data).to_le_bytes());
        h
    }

    /// `length` bytes from `start`, read by a reader of their own.
    pub fn read(&self, start: u64, length: u64) -> io::Result<Box<dyn Read + Send>> {
        let end = start.saturating_add(length).min(self.length());
        let mut packer = Packer {
            source: None, open: self.open.clone(), channels: self.channels(), header: self.header(), at: start, end,
            group: Vec::new(), group_index: 0, groups: self.groups(), queues: vec![Vec::new(); self.channels()], drop: 0,
            total: self.info.samples.div_ceil(8), owed: 0, done: false,
        };
        // An empty range reads nothing, and opens nothing.
        if end > HEADER && start < end {
            packer.start_at((start.max(HEADER) - HEADER) / self.group())?;
        }
        Ok(Box::new(packer))
    }
}

/// The DSF channel type for a channel count: mono, stereo, three channels,
/// quad, five channels, 5.1.
fn channel_type(channels: u32) -> u32 {
    match channels { 1 => 1, 2 => 2, 3 => 3, 4 => 4, 5 => 6, _ => 7 }
}

struct Packer {
    source: Option<Box<dyn DsdSource>>,
    open: Opener,
    channels: usize,
    header: Vec<u8>,
    /// The next byte of the file to hand over, and one past the last.
    at: u64,
    end: u64,
    /// The block group being handed over, already laid out as DSF.
    group: Vec<u8>,
    group_index: u64,
    groups: u64,
    /// What the source gave per channel and no group took yet.
    queues: Vec<Vec<u8>>,
    /// Bytes per channel still to discard from the source: the part before
    /// the first group asked for that a seek could not skip.
    drop: u64,
    /// Bytes per channel of the whole track, and those the source still owes
    /// from where it landed: one that ends sooner is a cut, never silence.
    total: u64,
    owed: u64,
    done: bool,
}

impl Packer {
    /// Opens the source at group `index`: at the nearest point at or before
    /// it, the samples in between read and dropped, so a range holds exactly
    /// what the whole file holds there.
    fn start_at(&mut self, index: u64) -> io::Result<()> {
        let mut source = (self.open)().map_err(io::Error::other)?;
        let target = index * BLOCK as u64 * 8;
        // A fresh source already stands at the first sample.
        let mut landed = 0;
        if target > 0 {
            // A source can land after the point asked for: a DST disc finds a
            // frame by the first one that starts in a sector. It is asked
            // again from earlier, further back each time, until it lands at
            // or before the target; the start is the last resort.
            let (mut probe, mut back) = (target, BLOCK as u64 * 8);
            loop {
                match source.seek(probe).map_err(|e| io::Error::other(e.to_string()))? {
                    Some(at) if at <= target => { landed = at; break; }
                    // It cannot move, and stands at the start.
                    None => break,
                    Some(_) if probe == 0 => { source = (self.open)().map_err(io::Error::other)?; break; }
                    Some(_) => { probe = probe.saturating_sub(back); back = back.saturating_mul(2); }
                }
            }
        }
        self.drop = (target - landed) / 8;
        self.owed = self.total.saturating_sub(landed / 8);
        self.source = Some(source);
        self.group_index = index;
        self.fill()
    }

    /// Lays out group `group_index` from the source, padded with zeros past
    /// the end of the samples as DSF asks.
    fn fill(&mut self) -> io::Result<()> {
        while !self.done && self.queues.iter().any(|q| q.len() < BLOCK) {
            let Some(source) = self.source.as_mut() else { break };
            match source.next_block().map_err(|e| io::Error::other(e.to_string()))? {
                Some(blocks) => {
                    if blocks.len() != self.channels { return Err(io::Error::other("the source changed its channel count")); }
                    let skip = self.drop.min(blocks[0].len() as u64) as usize;
                    self.drop -= skip as u64;
                    self.owed = self.owed.saturating_sub(blocks[0].len() as u64);
                    for (queue, block) in self.queues.iter_mut().zip(&blocks) { queue.extend_from_slice(&block[skip..]); }
                }
                None if self.owed > 0 => return Err(io::Error::other("the source ended before the length it stated")),
                None => self.done = true,
            }
        }
        self.group.clear();
        for queue in &mut self.queues {
            let take = queue.len().min(BLOCK);
            self.group.extend(queue.drain(..take).map(u8::reverse_bits));
            self.group.resize(self.group.len() + BLOCK - take, 0);
        }
        Ok(())
    }
}

impl Read for Packer {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.at >= self.end || out.is_empty() { return Ok(0); }
        if self.at < HEADER {
            let from = self.at as usize;
            let count = (out.len() as u64).min(HEADER - self.at).min(self.end - self.at) as usize;
            out[..count].copy_from_slice(&self.header[from..from + count]);
            self.at += count as u64;
            return Ok(count);
        }
        let group = (BLOCK * self.channels) as u64;
        let data = self.at - HEADER;
        while data / group > self.group_index {
            if self.group_index + 1 >= self.groups { return Ok(0); }
            self.group_index += 1;
            self.fill()?;
        }
        let within = (data % group) as usize;
        let count = (out.len() as u64).min((self.group.len() - within) as u64).min(self.end - self.at) as usize;
        out[..count].copy_from_slice(&self.group[within..within + count]);
        self.at += count as u64;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How a test source answers a seek.
    #[derive(Clone, Copy, Debug)]
    enum Lands { Before, After, Never }
    /// Frames of 1,000 bytes per channel, each byte telling its channel and
    /// position apart, landing on a frame boundary when asked to seek: the one
    /// at or before the sample, the one after it, or none.
    struct Frames { info: DsdInfo, next: u64, lands: Lands, has: u64 }
    impl Frames {
        const FRAME: u64 = 1000;
        fn byte(channel: usize, index: u64) -> u8 { (index.wrapping_mul(31) as u8) ^ (channel as u8).wrapping_mul(0x5b) }
    }
    impl DsdSource for Frames {
        fn info(&self) -> &DsdInfo { &self.info }
        fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, chilpa_dsd::DsdError> {
            let total = self.has;
            if self.next >= total { return Ok(None); }
            let end = (self.next + Self::FRAME).min(total);
            let blocks = (0..usize::from(self.info.channels)).map(|c| (self.next..end).map(|i| Self::byte(c, i)).collect()).collect();
            self.next = end;
            Ok(Some(blocks))
        }
        fn seek(&mut self, sample: u64) -> Result<Option<u64>, chilpa_dsd::DsdError> {
            let total = self.info.samples / 8;
            self.next = match self.lands {
                Lands::Never => return Ok(None),
                Lands::Before => (sample / 8 / Self::FRAME) * Self::FRAME,
                Lands::After => ((sample / 8).div_ceil(Self::FRAME) * Self::FRAME).min(total),
            };
            Ok(Some(self.next * 8))
        }
    }
    fn stream(channels: u16, bytes: u64, lands: Lands) -> DsfStream {
        short(channels, bytes, bytes, lands)
    }
    /// A source that states `bytes` per channel and has only `has`.
    fn short(channels: u16, bytes: u64, has: u64, lands: Lands) -> DsfStream {
        let info = DsdInfo { dsd_rate_hz: 2_822_400, channels, samples: bytes * 8 };
        let made = info.clone();
        DsfStream::from_source(info, Arc::new(move || Ok(Box::new(Frames { info: made.clone(), next: 0, lands, has }) as Box<dyn DsdSource>)), None).unwrap()
    }
    fn all(s: &DsfStream, start: u64, length: u64) -> Vec<u8> {
        let mut out = Vec::new();
        s.read(start, length).unwrap().read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn the_dsf_reader_gets_back_the_same_bits_per_channel() {
        for channels in [1u16, 2, 6] {
            // Not a whole number of blocks, so the last one is padded.
            let bytes = 3 * BLOCK as u64 + 777;
            let s = stream(channels, bytes, Lands::Before);
            let file = all(&s, 0, s.length());
            assert_eq!(file.len() as u64, s.length());
            let path = std::env::temp_dir().join(format!("chilpa-dsf-stream-{channels}.dsf"));
            std::fs::write(&path, &file).unwrap();
            let mut reader = chilpa_dsd::DsfReader::open(&path).unwrap();
            assert_eq!(reader.info(), &DsdInfo { dsd_rate_hz: 2_822_400, channels, samples: bytes * 8 });
            let mut got = vec![Vec::new(); usize::from(channels)];
            while let Some(blocks) = reader.next_block().unwrap() {
                for (c, block) in blocks.into_iter().enumerate() { got[c].extend(block); }
            }
            for (c, bits) in got.iter().enumerate() {
                let expected: Vec<u8> = (0..bytes).map(|i| Frames::byte(c, i)).collect();
                assert_eq!(&bits[..bytes as usize], &expected[..], "channel {c} of {channels}");
            }
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn any_range_reads_what_the_whole_file_holds_there() {
        for lands in [Lands::Before, Lands::After, Lands::Never] {
            let s = stream(2, 5 * BLOCK as u64 + 123, lands);
            let file = all(&s, 0, s.length());
            let group = 2 * BLOCK as u64;
            for (start, length) in [(0, 10), (50, 100), (HEADER, 1), (HEADER + group - 3, 9), (HEADER + 3 * group + 17, group + 5),
                (s.length() - 40, 40), (s.length() - 40, 4000), (HEADER + 2 * group, 0)] {
                let end = (start + length).min(s.length()) as usize;
                assert_eq!(all(&s, start, length), &file[start as usize..end], "start {start} length {length} lands {lands:?}");
            }
        }
    }

    /// A two-track SACD image as `tests/sacd_queue.rs` builds it, its audio
    /// bytes all different so a channel or order mistake shows.
    fn sacd_image(name: &str) -> std::path::PathBuf {
        const SECTOR: usize = 2048;
        const AREA: u32 = 540;
        const AUDIO_START: u32 = 550;
        const AUDIO_END: u32 = 569;
        fn sector(img: &mut [u8], lsn: u32) -> &mut [u8] { let at = lsn as usize * SECTOR; &mut img[at..at + SECTOR] }
        let be32 = |v: u32| v.to_be_bytes();
        let mut img = vec![0u8; 600 * SECTOR];
        {
            let m = sector(&mut img, 510);
            m[..8].copy_from_slice(b"SACDMTOC");
            m[8] = 1; m[9] = 20;
            m[0x40..0x44].copy_from_slice(&be32(AREA));
            m[0x54..0x56].copy_from_slice(&5u16.to_be_bytes());
            m[0x80] = 1; m[0x8a] = 2;
        }
        {
            let t = sector(&mut img, 511);
            t[..8].copy_from_slice(b"SACDText");
            t[0x10..0x12].copy_from_slice(&0x40u16.to_be_bytes());
            t[0x12..0x14].copy_from_slice(&0x80u16.to_be_bytes());
            t[0x40..0x40 + 18].copy_from_slice(b"Agents Of Fortune\0");
            t[0x80..0x80 + 17].copy_from_slice(b"Blue \xD6yster Cult\0");
        }
        {
            let a = sector(&mut img, AREA);
            a[..8].copy_from_slice(b"TWOCHTOC");
            a[0x14] = 4; a[0x15] = 2; a[0x20] = 2; a[0x41] = 2; a[0x45] = 2;
            a[0x48..0x4c].copy_from_slice(&be32(AUDIO_START));
            a[0x4c..0x50].copy_from_slice(&be32(AUDIO_END));
            a[0x5a] = 2;
            a[0x80..0x82].copy_from_slice(&3u16.to_be_bytes());
        }
        const SECOND: usize = 8 + 255 * 4;
        {
            let t1 = sector(&mut img, AREA + 1);
            t1[..8].copy_from_slice(b"SACDTRL1");
            t1[8..12].copy_from_slice(&be32(AUDIO_START));
            t1[12..16].copy_from_slice(&be32(AUDIO_START + 10));
            t1[SECOND..SECOND + 4].copy_from_slice(&be32(10));
            t1[SECOND + 4..SECOND + 8].copy_from_slice(&be32(10));
        }
        {
            let t2 = sector(&mut img, AREA + 2);
            t2[..8].copy_from_slice(b"SACDTRL2");
            t2[12..16].copy_from_slice(&[0, 1, 0, 0]);
            t2[SECOND..SECOND + 4].copy_from_slice(&[0, 0, 2, 0]);
            t2[SECOND + 4..SECOND + 8].copy_from_slice(&[0, 0, 2, 0]);
        }
        {
            let b = sector(&mut img, AREA + 3);
            b[..8].copy_from_slice(b"SACDTTxt");
            b[8..10].copy_from_slice(&(SECTOR as u16 + 0x20).to_be_bytes());
            b[10..12].copy_from_slice(&(SECTOR as u16 + 0x80).to_be_bytes());
        }
        {
            let b = sector(&mut img, AREA + 4);
            b[0x20] = 1; b[0x24] = 1;
            b[0x26..0x26 + 30].copy_from_slice(b"This Ain't The Summer Of Love\0");
            b[0x80] = 1; b[0x84] = 1;
            b[0x86..0x86 + 17].copy_from_slice(b"True Confessions\0");
        }
        for (i, lsn) in (AUDIO_START..=AUDIO_END).enumerate() {
            let s = sector(&mut img, lsn);
            s[0] = (1 << 5) | (1 << 2);
            let header = 1 + 2 + 3;
            let w: u16 = 0x8000 | (2 << 11) | (SECTOR - header) as u16;
            s[1..3].copy_from_slice(&w.to_be_bytes());
            s[4] = (i / 75) as u8;
            s[5] = (i % 75) as u8;
            for (k, b) in s[header..].iter_mut().enumerate() { *b = ((i * 2042 + k) % 251) as u8 ^ 0xa5; }
        }
        let path = std::env::temp_dir().join(format!("chilpa-dsf-stream-{name}.iso"));
        std::fs::write(&path, img).unwrap();
        path
    }

    #[test]
    fn a_disc_track_goes_out_with_the_bits_the_disc_carries() {
        let iso = sacd_image("disc-track");
        let track = std::path::PathBuf::from(format!("sacd:{}#1", iso.display()));
        let stream = DsfStream::open(&track).unwrap();
        let tags = stream.tags().unwrap();
        assert_eq!(tags.title.as_deref(), Some("This Ain't The Summer Of Love"));
        assert_eq!(tags.album.as_deref(), Some("Agents Of Fortune"));
        let mut disc = crate::sacd_tracks::open(&iso, 1).unwrap();
        assert_eq!(stream.rate_hz(), 2_822_400);
        assert_eq!(stream.seconds(), disc.info().seconds());
        let mut expected = vec![Vec::new(); 2];
        while let Some(blocks) = disc.next_block().unwrap() {
            for (c, block) in blocks.into_iter().enumerate() { expected[c].extend(block); }
        }
        assert!(expected[0].iter().any(|b| *b != expected[0][0]), "the fixture is not constant");
        let path = std::env::temp_dir().join("chilpa-dsf-stream-disc-track.dsf");
        std::fs::write(&path, all(&stream, 0, stream.length())).unwrap();
        let mut reader = chilpa_dsd::DsfReader::open(&path).unwrap();
        let mut got = vec![Vec::new(); 2];
        while let Some(blocks) = reader.next_block().unwrap() {
            for (c, block) in blocks.into_iter().enumerate() { got[c].extend(block); }
        }
        for c in 0..2 { assert_eq!(&got[c][..expected[c].len()], &expected[c][..], "channel {c}"); }
        assert!(DsfStream::open(std::path::Path::new("/music/missing.dff")).is_err(), "a file that is not there");
        assert!(DsfStream::open(std::path::Path::new("/music/a.flac")).is_err(), "not DSD");
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(iso);
    }

    /// A DSDIFF as `chilpa-dsd`'s own tests write it, with `bytes` bytes per
    /// channel; `compression` is what CMPR says.
    fn dff(name: &str, bytes: usize, compression: &[u8; 4]) -> std::path::PathBuf { dff_of(name, bytes, compression, 2, 2_822_400) }
    fn dff_of(name: &str, bytes: usize, compression: &[u8; 4], count: u16, rate: u32) -> std::path::PathBuf {
        fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
            let mut out = id.to_vec();
            out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
            out.extend_from_slice(payload);
            if payload.len() % 2 == 1 { out.push(0); }
            out
        }
        let mut prop = b"SND ".to_vec();
        prop.extend(chunk(b"FS  ", &rate.to_be_bytes()));
        let mut channels = count.to_be_bytes().to_vec();
        channels.extend_from_slice(&b"MLFTMRGTC   LFE LS  RS  "[..4 * usize::from(count)]);
        prop.extend(chunk(b"CHNL", &channels));
        let mut compression_chunk = compression.to_vec();
        compression_chunk.push(3);
        compression_chunk.extend_from_slice(b"raw");
        prop.extend(chunk(b"CMPR", &compression_chunk));
        let samples: Vec<u8> = (0..bytes).flat_map(|i| (0..count).map(move |c| ((i * 7 + usize::from(c) * 13) % 251) as u8)).collect();
        let mut form = b"DSD ".to_vec();
        form.extend(chunk(b"FVER", &0x0105_0000u32.to_be_bytes()));
        form.extend(chunk(b"PROP", &prop));
        form.extend(chunk(if compression == b"DST " { b"DST " } else { b"DSD " }, &samples));
        let path = std::env::temp_dir().join(format!("chilpa-dsf-stream-{name}.dff"));
        std::fs::write(&path, chunk(b"FRM8", &form)).unwrap();
        path
    }

    #[test]
    fn a_dsdiff_goes_out_with_its_own_bits_and_any_range_matches() {
        let bytes = 3 * BLOCK + 555;
        let path = dff("dsdiff", bytes, b"DSD ");
        let stream = DsfStream::open(&path).unwrap();
        assert!(stream.tags().is_none(), "a file's tags are read from the file");
        let mut original = chilpa_dsd::DffReader::open(&path).unwrap();
        let mut expected = vec![Vec::new(); 2];
        while let Some(blocks) = original.next_block().unwrap() {
            for (c, block) in blocks.into_iter().enumerate() { expected[c].extend(block); }
        }
        let file = all(&stream, 0, stream.length());
        let out = std::env::temp_dir().join("chilpa-dsf-stream-dsdiff.dsf");
        std::fs::write(&out, &file).unwrap();
        let mut reader = chilpa_dsd::DsfReader::open(&out).unwrap();
        let mut got = vec![Vec::new(); 2];
        while let Some(blocks) = reader.next_block().unwrap() {
            for (c, block) in blocks.into_iter().enumerate() { got[c].extend(block); }
        }
        for c in 0..2 { assert_eq!(&got[c][..bytes], &expected[c][..], "channel {c}"); }
        let group = 2 * BLOCK as u64;
        for (start, length) in [(HEADER + group + 5, 3 * BLOCK as u64), (stream.length() - 9, 9), (10, HEADER)] {
            assert_eq!(all(&stream, start, length), &file[start as usize..(start + length) as usize], "start {start}");
        }
        let dst = dff("dsdiff-dst", 64, b"DST ");
        assert!(DsfStream::open(&dst).is_err(), "a DST DSDIFF cannot be laid out again");
        let four = dff_of("dsdiff-four", 64, b"DSD ", 4, 2_822_400);
        assert!(chilpa_dsd::DffReader::open(&four).is_ok(), "the reader takes it");
        assert!(DsfStream::open(&four).is_err(), "which speakers four channels are is not known here");
        let fast = dff_of("dsdiff-dsd256", 64, b"DSD ", 2, 11_289_600);
        assert!(chilpa_dsd::DffReader::open(&fast).is_ok(), "the reader takes it");
        assert!(DsfStream::open(&fast).is_err(), "DSF 1.01 states DSD64 and DSD128 only");
        assert!(DsfStream::open(&dff_of("dsdiff-dsd128", 64, b"DSD ", 1, 5_644_800)).is_ok(), "mono DSD128 goes out");
        for p in [path, out, dst, four, fast] { let _ = std::fs::remove_file(p); }
    }

    #[test]
    fn a_source_that_ends_before_its_length_is_a_cut_not_silence() {
        let s = short(2, 3 * BLOCK as u64, 2 * BLOCK as u64 + 10, Lands::Before);
        let mut out = Vec::new();
        assert!(s.read(0, s.length()).unwrap().read_to_end(&mut out).is_err());
    }

    #[test]
    fn an_empty_range_or_the_header_alone_opens_no_source() {
        let info = DsdInfo { dsd_rate_hz: 2_822_400, channels: 2, samples: 3 * BLOCK as u64 * 8 };
        let s = DsfStream::from_source(info, Arc::new(|| Err("must not open".into())), None).unwrap();
        for (start, length) in [(HEADER + 100, 0), (0, HEADER), (10, 20)] {
            let mut out = Vec::new();
            s.read(start, length).unwrap().read_to_end(&mut out).unwrap();
            assert_eq!(out.len() as u64, length, "start {start}");
        }
        assert!(s.read(HEADER, 1).is_err(), "a byte of samples does open it");
    }

    #[test]
    fn the_header_states_length_layout_and_bit_order() {
        let s = stream(2, BLOCK as u64 * 2 + 1, Lands::Before);
        let h = all(&s, 0, HEADER);
        assert_eq!(&h[..4], b"DSD ");
        assert_eq!(u64::from_le_bytes(h[12..20].try_into().unwrap()), s.length());
        assert_eq!(&h[28..32], b"fmt ");
        assert_eq!(u32::from_le_bytes(h[48..52].try_into().unwrap()), 2, "stereo");
        assert_eq!(u32::from_le_bytes(h[52..56].try_into().unwrap()), 2, "two channels");
        assert_eq!(u32::from_le_bytes(h[56..60].try_into().unwrap()), 2_822_400);
        assert_eq!(u32::from_le_bytes(h[60..64].try_into().unwrap()), 1, "earliest bit least significant");
        assert_eq!(u64::from_le_bytes(h[64..72].try_into().unwrap()), (BLOCK as u64 * 2 + 1) * 8);
        assert_eq!(u32::from_le_bytes(h[72..76].try_into().unwrap()), BLOCK as u32);
        assert_eq!(&h[80..84], b"data");
        assert_eq!(s.length(), HEADER + 3 * 2 * BLOCK as u64, "three blocks per channel, the last padded");
    }
}
