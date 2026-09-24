//! A CD's table of contents, and the arithmetic it needs.
//!
//! The TOC carries no durations: each track runs to the next one, and the
//! last to the lead-out. That is right on an audio-only disc and **wrong on
//! a CD-Extra**, where a data session follows the audio: between the two
//! sessions there is a gap of 11,400 sectors, some 152 seconds, that is not
//! music.
//!
//! The QBZ audit found that bug: the last audio track ran all the way to the
//! start of the data track, so the rip failed and the disc gave different
//! identifiers across systems.

/// Sectors per second.
pub const SECTORS_PER_SEC: u32 = 75;
/// The gap between two sessions: the first one's lead-out plus the second
/// one's lead-in.
pub const SESSION_GAP_SECTORS: u32 = 11_400;
/// Bytes in an audio sector: 588 stereo frames of 16 bits.
pub const SECTOR_BYTES: usize = 2352;

/// One TOC entry exactly as the drive hands it over.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TocEntry {
    pub number: u8,
    pub start_lsn: u32,
    pub is_audio: bool,
    /// Which session it lives in. An audio only disc has everything in 1.
    pub session: u8,
}

/// A track with its duration already computed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Track {
    pub number: u8,
    pub start_lsn: u32,
    pub sectors: u32,
    pub is_audio: bool,
    pub session: u8,
}

impl Track {
    pub fn seconds(&self) -> f64 {
        f64::from(self.sectors) / f64::from(SECTORS_PER_SEC)
    }

    pub fn end_lsn(&self) -> u32 {
        self.start_lsn + self.sectors
    }
}

/// The whole disc.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Toc {
    pub tracks: Vec<Track>,
    pub leadout_lsn: u32,
}

impl Toc {
    pub fn audio_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|t| t.is_audio)
    }

    pub fn track(&self, number: u8) -> Option<&Track> {
        self.tracks.iter().find(|t| t.number == number)
    }

    /// True if the disc mixes audio and data, that is a CD-Extra or a mixed
    /// mode disc.
    pub fn is_mixed(&self) -> bool {
        self.tracks.iter().any(|t| t.is_audio) && self.tracks.iter().any(|t| !t.is_audio)
    }
}

/// Builds the TOC, computing durations.
///
/// The entries come in order. When the next track is in another session,
/// the gap between sessions is discounted instead of counted as music.
pub fn build(entries: &[TocEntry], leadout_lsn: u32) -> Toc {
    let mut tracks = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let next = entries.get(i + 1);
        let fin = match next {
            Some(s) if s.session != e.session => s.start_lsn.saturating_sub(SESSION_GAP_SECTORS),
            Some(s) => s.start_lsn,
            None => leadout_lsn,
        };
        tracks.push(Track {
            number: e.number,
            start_lsn: e.start_lsn,
            sectors: fin.saturating_sub(e.start_lsn),
            is_audio: e.is_audio,
            session: e.session,
        });
    }
    Toc { tracks, leadout_lsn }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(number: u8, start: u32, session: u8) -> TocEntry {
        TocEntry { number, start_lsn: start, is_audio: true, session }
    }

    fn data(number: u8, start: u32, session: u8) -> TocEntry {
        TocEntry { number, start_lsn: start, is_audio: false, session }
    }

    #[test]
    fn each_track_runs_to_the_next_and_the_last_one_to_the_lead_out() {
        let toc = build(&[audio(1, 0, 1), audio(2, 15_000, 1)], 30_000);
        assert_eq!(toc.tracks[0].sectors, 15_000);
        assert_eq!(toc.tracks[1].sectors, 15_000);
        assert_eq!(toc.tracks[1].end_lsn(), 30_000);
        assert!(!toc.is_mixed());
    }

    #[test]
    fn on_a_cd_extra_the_last_audio_track_does_not_swallow_the_session_gap() {
        // The audit's finding: without discounting the gap, track 2 measured
        // 152 seconds too long and the rip failed.
        let toc = build(&[audio(1, 0, 1), audio(2, 15_000, 1), data(3, 41_400, 2)], 60_000);
        assert_eq!(toc.tracks[1].sectors, 41_400 - SESSION_GAP_SECTORS - 15_000);
        assert_eq!(toc.tracks[1].end_lsn(), 30_000);
        assert!(toc.is_mixed());
    }

    #[test]
    fn a_mixed_mode_disc_in_one_session_subtracts_nothing() {
        // Data first and audio after, all in session 1: there is no gap to
        // discount here.
        let toc = build(&[data(1, 0, 1), audio(2, 20_000, 1)], 40_000);
        assert_eq!(toc.tracks[0].sectors, 20_000);
        assert_eq!(toc.tracks[1].sectors, 20_000);
    }

    #[test]
    fn the_duration_comes_out_in_seconds_at_75_sectors() {
        let toc = build(&[audio(1, 0, 1)], 750);
        assert_eq!(toc.tracks[0].seconds(), 10.0);
    }

    #[test]
    fn only_audio_tracks_are_listed() {
        let toc = build(&[audio(1, 0, 1), audio(2, 15_000, 1), data(3, 41_400, 2)], 60_000);
        assert_eq!(toc.audio_tracks().count(), 2);
        assert_eq!(toc.track(3).unwrap().is_audio, false);
    }
}
