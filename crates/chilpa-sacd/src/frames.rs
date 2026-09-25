//! From sectors to DSD blocks, one per channel.
//!
//! An uncompressed stereo frame is 9408 bytes and carries both channels
//! **interleaved byte by byte**. That was not deduced, it was measured. QBZ
//! tried the other hypothesis first (a contiguous block per channel) and the
//! result left 23% of the energy above the audible band; with byte
//! interleaving it came to 99.8% inside. The wrong hypothesis made it into a
//! release, so it is written down here.

use crate::sector;
use crate::{Encoding, SacdError, SECTOR, STEREO_FRAME};
use chilpa_dsd::{DsdInfo, DsdSource};

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
    info: DsdInfo,
}

impl SacdTrackReader {
    /// Opens a track. DST cannot be played yet.
    pub fn open(image: SacdImage, track: &SacdTrack) -> Result<Self, SacdError> {
        if track.encoding.is_compressed() {
            return Err(SacdError::Unsupported(
                chilpa_i18n::mark("the track is DST compressed and there is no decoder yet").into(),
            ));
        }
        let samples = u64::from(track.duration_frames) * u64::from(crate::DSD64_HZ / 75);
        Ok(Self {
            lsn: u64::from(track.start_lsn),
            end_lsn: u64::from(track.start_lsn) + u64::from(track.length_lsn),
            image,
            pending: Vec::with_capacity(STEREO_FRAME * 2),
            synced: false,
            frames_left: track.duration_frames,
            info: DsdInfo { dsd_rate_hz: crate::DSD64_HZ, channels: 2, samples },
        })
    }

    /// Reads sectors until there is at least one whole frame.
    fn fill(&mut self) -> Result<(), SacdError> {
        let mut sector_buf = vec![0u8; SECTOR];
        while self.pending.len() < STEREO_FRAME && self.lsn < self.end_lsn {
            self.image.read_sector(self.lsn, &mut sector_buf)?;
            let packets = sector::parse(&sector_buf, self.lsn)?;
            if packets.dst {
                return Err(SacdError::Unsupported(
                    chilpa_i18n::mark("the sector is DST compressed").into(),
                ));
            }
            for p in packets.packets.iter().filter(|p| p.is_audio()) {
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

/// Which encodings can be played today.
pub fn is_playable(encoding: Encoding) -> bool {
    !encoding.is_compressed()
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

    #[test]
    fn dst_is_not_playable_yet() {
        assert!(!is_playable(Encoding::Dst));
        assert!(is_playable(Encoding::Dsd3In14));
    }
}
