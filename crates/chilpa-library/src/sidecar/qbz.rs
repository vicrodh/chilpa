//! Reading the sidecars QBZ left, which are never written.
//!
//! QBZ kept a `.qbz.json` per record. Version 1 holds the album and its
//! tracks; version 2 adds `extendedAlbum` and `extendedTracks`. Its keys
//! are camelCase, and it says "cleared" with an empty text or a zero
//! because a missing field meant "not overridden".
//!
//! It named each track by its whole path. A folder that moved left those
//! names pointing nowhere, so here a track goes by the last part of the
//! name QBZ gave it.

use super::document::{Override, Sidecar, SidecarError, Track};
use serde_json::Value;

/// A text that says something.
fn said(value: &Value) -> Option<String> {
    value.as_str().map(str::trim).filter(|text| !text.is_empty()).map(str::to_string)
}

/// A text where empty means cleared.
fn text_or_cleared(value: &Value) -> Override<String> {
    match value.as_str() {
        None => Override::Absent,
        Some(text) if text.trim().is_empty() => Override::Cleared,
        Some(text) => Override::Value(text.trim().to_string()),
    }
}

/// A number where zero means cleared.
fn number_or_cleared(value: &Value) -> Override<u32> {
    match value.as_u64() {
        None => Override::Absent,
        Some(0) => Override::Cleared,
        Some(n) => u32::try_from(n).map(Override::Value).unwrap_or(Override::Absent),
    }
}

/// A list that says something. QBZ wrote every list on every save, so an
/// empty one is what the file had that day, not something cleared.
fn listed(value: &Value) -> Override<Vec<String>> {
    let texts: Vec<String> = value.as_array().map(|items| items.iter().filter_map(said).collect()).unwrap_or_default();
    if texts.is_empty() {
        Override::Absent
    } else {
        Override::Value(texts)
    }
}

fn texted(value: &Value) -> Override<String> {
    said(value).map(Override::Value).unwrap_or(Override::Absent)
}

/// What a track goes by here, out of the path QBZ knew it by.
fn key_of(path: &str) -> String {
    // A track inside a disc image was `sacd:<image>#<number>`.
    if path.starts_with("sacd:") {
        if let Some((_, number)) = path.rsplit_once('#') {
            return format!("#{number}");
        }
    }
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// A track QBZ cut out of a CUE sheet. Chilpa has no such tracks.
fn from_a_cue(entry: &Value) -> bool {
    !entry["cueStartSecs"].is_null()
}

pub fn import(bytes: &[u8]) -> Result<Sidecar, SidecarError> {
    let root: Value = serde_json::from_slice(bytes).map_err(|e| SidecarError::Unreadable(e.to_string()))?;
    if !root["album"].is_object() || !root["tracks"].is_array() {
        return Err(SidecarError::Unreadable("not a QBZ sidecar".to_string()));
    }
    let mut out = Sidecar::new(root["updatedAt"].as_i64().unwrap_or(0));
    out.imported = true;

    let album = &root["album"];
    out.album.title = texted(&album["albumTitle"]);
    // An empty album artist was QBZ's "none". Here a record always has one.
    out.album.album_artist = texted(&album["albumArtist"]);
    out.album.date = match number_or_cleared(&album["year"]) {
        Override::Absent => Override::Absent,
        Override::Cleared => Override::Cleared,
        Override::Value(year) => Override::Value(format!("{year:04}")),
    };
    out.album.genres = match text_or_cleared(&album["genre"]) {
        Override::Absent => Override::Absent,
        Override::Cleared => Override::Cleared,
        Override::Value(genre) => Override::Value(crate::store::split_genres(&[genre])),
    };
    out.album.catalog_number = text_or_cleared(&album["catalogNumber"]);

    let extended = &root["extendedAlbum"];
    if extended.is_object() {
        out.album.album_artists = listed(&extended["albumArtists"]);
        out.album.compilation = extended["compilation"].as_bool().map(Override::Value).unwrap_or(Override::Absent);
        out.album.musicbrainz_release_id = texted(&extended["musicbrainzReleaseId"]);
        out.album.musicbrainz_release_group_id = texted(&extended["musicbrainzReleaseGroupId"]);
        out.album.musicbrainz_album_artist_ids = listed(&extended["musicbrainzAlbumArtistIds"]);
        out.album.discogs_release_id = texted(&extended["discogsReleaseId"]);
        // `artworkPath` named a file in QBZ's cache by its whole path. It
        // is not read.
    }

    // Two files QBZ knew by different paths that go by one name here --
    // CD1/01.flac and CD2/01.flac -- cannot be told apart: neither is
    // given anything, rather than both being given the last one's.
    let mut seen: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    let mut ambiguous: Vec<String> = Vec::new();
    for list in ["tracks", "extendedTracks"] {
        for entry in root[list].as_array().into_iter().flatten() {
            let Some(path) = entry["filePath"].as_str() else { continue };
            let key = key_of(path);
            match seen.get(&key) {
                Some(other) if *other != path => ambiguous.push(key),
                Some(_) => {}
                None => {
                    seen.insert(key, path);
                }
            }
        }
    }

    for entry in root["tracks"].as_array().into_iter().flatten() {
        let Some(path) = entry["filePath"].as_str() else { continue };
        if from_a_cue(entry) {
            continue;
        }
        let track: &mut Track = out.track_mut(&key_of(path));
        track.title = texted(&entry["title"]);
        track.disc_number = number_or_cleared(&entry["discNumber"]);
        track.track_number = number_or_cleared(&entry["trackNumber"]);
    }
    for entry in root["extendedTracks"].as_array().into_iter().flatten() {
        let Some(path) = entry["filePath"].as_str() else { continue };
        if from_a_cue(entry) {
            continue;
        }
        let track: &mut Track = out.track_mut(&key_of(path));
        track.artist_credit = texted(&entry["artistCredit"]);
        track.artists = listed(&entry["artists"]);
        track.composers = listed(&entry["composers"]);
        track.performers = listed(&entry["performers"]);
        track.musicbrainz_recording_id = texted(&entry["musicbrainzRecordingId"]);
        track.musicbrainz_track_id = texted(&entry["musicbrainzTrackId"]);
        track.musicbrainz_artist_ids = listed(&entry["musicbrainzArtistIds"]);
    }
    out.tracks.retain(|key, _| !ambiguous.contains(key));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of one found in a real library, with its folder's old name.
    const REAL: &str = r#"{
      "version": 1,
      "updatedAt": 1769912863,
      "album": {
        "albumTitle": "The World! Evangelion Jazz Night =The Tokyo III Jazz Club=",
        "albumArtist": "Shiro Sagisu",
        "year": 2014,
        "genre": "Jazz, Non-Music, Pop",
        "catalogNumber": "KICA-3219"
      },
      "tracks": [
        {
          "filePath": "/music/SHIRO SAGISU - Old folder name/01. Welcome to the Tokyo III jazz club.flac",
          "cueStartSecs": null,
          "title": "Welcome To The Tokyo III Jazz Club",
          "discNumber": 1,
          "trackNumber": 1
        }
      ]
    }"#;

    #[test]
    fn a_qbz_sidecar_is_read_by_file_name() {
        let s = import(REAL.as_bytes()).unwrap();
        assert!(s.imported);
        assert_eq!(s.updated_at, 1769912863);
        assert_eq!(s.album.album_artist, Override::Value("Shiro Sagisu".to_string()));
        assert_eq!(s.album.date, Override::Value("2014".to_string()));
        assert_eq!(s.album.catalog_number, Override::Value("KICA-3219".to_string()));
        // Read as any genre tag is: a list with commas is its genres.
        assert_eq!(s.album.genres, Override::Value(vec!["Jazz".to_string(), "Non-Music".to_string(), "Pop".to_string()]));
        let track = &s.tracks["01. Welcome to the Tokyo III jazz club.flac"];
        assert_eq!(track.title, Override::Value("Welcome To The Tokyo III Jazz Club".to_string()));
        assert_eq!(track.track_number, Override::Value(1));
    }

    #[test]
    fn a_qbz_zero_clears_and_a_qbz_blank_artist_is_ignored() {
        let json = r#"{"version":1,"updatedAt":1,
            "album":{"albumTitle":"A","albumArtist":"  ","year":0,"genre":"","catalogNumber":""},
            "tracks":[{"filePath":"C:\\Music\\A\\01.flac","cueStartSecs":null,"title":"T","discNumber":0,"trackNumber":7}]}"#;
        let s = import(json.as_bytes()).unwrap();
        assert_eq!(s.album.album_artist, Override::Absent);
        assert_eq!(s.album.date, Override::Cleared);
        assert_eq!(s.album.genres, Override::Cleared);
        assert_eq!(s.album.catalog_number, Override::Cleared);
        let track = &s.tracks["01.flac"];
        assert_eq!(track.disc_number, Override::Cleared);
        assert_eq!(track.track_number, Override::Value(7));
    }

    #[test]
    fn a_qbz_cue_entry_is_ignored() {
        let json = r#"{"version":1,"updatedAt":1,"album":{},
            "tracks":[{"filePath":"/m/a/image.flac","cueStartSecs":12.5,"title":"Cut","discNumber":1,"trackNumber":2}]}"#;
        assert!(import(json.as_bytes()).unwrap().tracks.is_empty());
    }

    #[test]
    fn a_qbz_sidecar_of_the_second_kind_brings_its_credits() {
        let json = r#"{"version":2,"updatedAt":1,"album":{"albumTitle":"A"},"tracks":[],
            "extendedAlbum":{"albumArtists":["Alpha","Beta"],"compilation":true,
                "musicbrainzReleaseId":"release","musicbrainzReleaseGroupId":null,
                "musicbrainzAlbumArtistIds":[],"discogsReleaseId":"123","artworkPath":"/cache/cover.jpg"},
            "extendedTracks":[{"filePath":"sacd:/m/Disc #1.iso#3","cueStartSecs":null,
                "artistCredit":"Alpha feat. Beta","artists":["Alpha","Beta"],"composers":[],"performers":["Player (guitar)"],
                "musicbrainzRecordingId":"recording","musicbrainzTrackId":null,"musicbrainzArtistIds":["a","b"]}]}"#;
        let s = import(json.as_bytes()).unwrap();
        assert_eq!(s.album.album_artists, Override::Value(vec!["Alpha".to_string(), "Beta".to_string()]));
        assert_eq!(s.album.compilation, Override::Value(true));
        assert_eq!(s.album.discogs_release_id, Override::Value("123".to_string()));
        assert_eq!(s.album.musicbrainz_album_artist_ids, Override::Absent);
        assert_eq!(s.album.cover, Override::Absent, "their cache is theirs");
        let track = &s.tracks["#3"];
        assert_eq!(track.artist_credit, Override::Value("Alpha feat. Beta".to_string()));
        assert_eq!(track.composers, Override::Absent, "an empty list is what the file had, not something cleared");
        assert_eq!(track.performers, Override::Value(vec!["Player (guitar)".to_string()]));
    }

    #[test]
    fn two_discs_with_one_file_name_give_neither_track_anything() {
        let json = r#"{"version":1,"updatedAt":1,"album":{"albumTitle":"Box","albumArtist":"","year":0,"genre":"","catalogNumber":""},
            "tracks":[{"filePath":"/m/Box/CD1/01.flac","title":"One","discNumber":1,"trackNumber":1},
                      {"filePath":"/m/Box/CD2/01.flac","title":"Other","discNumber":2,"trackNumber":1},
                      {"filePath":"/m/Box/CD2/02.flac","title":"Two","discNumber":2,"trackNumber":2}]}"#;
        let s = import(json.as_bytes()).unwrap();
        assert!(!s.tracks.contains_key("01.flac"), "which of the two it was cannot be told");
        assert_eq!(s.tracks["02.flac"].title, Override::Value("Two".to_string()));
        assert_eq!(s.album.title, Override::Value("Box".to_string()));
    }

    #[test]
    fn what_is_not_theirs_is_unreadable() {
        for json in ["junk", "{}", r#"{"album":{}}"#, r#"{"album":[],"tracks":[]}"#] {
            assert!(matches!(import(json.as_bytes()), Err(SidecarError::Unreadable(_))), "{json}");
        }
    }
}
