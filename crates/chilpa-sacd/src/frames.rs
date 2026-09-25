//! From sectors to DSD blocks, one per channel.
//!
//! An uncompressed stereo frame is 9408 bytes and carries both channels
//! **interleaved byte by byte**. That was not deduced, it was measured. QBZ
//! tried the other hypothesis first (a contiguous block per channel) and the
//! result left 23% of the energy above the audible band; with byte
//! interleaving it came to 99.8% inside. The wrong hypothesis made it into a
//! release, so it is written down here.
//!
//! A DST track decodes each frame back into those same 9408 bytes, in the
//! same order, so both end up in the same place.

use crate::sector::{self, Packet};
use crate::{SacdError, SECTOR, STEREO_FRAME};
use chilpa_dsd::{DsdInfo, DsdSource};
use dst_decoder::decoder::DstDecoder;

use crate::toc::{SacdImage, SacdTrack};

/// Reads a track from the audio area and hands it over as DSD per channel.
pub struct SacdTrackReader {
    image: SacdImage,
    /// Sectors of the track still to read.
    lsn: u64,
    end_lsn: u64,
    /// Audio read that does not yet make up a whole frame.
    pending: Vec<u8>,
    /// Whether the track's first frame has been reached. Until it is, audio
    /// is dropped: a track's first sector begins with the tail of the last
    /// frame of the track before -- 672 or 1344 bytes of it on most tracks of
    /// the discs this was measured on -- and taking it cut every frame of
    /// the track across two real ones, which QBZ measured as noise with the
    /// music behind it.
    synced: bool,
    /// Frames the disc says the track still has. Its sectors can hold a
    /// whole frame more, and that frame is the next track's.
    frames_left: u32,
    /// What a DST track needs on top; `None` for uncompressed DSD.
    dst: Option<Dst>,
    info: DsdInfo,
}

impl SacdTrackReader {
    /// Opens a track, uncompressed or DST.
    pub fn open(image: SacdImage, track: &SacdTrack) -> Result<Self, SacdError> {
        let dst = match track.encoding.is_compressed() {
            true => Some(Dst::new(track)?),
            false => None,
        };
        let samples = u64::from(track.duration_frames) * u64::from(crate::DSD64_HZ / 75);
        Ok(Self {
            lsn: u64::from(track.start_lsn),
            end_lsn: u64::from(track.start_lsn) + u64::from(track.length_lsn),
            image,
            pending: Vec::with_capacity(STEREO_FRAME * 2),
            synced: false,
            frames_left: track.duration_frames,
            dst,
            info: DsdInfo { dsd_rate_hz: crate::DSD64_HZ, channels: 2, samples },
        })
    }

    /// Reads sectors until there is at least one whole frame.
    fn fill(&mut self) -> Result<(), SacdError> {
        let mut sector_buf = vec![0u8; SECTOR];
        while self.pending.len() < STEREO_FRAME && self.lsn < self.end_lsn {
            self.image.read_sector(self.lsn, &mut sector_buf)?;
            let packets = sector::parse(&sector_buf, self.lsn)?;
            if packets.dst != self.dst.is_some() {
                return Err(SacdError::BadSector {
                    lsn: self.lsn,
                    why: chilpa_i18n::mark("the sector is not encoded the way its area says").into(),
                });
            }
            for p in packets.packets.iter().filter(|p| p.is_audio()) {
                if let Some(dst) = self.dst.as_mut() {
                    dst.take(p, &sector_buf[p.at..p.at + p.len], self.lsn, &mut self.pending)?;
                    continue;
                }
                if !self.synced && !p.frame_start {
                    continue;
                }
                self.synced = true;
                self.pending.extend_from_slice(&sector_buf[p.at..p.at + p.len]);
            }
            self.lsn += 1;
        }
        Ok(())
    }
}

impl DsdSource for SacdTrackReader {
    fn info(&self) -> &DsdInfo {
        &self.info
    }

    fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, chilpa_dsd::DsdError> {
        if self.frames_left == 0 {
            return Ok(None);
        }
        self.fill().map_err(|e| chilpa_dsd::DsdError::Corrupt(e.to_string()))?;
        if self.pending.len() < STEREO_FRAME {
            // What is left does not make a frame: it is the end of the track.
            self.pending.clear();
            return Ok(None);
        }
        let frame: Vec<u8> = self.pending.drain(..STEREO_FRAME).collect();
        self.frames_left -= 1;
        Ok(Some(split_channels(&frame)))
    }
}

/// The most a DST frame may take on the disc. An uncompressed one is 9408
/// bytes, and DST that does not save space stores the frame as it is.
const MAX_DST_FRAME: usize = 64 * 1024;

/// A DST track's frames, whole again and decoded.
///
/// A compressed frame spans as many packets as its first packet's record
/// says, and several can start in one sector: the sector a track starts in
/// can hold the start of the previous track's last frame, and its sectors
/// can hold the next track's first. So the timecode, and not the order the
/// frames come in, says which are this track's.
struct Dst {
    decoder: DstDecoder,
    /// The frame whose packets are being gathered.
    open: Option<OpenFrame>,
    /// Whether a frame has started since the track's first sector. Before
    /// one does, a packet that starts nothing is the tail of a frame from
    /// before the track; after, it is one no frame counted.
    begun: bool,
    /// Timecode of the track's first frame, the next it will play, and one
    /// past its last.
    first: u32,
    next: u32,
    end: u32,
    decoded: Vec<u8>,
}

struct OpenFrame {
    payload: Vec<u8>,
    /// Packets still to come.
    missing: u8,
    timecode: u32,
    lsn: u64,
}

impl Dst {
    fn new(track: &SacdTrack) -> Result<Self, SacdError> {
        let decoder = DstDecoder::new(2, crate::DSD64_HZ as usize)
            .map_err(|e| SacdError::Unsupported(e.to_string()))?;
        Ok(Self {
            decoder,
            open: None,
            begun: false,
            first: track.start_frame,
            next: track.start_frame,
            end: track.start_frame.saturating_add(track.duration_frames),
            decoded: vec![0; STEREO_FRAME],
        })
    }

    /// Takes one audio packet. A frame is decoded when its last packet is
    /// in, and goes to `out` if it is this track's.
    fn take(&mut self, p: &Packet, bytes: &[u8], lsn: u64, out: &mut Vec<u8>) -> Result<(), SacdError> {
        let bad = |why: &str| SacdError::BadSector { lsn, why: why.into() };
        if p.frame_start {
            if self.open.is_some() {
                return Err(bad(chilpa_i18n::mark("a DST frame ends before all its packets came")));
            }
            let missing = match p.dst_packets {
                Some(n @ 1..=31) => n,
                _ => return Err(bad(chilpa_i18n::mark("a DST frame says it spans no packets"))),
            };
            if p.dst_channels != Some(2) {
                return Err(bad(chilpa_i18n::mark("a frame of the stereo area is not stereo")));
            }
            let timecode = p.timecode.ok_or_else(|| bad(chilpa_i18n::mark("a DST frame has no timecode")))?;
            self.open = Some(OpenFrame { payload: Vec::with_capacity(16 * 1024), missing, timecode, lsn });
            self.begun = true;
        }
        let Some(frame) = self.open.as_mut() else {
            return match self.begun {
                true => Err(bad(chilpa_i18n::mark("a DST packet belongs to no frame"))),
                false => Ok(()),
            };
        };
        frame.payload.extend_from_slice(bytes);
        if frame.payload.len() > MAX_DST_FRAME {
            return Err(bad(chilpa_i18n::mark("a DST frame is larger than 64 KiB")));
        }
        frame.missing -= 1;
        if frame.missing == 0 {
            if let Some(frame) = self.open.take() {
                self.finish(frame, out)?;
            }
        }
        Ok(())
    }

    fn finish(&mut self, frame: OpenFrame, out: &mut Vec<u8>) -> Result<(), SacdError> {
        if frame.timecode < self.first || frame.timecode >= self.end {
            // The previous track's last frame, or the next track's first.
            return Ok(());
        }
        if frame.timecode != self.next {
            return Err(SacdError::BadSector {
                lsn: frame.lsn,
                why: chilpa_i18n::t_args(
                    "the DST frames jump from {} to {}",
                    &[&self.next.to_string(), &frame.timecode.to_string()],
                ),
            });
        }
        // Nothing is kept unless the frame decoded whole: on an error the
        // decoder fills the frame with 0x55, which is silence that would
        // reach the DAC as if it were the disc's.
        match self.decoder.decode_frame(&frame.payload, &mut self.decoded) {
            Ok(STEREO_FRAME) => {
                out.extend_from_slice(&self.decoded);
                self.next += 1;
                Ok(())
            }
            Ok(n) => Err(SacdError::BadSector {
                lsn: frame.lsn,
                why: chilpa_i18n::t_args("a DST frame decoded to {} bytes and not 9408", &[&n.to_string()]),
            }),
            Err(e) => Err(SacdError::BadSector {
                lsn: frame.lsn,
                why: chilpa_i18n::t_args("a DST frame does not decode: {}", &[&e.to_string()]),
            }),
        }
    }
}

/// Splits a stereo frame into its two channels. The bytes alternate: even
/// goes left, odd goes right.
pub fn split_channels(frame: &[u8]) -> Vec<Vec<u8>> {
    let half = frame.len() / 2;
    let mut left = Vec::with_capacity(half);
    let mut right = Vec::with_capacity(half);
    for pair in frame.chunks_exact(2) {
        left.push(pair[0]);
        right.push(pair[1]);
    }
    vec![left, right]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_come_interleaved_byte_by_byte() {
        let frame: Vec<u8> = (0..8u8).collect();
        let channels = split_channels(&frame);
        assert_eq!(channels[0], vec![0, 2, 4, 6], "the left one takes the even ones");
        assert_eq!(channels[1], vec![1, 3, 5, 7], "the right one takes the odd ones");
    }

    #[test]
    fn a_frame_splits_into_two_equal_halves() {
        let channels = split_channels(&vec![0u8; STEREO_FRAME]);
        assert_eq!(channels[0].len(), STEREO_FRAME / 2);
        assert_eq!(channels[0].len(), channels[1].len());
    }
}
