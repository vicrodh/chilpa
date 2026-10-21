//! What a file is, said the way a list shows it.
//!
//! The window colors one icon by tier and writes a short label next to it
//! ("FLAC 24/96"). Both are decided here, not in QML: the bits are the ones
//! that carry the signal, which is what the file and the engine say, and
//! never the width of the word the device was opened with. A 16 bit rip
//! going out in 32 bit words is still 16 bits, and saying 24 or 32 is the
//! bug that already shipped once.

/// How good a file is, as the quality icon is colored.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tier {
    #[default]
    Unknown,
    /// MP3, AAC, Ogg, Opus: what was thrown away is gone.
    Lossy,
    /// 16 bits at 48 kHz or less.
    Lossless,
    /// 24 bits or more, or above 48 kHz.
    HiRes,
    /// DSD64 and up.
    Dsd,
}

impl Tier {
    /// As a number, for a model role: 0 unknown to 4 DSD.
    pub fn rank(self) -> i32 {
        self as i32
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Quality {
    /// "FLAC", "ALAC", "MP3", "DSD". Empty when it is not known.
    pub format: String,
    /// The bits that carry the signal. `None` on a lossy file.
    pub bits: Option<u8>,
    pub rate_hz: Option<u32>,
    pub tier: Tier,
}

/// The lowest DSD rate, DSD64: 64 times 44.1 kHz.
const DSD64: u32 = 2_822_400;

impl Quality {
    /// A file's quality from its extension and what its header states.
    pub fn of_file(path: &str, bits: Option<u8>, rate_hz: Option<u32>) -> Quality {
        let ext = std::path::Path::new(path)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let (format, lossy) = match ext.as_str() {
            "flac" => ("FLAC", false),
            "alac" => ("ALAC", false),
            // One container, two codecs: only the lossless one states bits.
            "m4a" | "mp4" if bits.is_some() => ("ALAC", false),
            "m4a" | "mp4" | "aac" => ("AAC", true),
            "mp3" => ("MP3", true),
            "ogg" | "oga" => ("Ogg", true),
            "opus" => ("Opus", true),
            "wav" => ("WAV", false),
            "aif" | "aiff" => ("AIFF", false),
            "wv" => ("WavPack", false),
            "ape" => ("APE", false),
            "dsf" | "dff" | "dsd" | "iso" => ("DSD", false),
            _ => ("", bits.is_none()),
        };
        let dsd = format == "DSD" || rate_hz.is_some_and(|r| r >= DSD64);
        let tier = if dsd {
            Tier::Dsd
        } else if format.is_empty() && bits.is_none() {
            Tier::Unknown
        } else if lossy {
            Tier::Lossy
        } else if bits.is_some_and(|b| b >= 24) || rate_hz.is_some_and(|r| r > 48_000) {
            Tier::HiRes
        } else {
            Tier::Lossless
        };
        Quality {
            format: if dsd { "DSD".to_string() } else { format.to_string() },
            bits: if lossy || dsd { None } else { bits },
            rate_hz,
            tier,
        }
    }

    /// A track's quality from what the library knows of it. A server's track
    /// is named by its queue entry, which has no extension; its codec says
    /// what the extension would (ADR-017).
    pub fn of_track(path: &str, codec: Option<&str>, bits: Option<u8>, rate_hz: Option<u32>) -> Quality {
        if !chilpa_models::RemoteEntry::is_remote(path) {
            return Self::of_file(path, bits, rate_hz);
        }
        Self::of_codec(codec.unwrap_or(""), bits, rate_hz)
    }

    /// A quality from a codec's name alone, for what has no file name.
    pub fn of_codec(codec: &str, bits: Option<u8>, rate_hz: Option<u32>) -> Quality {
        let ext = match codec {
            "flac" => "flac",
            "alac" => "alac",
            "aac" => "aac",
            "mp3" => "mp3",
            "vorbis" | "ogg" => "ogg",
            "opus" => "opus",
            "wav" | "pcm" => "wav",
            "aiff" => "aiff",
            "wavpack" => "wv",
            "ape" => "ape",
            "dsd" | "dsf" | "dff" => "dsf",
            _ => "",
        };
        Self::of_file(&format!("track.{ext}"), bits, rate_hz)
    }

    /// For a list column: "FLAC 24/96", "DSD64", "MP3".
    pub fn short(&self) -> String {
        if self.tier == Tier::Dsd {
            return self.dsd_name();
        }
        match (self.bits, self.rate_hz) {
            (Some(bits), Some(rate)) => format!("{} {bits}/{}", self.format, khz(rate)),
            _ => self.format.clone(),
        }
    }

    /// Where there is little room: "FLAC 24-96", "DSD64".
    pub fn brief(&self) -> String {
        if self.tier == Tier::Dsd {
            return self.dsd_name();
        }
        match (self.bits, self.rate_hz) {
            (Some(bits), Some(rate)) => format!("{} {bits}-{}", self.format, khz(rate)),
            (None, Some(rate)) if !self.format.is_empty() => format!("{} {}", self.format, khz(rate)),
            _ => self.format.clone(),
        }
    }

    /// For a record's header: "FLAC · 24-bit / 192 kHz".
    pub fn long(&self) -> String {
        if self.tier == Tier::Dsd {
            return self.dsd_name();
        }
        match (self.bits, self.rate_hz) {
            (Some(bits), Some(rate)) => format!("{} · {bits}-bit / {} kHz", self.format, khz(rate)),
            (None, Some(rate)) if !self.format.is_empty() => format!("{} · {} kHz", self.format, khz(rate)),
            _ => self.format.clone(),
        }
    }

    fn dsd_name(&self) -> String {
        match self.rate_hz {
            Some(r) if r >= DSD64 => format!("DSD{}", r / 44_100),
            _ => "DSD".to_string(),
        }
    }
}

/// 44100 as "44.1", 96000 as "96".
fn khz(rate: u32) -> String {
    if rate % 1000 == 0 {
        (rate / 1000).to_string()
    } else {
        format!("{:.1}", f64::from(rate) / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_codec_names_the_format_of_what_has_no_file_name() {
        assert_eq!(Quality::of_codec("flac", Some(16), Some(44_100)).brief(), "FLAC 16-44.1");
        assert_eq!(Quality::of_codec("flac", Some(24), Some(96_000)).long(), "FLAC · 24-bit / 96 kHz");
    }

    #[test]
    fn an_explicit_alac_codec_does_not_require_known_resolution() {
        let q = Quality::of_codec("alac", None, None);
        assert_eq!((q.short(), q.tier, q.bits, q.rate_hz), ("ALAC".into(), Tier::Lossless, None, None));
        let q = Quality::of_codec("alac", None, Some(96_000));
        assert_eq!((q.format.as_str(), q.tier, q.bits, q.rate_hz), ("ALAC", Tier::HiRes, None, Some(96_000)));
        assert_eq!(Quality::of_codec("alac", Some(24), Some(48_000)).short(), "ALAC 24/48");
        assert_eq!(Quality::of_codec("aac", None, None).tier, Tier::Lossy);
    }

    #[test]
    fn a_cd_rip_is_lossless_and_says_16_bits() {
        let q = Quality::of_file("/m/a.flac", Some(16), Some(44_100));
        assert_eq!(q.tier, Tier::Lossless);
        assert_eq!(q.short(), "FLAC 16/44.1");
        assert_eq!(q.long(), "FLAC · 16-bit / 44.1 kHz");
    }

    #[test]
    fn more_bits_or_a_higher_rate_is_hi_res() {
        assert_eq!(Quality::of_file("a.flac", Some(24), Some(48_000)).tier, Tier::HiRes);
        assert_eq!(Quality::of_file("a.flac", Some(16), Some(96_000)).tier, Tier::HiRes);
        assert_eq!(Quality::of_file("a.flac", Some(24), Some(192_000)).short(), "FLAC 24/192");
    }

    #[test]
    fn a_lossy_file_carries_no_bits_even_if_a_decoder_reports_some() {
        let q = Quality::of_file("a.mp3", Some(32), Some(44_100));
        assert_eq!((q.tier, q.bits), (Tier::Lossy, None));
        assert_eq!(q.short(), "MP3");
    }

    #[test]
    fn a_vorbis_pcm_reference_does_not_turn_the_source_into_hi_res() {
        for q in [
            Quality::of_file("track.ogg", Some(32), Some(96_000)),
            Quality::of_file("track.oga", Some(32), Some(44_100)),
            Quality::of_codec("vorbis", Some(32), Some(48_000)),
        ] {
            assert_eq!((q.tier, q.bits), (Tier::Lossy, None));
            assert_eq!(q.short(), "Ogg");
        }
    }

    #[test]
    fn the_same_container_is_alac_with_bits_and_aac_without() {
        assert_eq!(Quality::of_file("a.m4a", Some(24), Some(96_000)).short(), "ALAC 24/96");
        let aac = Quality::of_file("a.m4a", None, Some(44_100));
        assert_eq!((aac.format.as_str(), aac.tier), ("AAC", Tier::Lossy));
    }

    #[test]
    fn dsd_is_named_by_its_multiple() {
        let q = Quality::of_file("a.dsf", Some(1), Some(2_822_400));
        assert_eq!((q.tier, q.short()), (Tier::Dsd, "DSD64".to_string()));
        assert_eq!(Quality::of_file("a.dff", None, Some(5_644_800)).short(), "DSD128");
        assert_eq!(Quality::of_file("a.dsf", None, None).short(), "DSD");
    }

    #[test]
    fn what_cannot_be_told_says_nothing() {
        let q = Quality::of_file("a.xyz", None, None);
        assert_eq!((q.tier, q.short()), (Tier::Unknown, String::new()));
    }
}

#[cfg(test)]
mod remote_tests {
    use super::*;

    #[test]
    fn a_server_s_track_is_labelled_by_its_codec() {
        let entry = "remote:example-server/aaaa/0Hj5";
        assert_eq!(Quality::of_track(entry, Some("flac"), Some(24), Some(96_000)).short(), "FLAC 24/96");
        assert_eq!(Quality::of_track(entry, Some("alac"), Some(16), Some(48_000)).short(), "ALAC 16/48");
        assert_eq!(Quality::of_track(entry, Some("opus"), None, Some(48_000)).tier, Tier::Lossy);
        assert_eq!(Quality::of_track("/m/a.flac", Some("mp3"), Some(16), Some(44_100)).format, "FLAC", "a file is named by its extension");
    }
}
