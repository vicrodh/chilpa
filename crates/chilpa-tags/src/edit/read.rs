//! What a file says, one field per thing the editor shows.

use crate::TagError;
use lofty::picture::PictureType;
use lofty::prelude::ItemKey;
use lofty::tag::{Accessor, Tag};
use std::path::Path;

/// What a file's tag says, as the editor needs it. Nothing is joined or
/// split: every list is the file's own values, in the file's order.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Snapshot {
    pub title: Option<String>,
    /// Every artist value, in order.
    pub artist_credit: Vec<String>,
    /// The artists as separate names, where the file keeps them apart.
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub album_artists: Vec<String>,
    pub compilation: Option<bool>,
    /// The date as it comes: `2019`, `2019-03-15`, or anything else.
    pub date: Option<String>,
    pub genres: Vec<String>,
    pub catalog_number: Option<String>,
    /// The record label, or the publisher where the file names no label.
    pub label: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub composers: Vec<String>,
    pub performers: Vec<String>,
    pub musicbrainz_release_id: Option<String>,
    pub musicbrainz_release_group_id: Option<String>,
    pub musicbrainz_album_artist_ids: Vec<String>,
    pub musicbrainz_recording_id: Option<String>,
    pub musicbrainz_track_id: Option<String>,
    pub musicbrainz_artist_ids: Vec<String>,
    /// How many pictures it carries.
    pub pictures: usize,
    /// Whether one of them is a front cover.
    pub front_cover: bool,
}

/// Every value a tag holds under a key, trimmed, without the empty ones.
fn values(tag: &Tag, key: ItemKey) -> Vec<String> {
    tag.get_strings(key).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).collect()
}

/// The first of them.
fn value(tag: &Tag, key: ItemKey) -> Option<String> {
    values(tag, key).into_iter().next()
}

fn trimmed(text: Option<std::borrow::Cow<'_, str>>) -> Option<String> {
    text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// What one tag says.
pub(crate) fn of_tag(tag: &Tag) -> Snapshot {
    let artist_credit = {
        let all = values(tag, ItemKey::TrackArtist);
        if all.is_empty() { trimmed(tag.artist()).into_iter().collect() } else { all }
    };
    let genres = {
        let all = values(tag, ItemKey::Genre);
        if all.is_empty() { trimmed(tag.genre()).into_iter().collect() } else { all }
    };
    Snapshot {
        title: trimmed(tag.title()),
        artist_credit,
        artists: values(tag, ItemKey::TrackArtists),
        album: trimmed(tag.album()),
        album_artist: value(tag, ItemKey::AlbumArtist),
        album_artists: values(tag, ItemKey::AlbumArtists),
        compilation: value(tag, ItemKey::FlagCompilation)
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes")),
        // The same order `read` looks in.
        date: value(tag, ItemKey::RecordingDate)
            .or_else(|| value(tag, ItemKey::ReleaseDate))
            .or_else(|| value(tag, ItemKey::Year)),
        genres,
        catalog_number: value(tag, ItemKey::CatalogNumber),
        label: value(tag, ItemKey::Label).or_else(|| value(tag, ItemKey::Publisher)),
        track_number: tag.track(),
        disc_number: tag.disk(),
        composers: values(tag, ItemKey::Composer),
        performers: values(tag, ItemKey::Performer),
        musicbrainz_release_id: value(tag, ItemKey::MusicBrainzReleaseId),
        musicbrainz_release_group_id: value(tag, ItemKey::MusicBrainzReleaseGroupId),
        musicbrainz_album_artist_ids: values(tag, ItemKey::MusicBrainzReleaseArtistId),
        musicbrainz_recording_id: value(tag, ItemKey::MusicBrainzRecordingId),
        musicbrainz_track_id: value(tag, ItemKey::MusicBrainzTrackId),
        musicbrainz_artist_ids: values(tag, ItemKey::MusicBrainzArtistId),
        pictures: tag.picture_count() as usize,
        front_cover: tag.get_picture_type(PictureType::CoverFront).is_some(),
    }
}

/// What a file says. A file with no tag says nothing, which is not an error.
///
/// It opens the file the way `read` does, so it reads what the scan reads:
/// the canonical tag, or the first one when there is none, and DSF and DFF
/// through this crate's own readers.
pub fn snapshot(path: &Path) -> Result<Snapshot, TagError> {
    let opened = crate::open(path)?;
    Ok(opened.tag.as_ref().map(of_tag).unwrap_or_default())
}
