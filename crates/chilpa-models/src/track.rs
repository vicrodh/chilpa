//! Tracks, albums and artists, as seen from any source.

use crate::artwork::ArtworkRef;
use crate::source::{Availability, SourceId};
use std::time::Duration;

/// A track's identity: the source and the id that source gives it.
///
/// The external id is opaque: Chilpa neither interprets it nor assumes it is
/// a number. In QBZ the id was a `u64` shared across origins, and two tracks
/// from different sources could collide.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TrackRef {
    pub source: SourceId,
    pub external_id: String,
}

impl TrackRef {
    pub fn new(source: SourceId, external_id: impl Into<String>) -> Self {
        Self { source, external_id: external_id.into() }
    }
}

/// An artist, with their external id when the source gives one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ArtistRef {
    pub name: String,
    pub external_id: Option<String>,
    /// MusicBrainz artist id, when it is known.
    pub mbid: Option<String>,
}

impl ArtistRef {
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into(), external_id: None, mbid: None }
    }
}

/// The album a track belongs to.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AlbumRef {
    pub title: String,
    pub external_id: Option<String>,
    /// MusicBrainz release id, the key for joining the same album across
    /// sources (ADR-003).
    pub mbid: Option<String>,
    pub disc_number: Option<u16>,
}

impl AlbumRef {
    pub fn titled(title: impl Into<String>) -> Self {
        Self { title: title.into(), external_id: None, mbid: None, disc_number: None }
    }
}

/// A track with what the UI needs to show it and the player needs to play
/// it.
///
/// `availability` can carry several sources for the same track: the local
/// file, a server and a plugin. Who wins is decided by
/// [`crate::source::preferred`] with the user's preference.
#[derive(Clone, PartialEq, Debug)]
pub struct Track {
    pub id: TrackRef,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: Option<AlbumRef>,
    pub duration: Duration,
    pub track_number: Option<u16>,
    pub artwork: ArtworkRef,
    /// The key that links the same track across sources, together with the
    /// recording MBID. On their own they are not identity.
    pub isrc: Option<String>,
    pub recording_mbid: Option<String>,
    pub availability: Vec<Availability>,
}

impl Track {
    /// A minimal track from a source, with no data beyond its title.
    pub fn new(id: TrackRef, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            artists: Vec::new(),
            album: None,
            duration: Duration::ZERO,
            track_number: None,
            artwork: ArtworkRef::None,
            isrc: None,
            recording_mbid: None,
            availability: Vec::new(),
        }
    }

    /// The artists as a single string, for views that show one line.
    pub fn artist_line(&self) -> String {
        self.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
    }

    /// True if this track and the other are the same recording, even coming
    /// from different sources. They are matched by recording MBID or by
    /// ISRC; never by title.
    pub fn same_recording(&self, other: &Self) -> bool {
        match (&self.recording_mbid, &other.recording_mbid) {
            (Some(a), Some(b)) if a == b => return true,
            _ => {}
        }
        match (&self.isrc, &other.isrc) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    pub fn is_playable(&self) -> bool {
        self.availability.iter().any(Availability::is_playable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::AudioQuality;
    use crate::source::Reachability;

    fn track(source: &str, id: &str) -> Track {
        Track::new(TrackRef::new(SourceId::new(source, "cuenta-1"), id), "Canción")
    }

    #[test]
    fn the_same_id_on_different_sources_is_not_the_same_track() {
        assert_ne!(track("example-source", "1").id, track("otro-source", "1").id);
    }

    #[test]
    fn the_isrc_matches_across_sources() {
        let mut a = track("example-source", "1");
        let mut b = track("otro-source", "99");
        a.isrc = Some("MXABC0000001".into());
        b.isrc = Some("MXABC0000001".into());
        assert!(a.same_recording(&b));
    }

    #[test]
    fn without_keys_nothing_matches_even_if_the_title_does() {
        assert!(!track("example-source", "1").same_recording(&track("otro-source", "2")));
    }

    #[test]
    fn a_track_only_on_an_unreachable_source_is_not_playable() {
        let mut t = track("example-source", "1");
        t.availability = vec![Availability {
            source: t.id.source.clone(),
            reachability: Reachability::Unreachable,
            quality: Some(AudioQuality {
                codec: "flac".into(),
                bit_depth: Some(16),
                sample_rate_hz: 44_100,
                channels: 2,
            }),
        }];
        assert!(!t.is_playable());
    }

    #[test]
    fn the_artist_line_joins_them_all() {
        let mut t = track("local", "1");
        t.artists = vec![ArtistRef::named("Uno"), ArtistRef::named("Dos")];
        assert_eq!(t.artist_line(), "Uno, Dos");
    }
}
