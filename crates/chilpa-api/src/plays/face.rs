//! What names a track that starts: what its file says of itself, and the
//! cards its play was launched with.
//!
//! A track that came from a search or from a source's listing carries a card
//! the decoder knows nothing of: without it the window would show the raw
//! address. The cards are the play's own, taken as it was launched, so a
//! track of a play that has since been replaced is still named by what it
//! was, not by what the listener chose after it. Nothing here reads the
//! window.

use std::path::Path;

use crate::search::Hit;
use crate::source_catalog::Catalog;
use crate::sources::Sounding;

/// The cards a play was launched with.
#[derive(Clone, Default)]
pub(crate) struct Cards {
    /// What a search found, by the address it plays from.
    pub search: Option<(String, Hit)>,
    /// What was started from a source's listing, with its picture once it
    /// is on disk.
    pub entry: Option<Sounding>,
}

/// How a track that starts is shown and logged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Face {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// The cover on disk, or empty.
    pub cover: String,
    /// It has no end: a station.
    pub live: bool,
    /// What its source says of how it was made, when it says.
    pub ai_generated: Option<bool>,
}

/// The face of the track at `path`, with the tags and the cover its file
/// gave.
pub(crate) fn face(path: &Path, tags: Option<&chilpa_tags::Tags>, artwork: Option<&Path>, cards: &Cards, catalog: &Catalog) -> Face {
    let address = path.display().to_string();
    let name = path.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
    let mut face = Face {
        title: tags.and_then(|tags| tags.title.clone()).unwrap_or(name),
        artist: tags.map(|tags| tags.artist_line()).unwrap_or_default(),
        album: tags.and_then(|tags| tags.album.clone()).unwrap_or_default(),
        cover: artwork.map(|cover| cover.display().to_string()).unwrap_or_default(),
        live: false,
        ai_generated: catalog.track_by_path(&address).and_then(|(track, _)| track.metadata["ai_generated"].as_bool()),
    };
    // What came from a search already carries its card.
    if let Some((_, hit)) = cards.search.as_ref().filter(|(url, _)| *url == address) {
        face.title = hit.title.clone();
        face.artist = hit.artist.clone();
        face.album = hit.album.clone();
    }
    // And what was started from a listing, the card it was listed with.
    if let Some(sounding) = cards.entry.as_ref().filter(|sounding| sounding.url == address) {
        face.title = sounding.entry.title.clone();
        face.artist = sounding.entry.subtitle.clone();
        face.album = String::new();
        if face.cover.is_empty() {
            face.cover = sounding.picture.clone();
        }
        face.live = sounding.entry.live;
    }
    face
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_plugin::messages::Entry;

    fn catalog() -> std::sync::Arc<Catalog> {
        crate::Chilpa::with_listed(Vec::new(), Vec::new()).catalog.clone()
    }

    fn hit(title: &str) -> Hit {
        Hit { title: title.into(), artist: "Somebody".into(), album: "A record".into(), ..Default::default() }
    }

    fn listed(url: &str, title: &str, picture: &str, live: bool) -> Sounding {
        let mut card: Sounding = serde_json::from_value(serde_json::json!({ "url": url, "source": "example.radio", "entry": Entry::default(), "picture": picture })).unwrap();
        card.entry.title = title.into();
        card.entry.subtitle = "Its city".into();
        card.entry.live = live;
        card
    }

    #[test]
    fn a_bare_address_found_by_a_search_is_named_by_its_card() {
        let address = "https://cdn.example/audio/0f3a9c";
        let cards = Cards { search: Some((address.into(), hit("The song"))), entry: None };
        let face = face(Path::new(address), None, None, &cards, &catalog());
        assert_eq!((face.title.as_str(), face.artist.as_str(), face.album.as_str()), ("The song", "Somebody", "A record"), "not the address, which is all the decoder knows");
        assert!(!face.live && face.cover.is_empty());
    }

    #[test]
    fn what_was_started_from_a_listing_is_named_by_the_card_it_was_listed_with() {
        let address = "https://radio.example/stream";
        let cards = Cards { search: None, entry: Some(listed(address, "The station", "/covers/station.jpg", true)) };
        let tags = chilpa_tags::Tags { title: Some("icy".into()), album: Some("ignored".into()), ..Default::default() };
        let named = face(Path::new(address), Some(&tags), None, &cards, &catalog());
        assert_eq!((named.title.as_str(), named.artist.as_str(), named.album.as_str()), ("The station", "Its city", ""));
        assert_eq!(named.cover, "/covers/station.jpg", "its own picture, when the file brings no cover");
        assert!(named.live);
        let with_cover = face(Path::new(address), None, Some(Path::new("/covers/embedded.jpg")), &cards, &catalog());
        assert_eq!(with_cover.cover, "/covers/embedded.jpg", "the file's own cover stays");
    }

    #[test]
    fn a_track_that_is_not_the_cards_is_named_by_its_file() {
        let cards = Cards { search: Some(("https://cdn.example/other".into(), hit("Another"))), entry: Some(listed("https://radio.example/other", "Another station", "/covers/other.jpg", true)) };
        let tags = chilpa_tags::Tags { title: Some("Tagged".into()), artists: vec!["A".into(), "B".into()], album: Some("Record".into()), ..Default::default() };
        let tagged = face(Path::new("/music/01 - file.flac"), Some(&tags), None, &cards, &catalog());
        assert_eq!((tagged.title.as_str(), tagged.artist.as_str(), tagged.album.as_str()), ("Tagged", "A, B", "Record"));
        let bare = face(Path::new("/music/01 - file.flac"), None, None, &cards, &catalog());
        assert_eq!((bare.title.as_str(), bare.artist.as_str(), bare.cover.as_str()), ("01 - file", "", ""));
        assert!(!bare.live);
    }
}
