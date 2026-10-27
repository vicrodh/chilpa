//! The Artists screen: the library's artists and those of the integrated
//! sources' saved records, through the sources Albums leaves out, each
//! wearing the sources that hold records of theirs, as Artist Scene's cards
//! do. Qualities and formats are a record's, not an artist's: they do not
//! filter here.

use crate::library_sources::{Prepared, SourceIndex, Stars};
use crate::{Artist, Filter, Reader};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

/// The icon the library wears among the sources.
const LIBRARY_ICON: &str = "qrc:/qt/qml/app/chilpa/kit/assets/logo.png";

/// An artist of the Artists screen and where their records are: the library
/// first, then each source, in Artist Scene's shape.
#[derive(Clone, Debug, PartialEq)]
pub struct ListedArtist {
    pub artist: Artist,
    pub sources: Vec<Value>,
}

impl Reader {
    /// The artists of the Artists screen, by name: who a record is credited
    /// to, or with `role` "all" everyone who plays on a track, among the
    /// records of the sources `extras` does not leave out.
    pub fn listed_artists(&self, role: &str, extras: &Filter) -> Vec<ListedArtist> {
        let extras = &Filter { excluded_sources: extras.excluded_sources.clone(), ..Filter::default() };
        let pictures = self.lib.artist_pictures().unwrap_or_default();
        let within = self.passing_album_ids(extras);
        let library = self.lib.artists_by_role_within(chilpa_library::ArtistRole::from_name(role), within.as_ref())
            .unwrap_or_default().into_iter().map(|artist| crate::browse::artist_of(artist, &pictures)).collect();
        let image = |record: i64| self.catalog.as_ref().and_then(|catalog| catalog.card(record)).map(|card| card.image).unwrap_or_default();
        listed(library, &self.source_index().1, extras, role == "all", image)
    }
}

/// The library's artists, and the sources' the library lacks, in one list
/// by name. A source's artist is the library's when their names match the
/// way the library keys its own.
fn listed(library: Vec<Artist>, index: &SourceIndex, extras: &Filter, everyone: bool, image: impl Fn(i64) -> String) -> Vec<ListedArtist> {
    let through = Prepared::new(&Filter { excluded_sources: extras.excluded_sources.clone(), ..Filter::default() });
    let stars = Stars::default();
    // By the library's key, then by plugin: the source's spelling and records.
    let mut found: BTreeMap<String, BTreeMap<String, (String, Vec<i64>)>> = BTreeMap::new();
    for item in index.items.iter().filter(|item| item.saved_album && item.album != 0 && through.passes(item, &stars)) {
        let names: Vec<&String> = if everyone { item.artists.iter().chain([&item.album_artist]).collect() } else { vec![&item.album_artist] };
        for name in names {
            let name = name.trim();
            let various = chilpa_library::is_various(name);
            // As in the library: a compilation's stand-in plays on no record of somebody else's.
            if name.is_empty() || various && !chilpa_library::is_various(&item.album_artist) {
                continue;
            }
            let name = if various { chilpa_library::VARIOUS_ARTISTS } else { name };
            let (spelled, records) = found.entry(name.to_ascii_lowercase()).or_default().entry(item.plugin.clone()).or_insert_with(|| (name.to_string(), Vec::new()));
            // The spelling the library would show: the least of them.
            if name < spelled.as_str() {
                *spelled = name.to_string();
            }
            if !records.contains(&item.album) {
                records.push(item.album);
            }
        }
    }
    let source = |plugin: &str, (name, records): (String, Vec<i64>)| {
        let key = format!("{}:{plugin}", crate::source_opener::PROVIDER);
        let entry = index.sources.iter().find(|entry| entry.key == key);
        json!({"plugin": plugin, "name": entry.map_or(plugin, |entry| entry.label.as_str()), "icon": entry.map_or("", |entry| entry.icon.as_str()),
            "key": format!("{plugin} matching-artist-v3:{name}"), "albums": records.len(),
            "image": records.iter().map(|record| image(*record)).find(|image| !image.is_empty()).unwrap_or_default()})
    };
    let mut listed: Vec<ListedArtist> = library.into_iter().map(|artist| {
        let picture = Some(&artist.artwork_path).filter(|path| !path.is_empty()).and_then(|path| url::Url::from_file_path(path).ok()).map(|url| url.to_string());
        let mut sources = vec![json!({"local": true, "name": "Chilpa", "key": artist.name, "albums": artist.album_count,
            "image": picture.unwrap_or_default(), "icon": LIBRARY_ICON})];
        sources.extend(found.remove(&artist.name.trim().to_ascii_lowercase()).into_iter().flatten().map(|(plugin, held)| source(&plugin, held)));
        ListedArtist { artist, sources }
    }).collect();
    listed.extend(found.into_values().map(|plugins| {
        let name = plugins.values().map(|(name, _)| name.clone()).min().unwrap_or_default();
        let records: HashSet<i64> = plugins.values().flat_map(|(_, records)| records.iter().copied()).collect();
        let artist = Artist { name, album_count: records.len(), track_count: 0, artwork_path: String::new() };
        ListedArtist { artist, sources: plugins.into_iter().map(|(plugin, held)| source(&plugin, held)).collect() }
    }));
    listed.sort_by_cached_key(|listed| listed.artist.name.to_lowercase());
    listed
}

/// Artists drawn as record cards: the name where a title goes and how
/// many records stand under it, and beside each the card Artist Scene
/// draws. Only those whose name holds every word of `filter` stay.
pub fn artist_cards(artists: &[ListedArtist], filter: &str) -> (Vec<crate::Album>, Vec<String>) {
    let words: Vec<String> = filter.to_lowercase().split_whitespace().map(String::from).collect();
    artists
        .iter()
        .filter(|listed| {
            let name = listed.artist.name.to_lowercase();
            words.iter().all(|w| name.contains(w.as_str()))
        })
        .map(|listed| {
            let a = &listed.artist;
            let card = crate::Album {
                id: 0,
                artist: chilpa_i18n::tf("{} record", "{} records", a.album_count as i64, &[&a.album_count.to_string()]),
                title: a.name.clone(),
                year: None,
                track_count: a.track_count,
                artwork_path: a.artwork_path.clone(),
                play_count: 0, explicit: None,
            };
            (card, json!({"title": a.name, "sources": listed.sources}).to_string())
        })
        .unzip()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library_sources::SourceItem;
    use crate::FacetEntry;

    fn artist(name: &str, albums: usize) -> Artist {
        Artist { name: name.into(), album_count: albums, track_count: 3, artwork_path: String::new() }
    }

    fn saved(plugin: &str, album: i64, album_artist: &str, artists: &[&str]) -> SourceItem {
        SourceItem { plugin: plugin.into(), album, album_artist: album_artist.into(), artists: artists.iter().map(|a| a.to_string()).collect(),
            saved_album: true, tier: "lossless", format: "flac", ..Default::default() }
    }

    fn index() -> SourceIndex {
        let entry = |plugin: &str, label: &str| FacetEntry { key: format!("plugin:{plugin}"), label: label.into(), icon: format!("file:///{plugin}.svg"), albums: 1, year: None };
        SourceIndex::new(
            vec![saved("one", -1, "MORA VALE", &["MORA VALE"]), saved("one", -2, "Mora Vale", &["Mora Vale", "Guest"]),
                saved("two", -3, "Band", &["Band"]), saved("two", -4, "Various", &["Band", "Various"])],
            vec![entry("one", "One"), entry("two", "Two")],
        )
    }

    fn names(listed: &[ListedArtist]) -> Vec<String> {
        listed.iter().map(|listed| listed.artist.name.clone()).collect()
    }

    #[test]
    fn a_library_artist_wears_the_sources_that_hold_their_records_and_the_others_join_by_name() {
        let found = listed(vec![artist("Mora Vale", 2)], &index(), &Filter::default(), false, |record| format!("https://img/{record}"));
        assert_eq!(names(&found), ["Band", "Mora Vale", "Various Artists"]);
        let worn = &found[1].sources;
        assert_eq!((worn[0]["local"].as_bool(), worn[0]["key"].as_str()), (Some(true), Some("Mora Vale")), "the library first");
        assert_eq!((worn[1]["plugin"].as_str(), worn[1]["name"].as_str(), worn[1]["albums"].as_u64()), (Some("one"), Some("One"), Some(2)));
        assert_eq!(worn[1]["key"], "one matching-artist-v3:MORA VALE", "the source's own spelling, the least of them");
        assert_eq!(worn[1]["image"], "https://img/-1");
        assert_eq!(found[0].sources.len(), 1, "a source's artist the library lacks wears that source alone");
        assert_eq!(found[0].artist.album_count, 1, "only the record credited to them; the compilation is the stand-in's");
    }

    #[test]
    fn a_filtered_out_source_takes_its_artists_away_and_everyone_counts_who_plays() {
        let without = Filter { excluded_sources: vec!["plugin:two".into()], ..Filter::default() };
        assert_eq!(names(&listed(vec![], &index(), &without, false, |_| String::new())), ["MORA VALE"]);
        let everyone = listed(vec![], &index(), &Filter::default(), true, |_| String::new());
        assert_eq!(names(&everyone), ["Band", "Guest", "MORA VALE", "Various Artists"], "the stand-in only on its own compilation");
        let lossy = Filter { qualities: vec!["lossy".into()], formats: vec!["mp3".into()], ..Filter::default() };
        assert_eq!(listed(vec![], &index(), &lossy, false, |_| String::new()).len(), 3, "a record's quality and format do not filter artists");
    }

    #[test]
    fn artists_become_cards_and_the_filter_keeps_every_word() {
        let listed = |name: &str, albums| ListedArtist { artist: artist(name, albums), sources: vec![json!({"local": true})] };
        let artists = [listed("Mora Vale", 2), listed("Vale Mora Trio", 1), listed("Band", 1)];
        let (cards, shown) = artist_cards(&artists, "vale MORA");
        assert_eq!(cards.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["Mora Vale", "Vale Mora Trio"]);
        assert_eq!((cards[0].artist.as_str(), cards[1].artist.as_str()), ("2 records", "1 record"));
        assert_eq!((cards[0].id, cards[0].track_count), (0, 3));
        let card: Value = serde_json::from_str(&shown[0]).unwrap();
        assert_eq!((card["title"].as_str(), card["sources"][0]["local"].as_bool()), (Some("Mora Vale"), Some(true)));
        assert_eq!(artist_cards(&artists, " ").0.len(), 3, "no words keep everyone");
    }
}
