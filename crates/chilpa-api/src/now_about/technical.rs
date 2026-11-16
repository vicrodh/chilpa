//! What a track is as audio: its file's own word when the file is here, and
//! what the library, its source or the player stored when it is not. What is
//! not known is left out, never said empty.

use crate::{Quality, Tier};
use chilpa_i18n as i18n;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Technical {
    pub quality: Quality,
    pub channels: Option<u8>,
    pub duration_ms: u64,
    pub bytes: Option<u64>,
}

impl Technical {
    /// What the file says of itself, over what was stored. A disc image's
    /// size is the whole disc's, not the track's, so it is not taken.
    pub(super) fn read_file(&mut self, path: &str, file: &chilpa_tags::FileInfo) {
        let quality = Quality::of_file(path, file.bit_depth, file.sample_rate_hz);
        if !quality.format.is_empty() || quality.rate_hz.is_some() { self.quality = quality; }
        if file.channels.is_some_and(|channels| channels > 0) { self.channels = file.channels; }
        let duration = u64::try_from(file.duration.as_millis()).unwrap_or(u64::MAX);
        if duration > 0 { self.duration_ms = duration; }
        if !path.starts_with("sacd:") {
            if let Some(size) = std::fs::metadata(path).ok().filter(std::fs::Metadata::is_file) { self.bytes = Some(size.len()); }
        }
    }

    /// As the answer lists them: a catalog key and its value.
    pub(super) fn facts(&self) -> Vec<(&'static str, String)> {
        let quality = &self.quality;
        let format = if quality.tier == Tier::Dsd { quality.short() } else { quality.format.clone() };
        let mut facts = Vec::new();
        if !format.is_empty() { facts.push((i18n::mark("Format"), format)); }
        if let Some(bits) = quality.bits { facts.push((i18n::mark("Bit depth"), bits.to_string())); }
        if let Some(rate) = quality.rate_hz.filter(|rate| *rate > 0) { facts.push((i18n::mark("Sample rate"), rate_text(rate))); }
        if let Some(channels) = self.channels.filter(|channels| *channels > 0) { facts.push((i18n::mark("Channels"), channels.to_string())); }
        if self.duration_ms > 0 { facts.push((i18n::mark("Duration"), crate::format::clock(self.duration_ms))); }
        if let Some(bytes) = self.bytes { facts.push((i18n::mark("File size"), format!("{:.1} MiB", bytes as f64 / 1_048_576.0))); }
        facts
    }
}

/// 44100 as "44.1 kHz", 96000 as "96 kHz", a DSD rate in MHz.
fn rate_text(rate: u32) -> String {
    let (value, unit) = if rate >= 1_000_000 { (f64::from(rate) / 1e6, "MHz") } else { (f64::from(rate) / 1e3, "kHz") };
    let digits = format!("{value:.4}");
    format!("{} {unit}", digits.trim_end_matches('0').trim_end_matches('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(facts: &[(&'static str, String)]) -> Vec<(String, String)> {
        facts.iter().map(|(what, value)| (what.to_string(), value.clone())).collect()
    }

    #[test]
    fn a_hi_res_file_says_its_format_bits_rate_channels_length_and_size() {
        let facts = Technical { quality: Quality::of_file("/m/a.flac", Some(24), Some(96_000)), channels: Some(2),
            duration_ms: 241_000, bytes: Some(47_185_920) }.facts();
        assert_eq!(named(&facts), [("Format".into(), "FLAC".into()), ("Bit depth".into(), "24".into()),
            ("Sample rate".into(), "96 kHz".into()), ("Channels".into(), "2".into()), ("Duration".into(), "4:01".into()),
            ("File size".into(), "45.0 MiB".into())]);
    }

    #[test]
    fn dsd_and_lossy_say_what_they_have_and_nothing_is_said_empty() {
        let dsd = Technical { quality: Quality::of_file("/m/a.dsf", Some(1), Some(2_822_400)), ..Default::default() }.facts();
        assert_eq!(named(&dsd), [("Format".into(), "DSD64".into()), ("Sample rate".into(), "2.8224 MHz".into())]);
        let lossy = Technical { quality: Quality::of_file("/m/a.mp3", None, Some(44_100)), ..Default::default() }.facts();
        assert_eq!(named(&lossy), [("Format".into(), "MP3".into()), ("Sample rate".into(), "44.1 kHz".into())]);
        assert!(Technical::default().facts().is_empty(), "an unknown track says nothing technical");
    }
}
