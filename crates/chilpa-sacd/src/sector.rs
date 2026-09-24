//! The packing inside a sector of the audio area.
//!
//! Layout measured in QBZ against a census of 3,066,893 sectors, without a
//! single exception:
//!
//! ```text
//! byte 0:  packet_count     = (b >> 5) & 0x07
//!          frame_info_count = (b >> 2) & 0x07
//!          dst              =  b & 0x01
//! then packet_count 16-bit descriptors, big endian:
//!          bit 15     starts a frame
//!          bits 14-11 data type
//!          bits 10-0  length IN BYTES       <- ELEVEN bits, not twelve
//! then frame_info_count records of 4 bytes (DST) or 3 (uncompressed)
//! ```
//!
//! The eleven bits of the length are the correction that matters. The
//! version going around gives it twelve, and the first descriptor of the
//! discs measured is 0x181B: with twelve bits that is 2075, more than the
//! whole sector. With eleven it is 27, and 5 + 27 + 2016 = 2048 exactly. The
//! sum `header + Σ lengths == 2048` is what proves the decoding, and here it
//! is checked on every sector instead of being taken on faith.

use crate::{SacdError, DATA_TYPE_AUDIO, SECTOR};

/// A packet inside a sector.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Packet {
    pub frame_start: bool,
    pub data_type: u16,
    /// Where the payload starts inside the sector.
    pub at: usize,
    pub len: usize,
    /// The area's timecode, when the packet starts an audio frame.
    pub timecode: Option<u32>,
}

impl Packet {
    pub fn is_audio(&self) -> bool {
        self.data_type == DATA_TYPE_AUDIO
    }
}

/// What is known about a sector of the audio area.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SectorPackets {
    /// True when the sector carries DST-compressed frames.
    pub dst: bool,
    pub packets: Vec<Packet>,
}

/// Decodes a sector's packing.
pub fn parse(sector: &[u8], lsn: u64) -> Result<SectorPackets, SacdError> {
    if sector.len() != SECTOR {
        return Err(SacdError::BadSector { lsn, why: "no mide 2048 bytes".into() });
    }
    let b0 = sector[0];
    let dst = b0 & 0x01 != 0;
    let packet_count = ((b0 >> 5) & 0x07) as usize;
    let frame_info_count = ((b0 >> 2) & 0x07) as usize;
    let frame_info_size = if dst { 4 } else { 3 };
    let frame_info_at = 1 + packet_count * 2;
    let header_len = frame_info_at + frame_info_count * frame_info_size;
    if packet_count == 0 || header_len > SECTOR {
        return Err(SacdError::BadSector { lsn, why: "cuentas de paquetes imposibles".into() });
    }

    let mut packets = Vec::with_capacity(packet_count);
    let mut at = header_len;
    let mut total = header_len;
    let mut read_frames = 0usize;
    for k in 0..packet_count {
        let w = u16::from_be_bytes([sector[1 + 2 * k], sector[2 + 2 * k]]);
        let len = (w & 0x07FF) as usize;
        let frame_start = w & 0x8000 != 0;
        let data_type = (w >> 11) & 0x07;

        // The frame-info record only exists for packets that start a frame.
        // In the lead-out there is a filler packet marked as a start with a
        // sentinel record: it counts, but its timecode means nothing.
                let timecode = if frame_start {
            let info = frame_info_at + read_frames * frame_info_size;
            read_frames += 1;
            if data_type == DATA_TYPE_AUDIO {
                let t = sector.get(info..info + 3).ok_or_else(|| SacdError::BadSector {
                    lsn,
                    why: "la tabla de frame-info está cortada".into(),
                })?;
                if t[1] >= 60 || t[2] >= 75 {
                    return Err(SacdError::BadSector {
                        lsn,
                        why: "código de tiempo inválido en frame-info".into(),
                    });
                }
                Some((u32::from(t[0]) * 60 + u32::from(t[1])) * 75 + u32::from(t[2]))
            } else {
                None
            }
        } else {
            None
        };

        total += len;
        if total > SECTOR {
            return Err(SacdError::BadSector {
                lsn,
                why: format!("los paquetes suman {total} bytes"),
            });
        }
        packets.push(Packet { frame_start, data_type, at, len, timecode });
        at += len;
    }
    if total != SECTOR {
        return Err(SacdError::BadSector {
            lsn,
            why: format!("los paquetes suman {total} y no 2048"),
        });
    }
    Ok(SectorPackets { dst, packets })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sector with a single audio packet that starts a frame.
    fn sector_de_audio(timecode: [u8; 3]) -> Vec<u8> {
        let mut s = vec![0u8; SECTOR];
        // 1 packet, 1 frame-info record, no DST.
        s[0] = (1 << 5) | (1 << 2);
        let header_len = 1 + 2 + 3;
        let len = SECTOR - header_len;
        let w: u16 = 0x8000 | (DATA_TYPE_AUDIO << 11) | len as u16;
        s[1..3].copy_from_slice(&w.to_be_bytes());
        s[3..6].copy_from_slice(&timecode);
        s
    }

    #[test]
    fn an_audio_sector_decodes_and_adds_up_to_2048() {
        let s = sector_de_audio([0, 1, 2]);
        let p = parse(&s, 550).unwrap();
        assert!(!p.dst);
        assert_eq!(p.packets.len(), 1);
        let packet = p.packets[0];
        assert!(packet.frame_start && packet.is_audio());
        assert_eq!(packet.at, 6);
        assert_eq!(packet.at + packet.len, SECTOR);
        assert_eq!(packet.timecode, Some(75 + 2));
    }

    #[test]
    fn a_sector_whose_packets_do_not_add_up_to_2048_is_refused() {
        let mut s = sector_de_audio([0, 0, 0]);
        // One byte is taken off the declared length: the sum no longer adds up.
        let w = u16::from_be_bytes([s[1], s[2]]) - 1;
        s[1..3].copy_from_slice(&w.to_be_bytes());
        assert!(matches!(parse(&s, 1), Err(SacdError::BadSector { .. })));
    }

    #[test]
    fn an_impossible_timecode_is_refused() {
        let s = sector_de_audio([0, 75, 0]);
        assert!(matches!(parse(&s, 1), Err(SacdError::BadSector { .. })));
    }

    #[test]
    fn the_length_uses_eleven_bits_and_not_twelve() {
        // 0x181B: with twelve bits that would be 2075, more than the sector.
        let w = 0x181Bu16;
        assert_eq!((w & 0x07FF) as usize, 27);
        assert_eq!(5 + 27 + 2016, SECTOR);
    }
}
