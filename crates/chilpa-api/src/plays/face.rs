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
use chilpa_library::store::listening::Shelved;
use crate::source_catalog::Catalog;
use crate::sources::Sounding;

/// What names the cover a sidecar keeps beside its files.
const SIDECAR_COVER: &str = ".chilpa.cover.";

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
    /// The title is only a window label derived from the path.
    title_from_path: bool,
    pub artist: String,
    pub album: String,
    /// The cover on disk, or empty.
    pub cover: String,
    /// It has no end: a station.
    pub live: bool,
    /// What its source says of how it was made, when it says.
    pub ai_generated: Option<bool>,
    /// Who it is for a listen, as the library row says when it holds it.
    pub track_number: Option<u16>,
    pub recording_mbid: Option<String>,
    pub isrc: Option<String>,
}

impl Face {
    /// Names backed by event tags, a card or the catalog. A path-derived
    /// window title is absent here, so it cannot replace a saved identity.
    pub(crate) fn sourced(&self) -> [&str; 3] {
        [if self.title_from_path { "" } else { &self.title }, &self.artist, &self.album]
    }
}

/// The face of the track at `path`, with the tags and the cover its file
/// gave, and the row of the library when it holds the track.
pub(crate) fn face(path: &Path, tags: Option<&chilpa_tags::Tags>, artwork: Option<&Path>, shelved: Option<&Shelved>, cards: &Cards, catalog: &Catalog) -> Face {
    let address = path.display().to_string();
    let name = path.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
    let mut face = Face {
        title: tags.and_then(|tags| tags.title.clone()).unwrap_or(name),
        title_from_path: tags.and_then(|tags| tags.title.as_ref()).is_none(),
        artist: tags.map(|tags| tags.artist_line()).unwrap_or_default(),
        album: tags.and_then(|tags| tags.album.clone()).unwrap_or_default(),
        cover: artwork.map(|cover| cover.display().to_string()).unwrap_or_default(),
        live: false,
        ai_generated: catalog.track_by_path(&address).and_then(|(track, _)| track.metadata["ai_generated"].as_bool()),
        track_number: None,
        recording_mbid: None,
        isrc: None,
    };
    // A track the library holds is named as its lists name it: its row
    // carries what a sidecar says over the file's own tags.
    if let Some(row) = shelved {
        if !row.title.is_empty() {
            face.title = row.title.clone();
            face.title_from_path = false;
        }
        if !row.artist.is_empty() { face.artist = row.artist.clone(); }
        if !row.album.is_empty() { face.album = row.album.clone(); }
        // Only a cover chosen in the editor: the row's own may be a smaller
        // copy of what the file embeds.
        if row.cover.contains(SIDECAR_COVER) { face.cover = row.cover.clone(); }
        face.track_number = row.track_number;
        face.recording_mbid = row.recording_mbid.clone().filter(|id| !id.is_empty());
        face.isrc = row.isrc.clone().filter(|code| !code.is_empty());
    }
    // What came from a search already carries its card.
    if let Some((_, hit)) = cards.search.as_ref().filter(|(url, _)| *url == address) {
        face.title = hit.title.clone();
        face.title_from_path = false;
        face.artist = hit.artist.clone();
        face.album = hit.album.clone();
    }
    // And what was started from a listing, the card it was listed with.
    if let Some(sounding) = cards.entry.as_ref().filter(|sounding| sounding.url == address) {
        face.title = sounding.entry.title.clone();
        face.title_from_path = false;
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
        let face = face(Path::new(address), None, None, None, &cards, &catalog());
        assert_eq!((face.title.as_str(), face.artist.as_str(), face.album.as_str()), ("The song", "Somebody", "A record"), "not the address, which is all the decoder knows");
        assert_eq!(face.sourced(), ["The song", "Somebody", "A record"]);
        assert!(!face.live && face.cover.is_empty());
    }

    #[test]
    fn what_was_started_from_a_listing_is_named_by_the_card_it_was_listed_with() {
        let address = "https://radio.example/stream";
        let cards = Cards { search: None, entry: Some(listed(address, "The station", "/covers/station.jpg", true)) };
        let tags = chilpa_tags::Tags { title: Some("icy".into()), album: Some("ignored".into()), ..Default::default() };
        let named = face(Path::new(address), Some(&tags), None, None, &cards, &catalog());
        assert_eq!((named.title.as_str(), named.artist.as_str(), named.album.as_str()), ("The station", "Its city", ""));
        assert_eq!(named.sourced(), ["The station", "Its city", ""]);
        assert_eq!(named.cover, "/covers/station.jpg", "its own picture, when the file brings no cover");
        assert!(named.live);
        let with_cover = face(Path::new(address), None, Some(Path::new("/covers/embedded.jpg")), None, &cards, &catalog());
        assert_eq!(with_cover.cover, "/covers/embedded.jpg", "the file's own cover stays");
    }

    #[test]
    fn a_track_that_is_not_the_cards_is_named_by_its_file() {
        let cards = Cards { search: Some(("https://cdn.example/other".into(), hit("Another"))), entry: Some(listed("https://radio.example/other", "Another station", "/covers/other.jpg", true)) };
        let tags = chilpa_tags::Tags { title: Some("Tagged".into()), artists: vec!["A".into(), "B".into()], album: Some("Record".into()), ..Default::default() };
        let tagged = face(Path::new("/music/01 - file.flac"), Some(&tags), None, None, &cards, &catalog());
        assert_eq!((tagged.title.as_str(), tagged.artist.as_str(), tagged.album.as_str()), ("Tagged", "A, B", "Record"));
        assert_eq!(tagged.sourced(), ["Tagged", "A, B", "Record"]);
        let bare = face(Path::new("/music/01 - file.flac"), None, None, None, &cards, &catalog());
        assert_eq!((bare.title.as_str(), bare.artist.as_str(), bare.cover.as_str()), ("01 - file", "", ""));
        assert_eq!(bare.sourced(), ["", "", ""]);
        assert!(!bare.live);
    }

    #[test]
    fn a_file_the_library_holds_is_named_as_its_row_names_it() {
        let cards = Cards::default();
        let tags = chilpa_tags::Tags { title: Some("From the file".into()), artists: vec!["File artist".into()], album: Some("File album".into()),
            track_number: Some(9), isrc: Some("FILE00000001".into()), ..Default::default() };
        let row = Shelved { title: "From the sidecar".into(), artist: "Sidecar artist".into(), album: "Sidecar album".into(),
            cover: "/music/Record/.chilpa.cover.jpg".into(), track_number: Some(2), recording_mbid: Some("rec-1".into()), isrc: None };
        let shelved = face(Path::new("/music/Record/02.flac"), Some(&tags), Some(Path::new("/covers/embedded.jpg")), Some(&row), &cards, &catalog());
        assert_eq!((shelved.title.as_str(), shelved.artist.as_str(), shelved.album.as_str()), ("From the sidecar", "Sidecar artist", "Sidecar album"));
        assert_eq!(shelved.sourced(), ["From the sidecar", "Sidecar artist", "Sidecar album"], "the listen takes the same names");
        assert_eq!(shelved.cover, "/music/Record/.chilpa.cover.jpg", "the cover chosen in the editor");
        assert_eq!((shelved.track_number, shelved.recording_mbid.as_deref(), shelved.isrc.as_deref()), (Some(2), Some("rec-1"), None));
        let plain = Shelved { cover: "/cache/covers/small.jpg".into(), title: String::new(), ..row };
        let kept = face(Path::new("/music/Record/02.flac"), Some(&tags), Some(Path::new("/covers/embedded.jpg")), Some(&plain), &cards, &catalog());
        assert_eq!(kept.cover, "/covers/embedded.jpg", "any other cover of the row leaves the file's");
        assert_eq!(kept.title, "From the file", "an empty row field leaves the file's");
    }

    #[test]
    fn the_facade_finds_the_row_by_the_file_s_address() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let local = lib.source_id("local", "default").unwrap();
        lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, artist, duration_ms, added_at, track_number, recording_mbid)
            VALUES (?1, '/music/Record/02.flac', 'From the sidecar', 'Sidecar artist', 1000, 1, 2, 'rec-1')", [local]).unwrap();
        let mut api = crate::Chilpa::with_listed(Vec::new(), Vec::new());
        api.library = Some(lib);
        let tags = chilpa_tags::Tags { title: Some("From the file".into()), artists: vec!["File artist".into()], ..Default::default() };
        let held = api.face_of(0, Path::new("/music/Record/02.flac"), Some(&tags), None);
        assert_eq!((held.title.as_str(), held.artist.as_str(), held.track_number), ("From the sidecar", "Sidecar artist", Some(2)));
        let loose = api.face_of(0, Path::new("/elsewhere/02.flac"), Some(&tags), None);
        assert_eq!((loose.title.as_str(), loose.artist.as_str(), loose.track_number), ("From the file", "File artist", None), "a file outside the library keeps its tags");
    }
}
