//! What the catalog knows of a source's music, kept between runs.
//!
//! Every record, playlist and artist a source lists gets a negative id the
//! first time it is seen, and keeps it: the album page, a tab, a restored
//! route and the player all name it by that id, the same way they name a
//! record of the library by its positive one. A track gets one too, so a
//! list can select it. The ids are interned by what the source says the
//! thing is, so seeing it again in another listing finds the same id.
//!
//! What a source answered stays on disk. The next run draws a shelf or a
//! record from it at once, and asks again only when it is stale.

use chilpa_plugin::messages::{Entry, EntryKind};
use chilpa_plugin::presentation::{Entity, Presentation, SourceDocument};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Above this many cards the ones seen longest ago go, with their tracks.
const CARDS_KEPT: usize = 6000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CardKind {
    #[default]
    Album,
    Playlist,
    Artist,
    Label,
}

/// A handle into a worker's tree. It is only good while the worker that
/// gave it out runs, which is what the epoch says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Handle {
    pub epoch: u64,
    pub id: String,
}

/// What a source says a format is, kept as the library keeps a server's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Format {
    pub codec: String,
    pub bits: Option<u8>,
    pub rate: Option<u32>,
    #[serde(default)]
    pub advertised_hires: bool,
}

impl Format {
    /// From the technical hints of a listing. A source names a codec the
    /// way its own server does ("flc"); the library's names are used.
    pub fn of(presentation: &Presentation) -> Option<Format> {
        let q = &presentation.advertised_quality;
        let named = q.format.to_ascii_lowercase();
        let bits = q.bit_depth.and_then(|b| u8::try_from(b).ok()).filter(|b| *b > 0);
        let codec = match named.as_str() {
            "flc" | "flac" => "flac",
            "alc" | "alac" => "alac",
            "mp4" | "m4a" if bits.is_some() => "alac",
            "aac" | "mp4" | "m4a" => "aac",
            "mp3" => "mp3",
            "ogg" | "vorbis" => "vorbis",
            "ops" | "opus" => "opus",
            "wav" | "pcm" => "wav",
            "aif" | "aiff" => "aiff",
            "dsf" | "dff" | "dsd" => "dsd",
            _ => "",
        };
        let rate = q.sample_rate_hz.filter(|r| *r > 0);
        (!codec.is_empty() || bits.is_some() || rate.is_some()).then(|| Format { codec: codec.into(), bits, rate, advertised_hires: false })
    }

    pub fn quality(&self) -> crate::quality::Quality {
        let mut quality = crate::quality::Quality::of_codec(&self.codec, self.bits, self.rate);
        if self.advertised_hires && self.bits.is_none() && self.rate.is_none() && quality.tier == crate::quality::Tier::Lossless {
            quality.tier = crate::quality::Tier::HiRes;
            quality.format = format!("{} Hi-Res", quality.format);
        }
        quality
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Card {
    pub id: i64,
    pub plugin: String,
    /// What the source says it is, stable between runs when it says so.
    pub identity: String,
    pub kind: CardKind,
    pub title: String,
    pub metadata: serde_json::Value,
    pub artist: String,
    pub year: Option<i32>,
    pub genre: String,
    pub last_played_at: Option<i64>,
    /// The cover's address, and the cover on disk once it was fetched.
    pub image: String,
    pub art: String,
    pub format: Option<Format>,
    /// Its tracks in order, once read.
    pub tracks: Option<Vec<i64>>,
    /// When its tracks were read, in Unix seconds.
    pub read_at: u64,
    /// When a listing last showed it, in Unix seconds, and which listing:
    /// reading it again is how a lost handle is found.
    pub seen_at: u64,
    pub seen_in: String,
    #[serde(skip)]
    pub handle: Option<Handle>,
}

impl Card {
    pub fn contain_art(&self) -> bool {
        match self.metadata["art_fit"].as_str() {
            Some(fit) => fit == "contain",
            None => self.kind == CardKind::Playlist && !self.image.is_empty(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Track {
    pub id: i64,
    /// Its queue entry: what the player resolves when it comes up.
    pub path: String,
    pub plugin: String,
    pub title: String,
    pub metadata: serde_json::Value,
    pub artist: String,
    pub album: String,
    /// The card it was read under, or zero.
    pub album_id: i64,
    pub disc: u16,
    pub number: u16,
    pub duration_ms: u64,
    pub format: Option<Format>,
    pub image: String,
    pub playable: bool,
}

/// A folder of the source's tree that is neither a record nor a list.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Folder {
    pub id: String,
    pub title: String,
}

/// One listing, as the catalog keeps it: what records, tracks and folders
/// it holds, in its order.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Shelf {
    pub document: serde_json::Value,
    pub title: String,
    pub cards: Vec<i64>,
    pub tracks: Vec<i64>,
    /// Folders are handles into a worker: they are not kept between runs.
    #[serde(skip)]
    pub folders: Vec<Folder>,
    /// What each card is to the listener here ("favorites", "purchases").
    pub membership: BTreeMap<i64, String>,
    pub read_at: u64,
    #[serde(skip)]
    pub epoch: u64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Store {
    next_card: i64,
    next_track: i64,
    pub cards: BTreeMap<i64, Card>,
    pub tracks: BTreeMap<i64, Track>,
    #[serde(serialize_with = "lasting")]
    pub shelves: BTreeMap<String, Shelf>,
    /// Whether each plugin is drawn as records, so a restored page knows
    /// before the plugins have started.
    pub drawn: BTreeMap<String, bool>,
    pub names: BTreeMap<String, String>,
    #[serde(skip)]
    card_ids: HashMap<(String, String), i64>,
    #[serde(skip)]
    track_ids: HashMap<String, i64>,
    /// The covers the listener chose for records, by plugin and identity.
    /// The library keeps them; this is what is shown.
    #[serde(skip)]
    pub chosen: HashMap<(String, String), String>,
}

/// What a listing turned into: the ids it holds, in its order.
#[derive(Default)]
pub(crate) struct Read {
    pub cards: Vec<i64>,
    /// Each card's entry id in the listing, for what the listing says of it.
    pub entries: Vec<(String, i64)>,
    pub tracks: Vec<i64>,
    pub folders: Vec<Folder>,
}

/// Only listings that can be found again are written down: a folder's
/// handle dies with its worker.
fn lasting<S: serde::Serializer>(shelves: &BTreeMap<String, Shelf>, to: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let kept: Vec<_> = shelves.iter().filter(|(key, _)| !super::split(key).1.starts_with("at:")).collect();
    let mut map = to.serialize_map(Some(kept.len()))?;
    for (key, shelf) in kept {
        map.serialize_entry(key, shelf)?;
    }
    map.end()
}

fn kind_of(entity: Entity) -> Option<CardKind> {
    match entity {
        Entity::Album => Some(CardKind::Album),
        Entity::Playlist => Some(CardKind::Playlist),
        Entity::Artist => Some(CardKind::Artist),
        _ => None,
    }
}

fn year_of(presentation: &Presentation) -> Option<i32> {
    i32::try_from(presentation.year).ok().filter(|y| *y > 0)
}

/// A line that ends in a year in brackets, "Band (1992)", is the artist and
/// the year: it is how a source without a field for it says it.
pub(crate) fn split_year(line: &str) -> (String, Option<i32>) {
    let trimmed = line.trim_end();
    let year = trimmed.strip_suffix(')').and_then(|rest| rest.rsplit_once(" ("))
        .and_then(|(name, year)| Some((name, year.parse::<i32>().ok().filter(|y| (1900..=2100).contains(y))?)));
    match year {
        Some((name, year)) if !name.trim().is_empty() => (name.trim().to_string(), Some(year)),
        _ => (line.to_string(), None),
    }
}

fn http(address: Option<&String>) -> String {
    address.filter(|a| a.starts_with("https://") || a.starts_with("http://")).cloned().unwrap_or_default()
}

impl Store {
    pub fn forget_plugin(&mut self, plugin: &str) {
        self.cards.retain(|_, c| c.plugin != plugin);
        self.tracks.retain(|_, t| t.plugin != plugin);
        self.shelves.retain(|key, _| !key.starts_with(&format!("{plugin} ")));
        self.card_ids.retain(|(p, _), _| p != plugin);
        self.track_ids.retain(|_, id| self.tracks.contains_key(id));
        self.drawn.remove(plugin);
        self.names.remove(plugin);
    }

    /// Rebuilds what is not written down: the indexes by identity.
    pub fn index(&mut self) {
        self.card_ids = self.cards.values().map(|c| ((c.plugin.clone(), c.identity.clone()), c.id)).collect();
        self.track_ids = self.tracks.values().map(|t| (t.path.clone(), t.id)).collect();
    }

    pub fn card_id(&mut self, plugin: &str, identity: &str) -> i64 {
        if let Some(id) = self.card_ids.get(&(plugin.to_string(), identity.to_string())) {
            return *id;
        }
        self.next_card -= 1;
        let id = self.next_card;
        self.card_ids.insert((plugin.to_string(), identity.to_string()), id);
        self.cards.insert(id, Card { id, plugin: plugin.into(), identity: identity.into(), ..Card::default() });
        id
    }

    pub(super) fn track_id(&mut self, path: &str) -> i64 {
        if let Some(id) = self.track_ids.get(path) {
            return *id;
        }
        self.next_track -= 1;
        let id = self.next_track;
        self.track_ids.insert(path.to_string(), id);
        self.tracks.insert(id, Track { id, path: path.into(), ..Track::default() });
        id
    }

    /// Whether a card for this identity was read, not only named.
    pub(super) fn has_read(&self, plugin: &str, identity: &str) -> bool {
        self.card_ids.get(&(plugin.to_string(), identity.to_string()))
            .and_then(|id| self.cards.get(id)).is_some_and(|c| !c.title.is_empty())
    }

    pub fn track_by_path(&self, path: &str) -> Option<&Track> {
        self.tracks.get(self.track_ids.get(path)?)
    }

    /// Takes in one page of a listing: what it lists is interned and what it
    /// says of each thing replaces what was known.
    pub fn take(&mut self, plugin: &str, epoch: u64, document: &SourceDocument, now: u64) -> Read {
        let mut read = Read::default();
        for entry in &document.listing.entries {
            let shown = document.presentation.get(&entry.id).cloned().unwrap_or_default();
            match (entry.kind, kind_of(shown.entity)) {
                (EntryKind::Folder, Some(kind)) => {
                    let id = self.take_card(plugin, epoch, entry, &shown, kind, now);
                    read.cards.push(id);
                    read.entries.push((entry.id.clone(), id));
                }
                (EntryKind::Item, _) if !entry.live && (entry.playable || shown.entity == Entity::Track) => read.tracks.push(self.take_track(plugin, entry, &shown, None)),
                // A folder that does something when opened (it removes a
                // favorite, it signs out) is not a place: it is not shown.
                (EntryKind::Folder, None) if shown.can_preview => read.folders.push(Folder { id: entry.id.clone(), title: entry.title.clone() }),
                _ => {}
            }
        }
        read
    }

    fn take_card(&mut self, plugin: &str, epoch: u64, entry: &Entry, shown: &Presentation, kind: CardKind, now: u64) -> i64 {
        let identity = if shown.stable_id.is_empty() { format!("handle:{}", entry.id) } else { shown.stable_id.clone() };
        let id = self.card_id(plugin, &identity);
        let card = self.cards.get_mut(&id).expect("interned");
        card.kind = kind;
        card.title = if shown.title.is_empty() { entry.title.clone() } else { shown.title.clone() };
        let artist = shown.metadata.get("artist").cloned().unwrap_or_default();
        let artist = [artist, shown.secondary.clone(), entry.subtitle.clone()].into_iter().find(|a| !a.trim().is_empty()).unwrap_or_default();
        let (artist, said) = split_year(&artist);
        card.artist = artist;
        card.year = year_of(shown).or(said).or(card.year);
        if let Some(genre) = shown.metadata.get("genre").filter(|g| !g.is_empty()) {
            card.genre = genre.clone();
        }
        let image = http(entry.image_url.as_ref());
        if !image.is_empty() && image != card.image {
            card.image = image;
            card.art.clear();
        }
        card.format = Format::of(shown).or(card.format.take());
        card.handle = Some(Handle { epoch, id: entry.id.clone() });
        card.seen_at = now;
        id
    }

    /// A track as a listing shows it. Under a record it is that record's:
    /// what the source does not say of it, the record says.
    pub fn take_track(&mut self, plugin: &str, entry: &Entry, shown: &Presentation, under: Option<&Card>) -> i64 {
        let path = crate::source_opener::entry_of(plugin, &entry.id);
        let id = self.track_id(&path);
        let track = self.tracks.get_mut(&id).expect("interned");
        track.plugin = plugin.into();
        track.title = if shown.title.is_empty() { entry.title.clone() } else { shown.title.clone() };
        let meta = |key: &str| shown.metadata.get(key).filter(|v| !v.trim().is_empty()).cloned();
        track.artist = meta("artist").or_else(|| Some(shown.secondary.clone()).filter(|s| !s.trim().is_empty()))
            .or_else(|| under.filter(|c| c.kind == CardKind::Album).map(|c| c.artist.clone()))
            .unwrap_or_else(|| entry.subtitle.clone());
        track.album = meta("album").or_else(|| under.filter(|c| c.kind == CardKind::Album).map(|c| c.title.clone())).unwrap_or_default();
        track.album_id = under.filter(|c| c.kind == CardKind::Album).map(|c| c.id).unwrap_or(track.album_id);
        track.disc = u16::try_from(shown.disc_number).unwrap_or(0);
        track.number = u16::try_from(shown.track_number).unwrap_or(0);
        track.duration_ms = entry.duration_ms;
        track.format = Format::of(shown).or(track.format.take());
        let image = http(entry.image_url.as_ref());
        track.image = if image.is_empty() { under.map(|c| c.image.clone()).unwrap_or_default() } else { image };
        track.playable = entry.playable;
        id
    }

    /// A card's cover as shown: the one the listener chose, over the one
    /// fetched.
    pub fn shown_art(&self, card: &Card) -> String {
        if let Some(chosen) = self.chosen.get(&(card.plugin.clone(), card.identity.clone())) {
            return chosen.clone();
        }
        if !card.art.is_empty() && std::path::Path::new(&card.art).is_file() {
            return card.art.clone();
        }
        crate::pictures::kept(&card.image).unwrap_or_default()
    }

    /// The cover on disk for a track: its record's, when it has one.
    pub fn art_of(&self, track: &Track) -> String {
        self.cards.get(&track.album_id).map(|c| self.shown_art(c)).filter(|a| !a.is_empty())
            .or_else(|| crate::pictures::kept(&track.image)).unwrap_or_default()
    }

    /// Keeps what fits: cards no kept listing names, seen longest ago, go
    /// first, and the tracks only they held go with them.
    pub fn prune(&mut self) {
        if self.cards.len() <= CARDS_KEPT {
            return;
        }
        let listed: std::collections::HashSet<i64> = self.shelves.values().flat_map(|s| s.cards.iter().copied()).collect();
        let mut old: Vec<(u64, i64)> = self.cards.values().filter(|c| !listed.contains(&c.id)).map(|c| (c.seen_at, c.id)).collect();
        old.sort();
        for (_, id) in old.into_iter().take(self.cards.len() - CARDS_KEPT) {
            self.cards.remove(&id);
        }
        let held: std::collections::HashSet<i64> = self.cards.values().flat_map(|c| c.tracks.iter().flatten().copied())
            .chain(self.shelves.values().flat_map(|s| s.tracks.iter().copied())).collect();
        self.tracks.retain(|id, _| held.contains(id));
        self.index();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document(value: serde_json::Value) -> SourceDocument {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_record_keeps_its_id_across_listings_and_handles() {
        let mut store = Store::default();
        let page = |handle: &str| document(json!({"entries":[{"id":handle,"title":"Record","subtitle":"Band","image_url":"https://img/a.jpg"}],
            "presentation":{handle:{"entity":"album","stable_id":"album:7","year":1999,"advertised_quality":{"format":"flc","bit_depth":24,"sample_rate_hz":96000}}}}));
        let first = store.take("guest", 1, &page("menu:a:1"), 10);
        let again = store.take("guest", 2, &page("menu:b:9"), 20);
        assert_eq!(first.cards, again.cards);
        let card = &store.cards[&first.cards[0]];
        assert!(card.id < 0, "remote ids never meet the library's");
        assert_eq!((card.title.as_str(), card.artist.as_str(), card.year), ("Record", "Band", Some(1999)));
        assert_eq!(card.handle, Some(Handle { epoch: 2, id: "menu:b:9".into() }));
        assert_eq!(card.format.as_ref().unwrap().quality().short(), "FLAC 24/96");
    }

    #[test]
    fn a_year_in_brackets_after_the_artist_is_the_year() {
        assert_eq!(split_year("Alice In Chains (1992)"), ("Alice In Chains".to_string(), Some(1992)));
        assert_eq!(split_year("Band (Live)"), ("Band (Live)".to_string(), None));
        assert_eq!(split_year("(1992)"), ("(1992)".to_string(), None));
    }

    #[test]
    fn a_track_under_a_record_takes_what_it_does_not_say_from_it() {
        let mut store = Store::default();
        let album = store.take("guest", 1, &document(json!({"entries":[{"id":"h","title":"Record","subtitle":"Band"}],
            "presentation":{"h":{"entity":"album","stable_id":"album:1"}}})), 0).cards[0];
        let under = store.cards[&album].clone();
        let entry: Entry = serde_json::from_value(json!({"id":"guest://9","title":"Song","kind":"item","duration_ms":1000})).unwrap();
        let shown: Presentation = serde_json::from_value(json!({"entity":"track","track_number":3})).unwrap();
        let id = store.take_track("guest", &entry, &shown, Some(&under));
        let track = &store.tracks[&id];
        assert_eq!((track.artist.as_str(), track.album.as_str(), track.album_id, track.number), ("Band", "Record", album, 3));
        assert_eq!(store.track_by_path(&track.path).map(|t| t.id), Some(id));
    }

    #[test]
    fn written_down_and_read_back_the_ids_hold() {
        let mut store = Store::default();
        let id = store.card_id("guest", "album:1");
        let text = serde_json::to_string(&store).unwrap();
        let mut back: Store = serde_json::from_str(&text).unwrap();
        back.index();
        assert_eq!(back.card_id("guest", "album:1"), id);
        assert_ne!(back.card_id("guest", "album:2"), id);
    }

    #[test]
    fn a_folder_s_listing_is_not_written_down_and_a_section_is() {
        let mut store = Store::default();
        store.shelves.insert("guest at:menu:1".into(), Shelf::default());
        store.shelves.insert("guest section:library".into(), Shelf::default());
        let back: Store = serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
        assert_eq!(back.shelves.keys().collect::<Vec<_>>(), ["guest section:library"]);
    }
}
