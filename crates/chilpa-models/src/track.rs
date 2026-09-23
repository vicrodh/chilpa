//! Tracks, álbumes y artistas, vistos desde cualquier source.

use crate::artwork::ArtworkRef;
use crate::source::{Availability, SourceId};
use std::time::Duration;

/// La identidad de un track: el source y el id que ese source le da.
///
/// El id externo es opaco: Chilpa no lo interpreta ni asume que sea un
/// número. En QBZ el id era un `u64` compartido entre orígenes, y dos tracks
/// de sources distintos podían colisionar.
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

/// Un artista, con su id externo cuando el source lo da.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ArtistRef {
    pub name: String,
    pub external_id: Option<String>,
    /// MusicBrainz artist id, cuando se conoce.
    pub mbid: Option<String>,
}

impl ArtistRef {
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into(), external_id: None, mbid: None }
    }
}

/// El álbum al que pertenece un track.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AlbumRef {
    pub title: String,
    pub external_id: Option<String>,
    /// MusicBrainz release id, la llave para juntar el mismo álbum entre
    /// sources (ADR-003).
    pub mbid: Option<String>,
    pub disc_number: Option<u16>,
}

impl AlbumRef {
    pub fn titled(title: impl Into<String>) -> Self {
        Self { title: title.into(), external_id: None, mbid: None, disc_number: None }
    }
}

/// Un track con lo que la UI necesita para mostrarlo y el player para
/// reproducirlo.
///
/// `availability` puede traer varios sources para el mismo track: el archivo
/// local, un servidor y un plugin. Quién gana lo decide
/// [`crate::source::preferred`] con la preferencia del usuario.
#[derive(Clone, PartialEq, Debug)]
pub struct Track {
    pub id: TrackRef,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: Option<AlbumRef>,
    pub duration: Duration,
    pub track_number: Option<u16>,
    pub artwork: ArtworkRef,
    /// La llave que liga el mismo track entre sources, junto con el MBID de
    /// grabación. No son identidad por sí solos.
    pub isrc: Option<String>,
    pub recording_mbid: Option<String>,
    pub availability: Vec<Availability>,
}

impl Track {
    /// Un track mínimo de un source, sin más datos que su título.
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

    /// Los artistas como un solo texto, para las vistas que muestran una
    /// línea.
    pub fn artist_line(&self) -> String {
        self.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
    }

    /// Cierto si este track y el otro son la misma grabación, aunque vengan
    /// de sources distintos. Se emparejan por MBID de grabación o por ISRC;
    /// nunca por título.
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
