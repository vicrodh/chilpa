//! Offline verification rejects decoder errors that playback can recover from.

use super::*;

/// Decode the complete file without producing output or retaining PCM.
/// A declared duration and codec verification, when available, distinguish
/// a truncated tail from the normal end of a downloaded original.
pub fn file(
    mut file: File,
    extension: Option<&str>,
    cancelled: impl Fn() -> bool,
) -> Result<String, DecodeError> {
    let invalid = |why: &str| DecodeError::Corrupt(why.to_string());
    let container_frames = aiff_frames(&mut file)?;
    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let options = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let mut format = symphonia::default::get_probe()
        .format(&hint, source, &options, &MetadataOptions::default())
        .map_err(|error| DecodeError::Unsupported(error.to_string()))?
        .format;
    let track = format
        .default_track()
        .ok_or_else(|| invalid("no audio track"))?;
    let id = track.id;
    let expected = container_frames
        .or(track.codec_params.n_frames)
        .filter(|frames| *frames > 0)
        .ok_or_else(|| invalid("the audio length cannot be verified"))?;
    let codec = codec_name(track.codec_params.codec).to_string();
    let start = track.codec_params.start_ts;
    let time_base = track.codec_params.time_base;
    let mut decoder = codecs::registry()
        .make(&track.codec_params, &DecoderOptions { verify: true })
        .map_err(|error| DecodeError::Unsupported(error.to_string()))?;
    let mut frames = 0u64;
    let mut end = start;
    let mut sample_rate = 0;
    let lossless = matches!(codec.as_str(), "pcm" | "flac" | "alac");
    loop {
        if cancelled() {
            return Err(invalid("verification cancelled"));
        }
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(error) => return Err(invalid(&error.to_string())),
        };
        if packet.track_id() != id {
            continue;
        }
        let block = decoder
            .decode(&packet)
            .map_err(|error| invalid(&error.to_string()))?;
        // Lossy blocks may include priming and final padding. Their packet
        // durations describe the audible timeline after container trimming.
        sample_rate = block.spec().rate;
        let (minimum, _) = sample_bounds(packet.dur(), time_base, sample_rate)
            .ok_or_else(|| invalid("invalid audio time base"))?;
        if !lossless && (block.frames() as u64) < minimum {
            return Err(invalid("an audio packet is incomplete"));
        }
        if !lossless {
            if packet.ts() > end {
                return Err(invalid("the audio timeline has missing packets"));
            }
            let packet_end = packet
                .ts()
                .checked_add(packet.dur())
                .ok_or_else(|| invalid("audio duration overflow"))?;
            end = end.max(packet_end);
        }
        frames = frames
            .checked_add(block.frames() as u64)
            .ok_or_else(|| invalid("audio length overflow"))?;
    }
    let verification = decoder.finalize().verify_ok;
    let observed = if lossless {
        frames
    } else {
        end.saturating_sub(start)
    };
    let complete = if let Some(exact) = container_frames {
        frames == exact
    } else if lossless {
        let (minimum, maximum) = sample_bounds(expected, time_base, sample_rate)
            .ok_or_else(|| invalid("invalid audio time base"))?;
        (minimum..=maximum).contains(&frames)
    } else {
        observed == expected
    };
    if !complete || frames == 0 || verification == Some(false) {
        return Err(invalid(&format!("audio content is incomplete or corrupt: expected {expected} frames, observed {observed}, checksum {verification:?}")));
    }
    Ok(codec)
}

/// Container ticks and PCM samples may have different rates. Bounds allow
/// less than one tick of timestamp rounding and stay exact at sample resolution.
fn sample_bounds(
    ticks: u64,
    base: Option<symphonia::core::units::TimeBase>,
    rate: u32,
) -> Option<(u64, u64)> {
    if rate == 0 {
        return None;
    }
    let base = base.unwrap_or(symphonia::core::units::TimeBase::new(1, rate));
    if base.numer == 0 || base.denom == 0 {
        return None;
    }
    let scale = u128::from(base.numer) * u128::from(rate);
    let denominator = u128::from(base.denom);
    let duration = u128::from(ticks) * scale;
    let rounding = scale.max(denominator) - 1;
    let minimum = duration.saturating_sub(rounding).div_ceil(denominator);
    let maximum = duration.saturating_add(rounding) / denominator;
    Some((minimum.try_into().ok()?, maximum.try_into().ok()?))
}

/// AIFF's COMM carries its exact sample count. The demuxer's duration also
/// counts the SSND offset header, so it is not a sample-count authority.
fn aiff_frames(file: &mut File) -> Result<Option<u64>, DecodeError> {
    use std::io::{Read, Seek, SeekFrom};
    let invalid = || DecodeError::Corrupt("incomplete AIFF container".into());
    let length = file.metadata().map_err(DecodeError::Open)?.len();
    let mut header = [0u8; 12];
    let count = file.read(&mut header).map_err(DecodeError::Open)?;
    file.rewind().map_err(DecodeError::Open)?;
    if count < 12 || &header[..4] != b"FORM" || !matches!(&header[8..12], b"AIFF" | b"AIFC") {
        return Ok(None);
    }
    let end = u64::from(u32::from_be_bytes(header[4..8].try_into().unwrap())) + 8;
    if end > length || end < 12 {
        return Err(invalid());
    }
    let mut at = 12u64;
    while at.saturating_add(8) <= end {
        file.seek(SeekFrom::Start(at)).map_err(DecodeError::Open)?;
        let mut chunk = [0u8; 8];
        file.read_exact(&mut chunk).map_err(|_| invalid())?;
        let size = u64::from(u32::from_be_bytes(chunk[4..8].try_into().unwrap()));
        if at.saturating_add(8).saturating_add(size) > end {
            return Err(invalid());
        }
        if &chunk[..4] == b"COMM" {
            if size < 18 {
                return Err(invalid());
            }
            let mut common = [0u8; 6];
            file.read_exact(&mut common).map_err(|_| invalid())?;
            let frames = u64::from(u32::from_be_bytes(common[2..6].try_into().unwrap()));
            file.rewind().map_err(DecodeError::Open)?;
            return Ok(Some(frames));
        }
        at = at
            .saturating_add(8)
            .saturating_add(size)
            .saturating_add(size % 2);
    }
    Err(invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, Write};

    fn wave(frames: u32) -> Vec<u8> {
        let size = frames * 4;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + size).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&44_100u32.to_le_bytes());
        bytes.extend_from_slice(&176_400u32.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.resize(44 + size as usize, 0);
        bytes
    }

    #[test]
    fn verification_rejects_a_truncated_tail_that_the_old_first_block_check_accepted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("silence.wav");
        let complete = wave(44_100);
        std::fs::write(&path, &complete).unwrap();
        assert_eq!(
            file(File::open(&path).unwrap(), Some("wav"), || false).unwrap(),
            "pcm"
        );
        std::fs::write(&path, &complete[..complete.len() - 400]).unwrap();
        let mut old = Decoded::open_file(File::open(&path).unwrap(), Some("wav")).unwrap();
        assert!(
            old.next_block().unwrap().is_some(),
            "the previous first-block validator accepted these truncated bytes"
        );
        assert!(file(File::open(&path).unwrap(), Some("wav"), || false).is_err());
        let mut output = File::options().write(true).open(&path).unwrap();
        output.rewind().unwrap();
        output.write_all(b"HTML").unwrap();
        assert!(file(File::open(&path).unwrap(), Some("wav"), || false).is_err());
    }
}
