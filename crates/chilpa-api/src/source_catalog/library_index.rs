//! What the listener brought from a source into the library, read for its
//! screens: the records of its saved albums and the tracks of its saved
//! tracks, compact, under one lock and without asking the plugin.

use super::store::{Card, CardKind, Store, Track};
use super::Catalog;
use crate::library_sources::SourceItem;
use serde_json::Value;

/// Names a field holds as a list, or as one text when the list is absent.
pub(super) fn names(metadata: &Value, list: &str, single: &str) -> Vec<String> {
    let clean = |text: &str| Some(text.trim().to_string()).filter(|t| !t.is_empty());
    let found: Vec<String> = match metadata.get(list).and_then(Value::as_array) {
        Some(values) => values.iter().filter_map(Value::as_str).filter_map(clean).collect(),
        None => metadata.get(single).and_then(Value::as_str).and_then(clean).into_iter().collect(),
    };
    let mut seen = std::collections::HashSet::new();
    found.into_iter().filter(|name| seen.insert(name.to_lowercase())).collect()
}

fn text(metadata: &Value, key: &str) -> String {
    metadata.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

/// A year the item states, or the one its release date starts with.
fn year(metadata: &Value) -> Option<i32> {
    metadata.get("year").and_then(Value::as_i64).and_then(|y| i32::try_from(y).ok())
        .or_else(|| text(metadata, "release_date").get(..4).and_then(|y| y.parse().ok()))
        .filter(|y| *y > 0)
}

/// A genre list without the root some sources put above every genre.
fn genres(metadata: &Value, card_genre: &str) -> Vec<String> {
    let mut found = super::genres::own_genres(metadata, card_genre);
    if found.is_empty() && !card_genre.trim().is_empty() { found.push(card_genre.trim().to_string()); }
    found.retain(|genre| !genre.eq_ignore_ascii_case("music"));
    found
}

pub(super) fn track_groups(track: &Track) -> Option<Box<crate::TrackGroups>> {
    let groups = crate::TrackGroups {
        year: year(&track.metadata),
        genre: chilpa_library::first_name(&genres(&track.metadata, "")),
        label: text(&track.metadata, "label"),
        composer: chilpa_library::first_name(&super::composers::own_composers(&track.metadata)),
    };
    (groups != crate::TrackGroups::default()).then(|| Box::new(groups))
}

fn release_type(metadata: &Value) -> String {
    let kind = text(metadata, "release_type").to_lowercase();
    if matches!(kind.as_str(), "album" | "ep" | "single" | "compilation" | "live") { kind } else { String::new() }
}

/// The library's names for a quality tier and a format.
fn quality(format: Option<&super::store::Format>) -> (&'static str, &'static str) {
    use crate::quality::Tier;
    let Some(format) = format else { return ("unknown", "") };
    let tier = match format.quality().tier {
        Tier::Lossy => "lossy", Tier::Lossless => "lossless", Tier::HiRes => "hires", Tier::Dsd => "dsd", Tier::Unknown => "unknown",
    };
    let key = match format.codec.as_str() {
        "flac" => "flac", "alac" => "alac", "aac" => "aac", "mp3" => "mp3", "vorbis" => "ogg",
        "opus" => "opus", "wav" => "wav", "aiff" => "aiff", "dsd" => "dsd", _ => "",
    };
    (tier, key)
}

fn record(card: &Card, enrichment: &super::genres::State) -> SourceItem {
    let (tier, format) = quality(card.format.as_ref());
    let tracks = card.tracks.as_ref().map_or_else(|| card.metadata["tracks"].as_u64().unwrap_or_default() as usize, Vec::len);
    SourceItem {
        album: card.id,
        plugin: card.plugin.clone(),
        identity: card.identity.clone(),
        album_identity: card.identity.clone(),
        album_artist: card.artist.clone(),
        album_title: card.title.clone(),
        title: card.title.clone(),
        artists: [card.artist.clone()].into_iter().filter(|a| !a.trim().is_empty()).collect(),
        year: card.year.or_else(|| year(&card.metadata)),
        genres: enrichment.of_card(card, super::now()).into_iter().filter(|g| !g.eq_ignore_ascii_case("music")).collect(),
        label: text(&card.metadata, "label"),
        composers: names(&card.metadata, "composers", "composer"),
        added_at: card.metadata["added_at"].as_i64().unwrap_or(0),
        release_type: release_type(&card.metadata),
        duration_ms: card.metadata["duration_ms"].as_u64().unwrap_or(0),
        tier, format, tracks,
        ..SourceItem::default()
    }
}

/// A track, with what its record says of what the track does not.
fn song(track: &Track, card: Option<&Card>, saved: bool, enrichment: &super::genres::State, credits: &super::composers::State) -> SourceItem {
    let album = card.map(|card| record(card, enrichment));
    let (tier, format) = quality(track.format.as_ref().or(card.and_then(|c| c.format.as_ref())));
    let mut artists: Vec<String> = [track.artist.clone()].into_iter().filter(|a| !a.trim().is_empty()).collect();
    if let Some(owner) = album.as_ref().map(|a| a.album_artist.clone()).filter(|a| !a.trim().is_empty() && !artists.iter().any(|seen| seen.eq_ignore_ascii_case(a))) {
        artists.push(owner);
    }
    let own_genres = genres(&track.metadata, "");
    let own_label = text(&track.metadata, "label");
    let identity = track.metadata["ref"].as_str().filter(|_| track.metadata["kind"] == "track")
        .map(|reference| reference.split_once("@playlist:").map_or(reference, |(song, _)| song).to_string())
        .unwrap_or_else(|| track.path.clone());
    let mut composers = credits.of_track(track, super::now());
    if composers.is_empty() { composers = album.as_ref().map(|a| a.composers.clone()).unwrap_or_default(); }
    SourceItem {
        track: track.id,
        album: if track.album_id < 0 { track.album_id } else { 0 },
        plugin: track.plugin.clone(),
        identity,
        album_identity: card.map(|c| c.identity.clone()).unwrap_or_default(),
        album_artist: album.as_ref().map(|a| a.album_artist.clone()).unwrap_or_else(|| track.artist.clone()),
        album_title: album.as_ref().map(|a| a.album_title.clone()).unwrap_or_else(|| track.album.clone()),
        title: track.title.clone(),
        artists,
        year: album.as_ref().and_then(|a| a.year).filter(|y| *y > 0).or_else(|| year(&track.metadata)),
        genres: if own_genres.is_empty() { album.as_ref().map(|a| a.genres.clone()).unwrap_or_default() } else { own_genres },
        label: if own_label.is_empty() { album.as_ref().map(|a| a.label.clone()).unwrap_or_default() } else { own_label },
        composers,
        added_at: track.metadata["added_at"].as_i64().or(album.as_ref().map(|a| a.added_at)).unwrap_or(0),
        release_type: album.as_ref().map(|a| a.release_type.clone()).unwrap_or_default(),
        disc: (track.disc > 0).then_some(track.disc as i64),
        number: (track.number > 0).then_some(track.number as i64),
        duration_ms: track.duration_ms,
        tier, format,
        saved_track: saved,
        tracks: 1,
        playable: track.playable,
        ..SourceItem::default()
    }
}

fn items_of(store: &Store, albums: &[String], tracks: &[String], enrichment: &super::genres::State, credits: &super::composers::State) -> Vec<SourceItem> {
    let mut items = Vec::new();
    let mut at = std::collections::HashMap::new();
    for plugin in albums {
        let Some(shelf) = store.shelves.get(&format!("{plugin} section:albums")) else { continue };
        for card in shelf.cards.iter().filter_map(|id| store.cards.get(id)).filter(|c| c.kind == CardKind::Album) {
            match &card.tracks {
                Some(ids) if !ids.is_empty() => {
                    for track in ids.iter().filter_map(|id| store.tracks.get(id)) {
                        if at.contains_key(&track.id) { continue; }
                        at.insert(track.id, items.len());
                        items.push(SourceItem { saved_album: true, ..song(track, Some(card), false, enrichment, credits) });
                    }
                }
                _ => items.push(SourceItem { saved_album: true, ..record(card, enrichment) }),
            }
        }
    }
    for plugin in tracks {
        let Some(shelf) = store.shelves.get(&format!("{plugin} section:tracks")) else { continue };
        for track in shelf.tracks.iter().filter_map(|id| store.tracks.get(id)) {
            if let Some(&known) = at.get(&track.id) {
                items[known].saved_track = true;
                continue;
            }
            at.insert(track.id, items.len());
            let card = store.cards.get(&track.album_id).filter(|c| c.kind == CardKind::Album);
            items.push(song(track, card, true, enrichment, credits));
        }
    }
    items
}

/// A record an integrated listing keeps, as a version among the library's:
/// what its page's version choice says of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceRecord {
    pub id: i64,
    pub plugin: String,
    pub identity: String,
    pub group: String,
    pub title: String,
    pub quality: crate::quality::Quality,
    /// What the source says: explicit, clean, or nothing.
    pub explicit: Option<bool>,
    pub tracks: usize,
}

impl SourceRecord {
    pub(crate) fn version(&self) -> crate::library_sources::SourceVersion {
        crate::library_sources::SourceVersion {
            group: self.group.clone(), edition: crate::library_sources::words(&self.title), tracks: self.tracks,
            explicit: self.explicit, tier: self.quality.tier.rank().clamp(0, 4) as u8, plugin: self.plugin.clone(),
            key: crate::library_sources::SourceVersion::key_of(&self.plugin, &self.identity),
        }
    }
}

fn source_record(store: &Store, card: &Card) -> SourceRecord {
    let read = card.tracks.iter().flatten().filter_map(|id| store.tracks.get(id)).filter_map(|t| t.format.as_ref()).map(|f| f.quality());
    let quality = read.chain(card.format.as_ref().map(|f| f.quality()))
        .max_by_key(|q| (q.tier.rank(), q.bits, q.rate_hz)).unwrap_or_default();
    SourceRecord {
        id: card.id, plugin: card.plugin.clone(), identity: card.identity.clone(),
        group: chilpa_library::store::versions::album_group(&card.title, &card.artist),
        title: card.title.clone(), quality,
        explicit: card.metadata["explicit"].as_bool(),
        tracks: card.tracks.as_ref().map_or_else(|| card.metadata["tracks"].as_u64().unwrap_or_default() as usize, Vec::len),
    }
}

impl Catalog {
    /// The records the integrated listings keep, saved ones and those of
    /// saved tracks, each once.
    pub(crate) fn integrated_records(&self, chosen: &[(String, bool, bool)]) -> Vec<SourceRecord> {
        let inner = self.lock();
        let store = &inner.store;
        let mut seen = std::collections::HashSet::new();
        let mut records = Vec::new();
        for (plugin, albums, tracks) in chosen {
            let cards: Vec<i64> = [albums.then_some("albums"), tracks.then_some("tracks")].into_iter().flatten()
                .filter_map(|section| store.shelves.get(&format!("{plugin} section:{section}")))
                .flat_map(|shelf| shelf.cards.iter().copied().chain(shelf.tracks.iter().filter_map(|id| store.tracks.get(id)).map(|t| t.album_id)))
                .collect();
            for card in cards.into_iter().filter_map(|id| store.cards.get(&id)).filter(|c| c.kind == CardKind::Album) {
                if seen.insert(card.id) { records.push(source_record(store, card)); }
            }
        }
        records
    }

    /// What changes when an integrated listing or one of its records is read
    /// again, and nothing else: a source browsed elsewhere leaves it alone.
    pub(crate) fn integrated_signature(&self, chosen: &[(String, bool, bool)]) -> u64 {
        use std::hash::{Hash, Hasher};
        let inner = self.lock();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        inner.genres.revision.hash(&mut hash);
        inner.composers.revision.hash(&mut hash);
        for (plugin, albums, tracks) in chosen {
            for section in [albums.then_some("albums"), tracks.then_some("tracks")].into_iter().flatten() {
                let Some(shelf) = inner.store.shelves.get(&format!("{plugin} section:{section}")) else { 0_u8.hash(&mut hash); continue };
                (shelf.read_at, &shelf.cards, &shelf.tracks).hash(&mut hash);
                for card in shelf.cards.iter().filter_map(|id| inner.store.cards.get(id)) {
                    (card.read_at, card.tracks.as_ref().map(Vec::len)).hash(&mut hash);
                }
            }
        }
        hash.finish()
    }

    /// The saved records of `albums` and the saved tracks of `tracks`, by
    /// plugin id. Read in place: a listing that is stale is queued by the
    /// caller, never here.
    pub(crate) fn integrated_items(&self, albums: &[String], tracks: &[String]) -> Vec<SourceItem> {
        let inner = self.lock();
        items_of(&inner.store, albums, tracks, &inner.genres, &inner.composers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_track_takes_from_its_record_what_it_does_not_say_and_a_record_without_tracks_stands_alone() {
        let mut store = Store::default();
        let read = store.card_id("guest", "album:1");
        let unread = store.card_id("guest", "album:2");
        for (id, title, genres) in [(read, "Read", json!(["Jazz", "Music"])), (unread, "Unread", json!(["Soul"]))] {
            let card = store.cards.get_mut(&id).unwrap();
            card.kind = CardKind::Album;
            card.title = title.into();
            card.artist = "Band".into();
            card.metadata = json!({"genres": genres, "label": "House", "release_date": "1971-05-01", "added_at": 9, "release_type": "EP", "tracks": 7});
        }
        let song_id = store.track_id("plugin:guest:track:1");
        {
            let track = store.tracks.get_mut(&song_id).unwrap();
            track.plugin = "guest".into();
            track.album_id = read;
            track.title = "Song".into();
            track.metadata = json!({"kind": "track", "ref": "track:1@playlist:9", "composers": ["Writer"], "year": 2005});
        }
        store.cards.get_mut(&read).unwrap().tracks = Some(vec![song_id]);
        store.shelves.insert("guest section:albums".into(), super::super::store::Shelf { cards: vec![read, unread], ..Default::default() });
        let items = items_of(&store, &["guest".into()], &[], &Default::default(), &Default::default());
        assert_eq!(items.len(), 2);
        let song = &items[0];
        assert!(song.saved_album);
        assert_eq!((song.track, song.album, song.title.as_str(), song.identity.as_str()), (song_id, read, "Song", "track:1"));
        assert_eq!(song.genres, ["Jazz"], "the root genre is not a genre");
        assert_eq!((song.label.as_str(), song.year, song.added_at, song.release_type.as_str()), ("House", Some(1971), 9, "ep"));
        assert_eq!((song.composers.clone(), song.artists.clone(), song.saved_track), (vec!["Writer".to_string()], vec!["Band".to_string()], false));
        let alone = &items[1];
        assert_eq!((alone.track, alone.album, alone.tracks, alone.genres.clone()), (0, unread, 7, vec!["Soul".to_string()]));
    }

    #[test]
    fn a_saved_track_on_a_saved_record_is_read_once() {
        let mut store = Store::default();
        let album = store.card_id("guest", "album:1");
        store.cards.get_mut(&album).unwrap().kind = CardKind::Album;
        let id = store.track_id("plugin:guest:track:1");
        store.tracks.get_mut(&id).unwrap().album_id = album;
        store.cards.get_mut(&album).unwrap().tracks = Some(vec![id]);
        store.shelves.insert("guest section:albums".into(), super::super::store::Shelf { cards: vec![album], ..Default::default() });
        store.shelves.insert("guest section:tracks".into(), super::super::store::Shelf { tracks: vec![id], ..Default::default() });
        let items = items_of(&store, &["guest".into()], &["guest".into()], &Default::default(), &Default::default());
        assert_eq!(items.len(), 1);
        assert!(items[0].saved_track && items[0].saved_album, "Tracks shows it, and Albums its record");
    }

    #[test]
    fn a_track_uses_its_record_year_then_its_own() {
        let track = Track { metadata: json!({"year": 1999}), ..Default::default() };
        let card = Card { year: Some(1985), ..Default::default() };
        assert_eq!(song(&track, Some(&card), true, &Default::default(), &Default::default()).year, Some(1985));
        assert_eq!(song(&track, Some(&Card::default()), true, &Default::default(), &Default::default()).year, Some(1999));
        assert_eq!(song(&track, None, true, &Default::default(), &Default::default()).year, Some(1999));
    }

    #[test]
    fn loaded_source_rows_name_the_same_metadata_groups_as_the_index() {
        let track = Track { metadata: json!({"year":1999, "genres":["Zulu", "alpha"], "label":"House", "composers":["Writer, Z", "author"]}), ..Default::default() };
        let row = super::super::view::row_of(&track, String::new());
        for (group, label) in [("year", "1999"), ("decade", "1990s"), ("genre", "alpha"), ("label", "House"), ("composer", "author")] {
            assert_eq!(crate::group_of(group, &row), label);
        }
        let row = super::super::view::row_of(&Track::default(), String::new());
        assert!(row.groups.is_none());
        for (group, label) in [("year", "Unknown year"), ("decade", "Unknown year"), ("genre", "No genre"), ("label", "No label"), ("composer", "No composer")] {
            assert_eq!(crate::group_of(group, &row), chilpa_i18n::t(label));
        }
    }
}
