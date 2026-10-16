//! How something sounds, in terms that depend on no service.

/// Format, bit depth, rate and channels.
///
/// In QBZ this was a `hires: bool` plus loose fields, and the UI labelled
/// outputs bit-perfect that were not. Here there are no labels: there are
/// numbers (ADR-004).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AudioQuality {
    /// `flac`, `alac`, `opus`, `dsd`, `pcm`…
    pub codec: String,
    /// `None` for lossy formats, where it means nothing.
    pub bit_depth: Option<u8>,
    pub sample_rate_hz: u32,
    pub channels: u8,
}

impl AudioQuality {
    /// True when it keeps the original signal. A lossy format never does, which
    /// is why it declares no bit depth either.
    pub fn is_lossless(&self) -> bool {
        self.bit_depth.is_some()
    }

    /// Bits per second of the equivalent PCM. Useful for comparing two versions
    /// of the same track; it is not the file's bitrate.
    pub fn rank(&self) -> u64 {
        u64::from(self.bit_depth.unwrap_or(0))
            * u64::from(self.sample_rate_hz)
            * u64::from(self.channels)
    }

    /// True when both can go out the same device without reconfiguring it.
    /// Changing rate or format between tracks forces a renegotiation of the
    /// output, which is where QBZ put clicks and silent resampling.
        pub fn same_stream_format(&self, other: &Self) -> bool {
        self.sample_rate_hz == other.sample_rate_hz
            && self.bit_depth == other.bit_depth
            && self.channels == other.channels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(codec: &str, bits: Option<u8>, hz: u32) -> AudioQuality {
        AudioQuality { codec: codec.into(), bit_depth: bits, sample_rate_hz: hz, channels: 2 }
    }

    #[test]
    fn anything_lossy_is_not_lossless() {
        assert!(q("flac", Some(24), 96_000).is_lossless());
        assert!(!q("opus", None, 48_000).is_lossless());
    }

    #[test]
    fn hi_res_ranks_above_cd() {
        assert!(q("flac", Some(24), 96_000).rank() > q("flac", Some(16), 44_100).rank());
    }

    #[test]
    fn lossy_never_beats_anything_lossless() {
        assert!(q("opus", None, 48_000).rank() < q("flac", Some(16), 44_100).rank());
    }

    #[test]
    fn the_same_format_does_not_renegotiate_the_output() {
        assert!(q("flac", Some(16), 44_100).same_stream_format(&q("alac", Some(16), 44_100)));
        assert!(!q("flac", Some(16), 44_100).same_stream_format(&q("flac", Some(16), 48_000)));
    }
}
