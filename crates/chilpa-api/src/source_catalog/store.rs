//! What the catalog knows of a source's music, kept between runs.
//!
//! I assign negative ids to source records, playlists, artists and tracks.
//! Retained entries and live roots keep their ids across listings and runs.
//! An evicted identity may receive a new id when it returns; monotonically
//! decreasing counters prevent an old id from naming a different entry.
//!
//! I restore retained shelves from disk and refresh stale source answers.

use super::shared_map::SharedMap;
use chilpa_plugin::messages::{Entry, EntryKind};
use chilpa_plugin::presentation::{Entity, Presentation, SourceDocument};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Folder {
    pub id: String,
    pub title: String,
}

/// One listing, as the catalog keeps it: what records, tracks and folders
/// it holds, in its order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
pub(crate) struct PlaylistSummary {
    pub id: i64,
    pub plugin: String,
    pub identity: String,
    pub title: String,
    pub description: String,
    pub art: String,
    pub track_count: usize,
    pub updated_at: Option<i64>,
    pub editable: bool,
    pub membership: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ColdSource {
    pub cards: BTreeMap<i64, String>,
    pub tracks: BTreeMap<i64, String>,
    pub playlists: Vec<PlaylistSummary>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Store {
    next_card: i64,
    next_track: i64,
    pub cards: SharedMap<i64, Card>,
    pub tracks: SharedMap<i64, Track>,
    #[serde(serialize_with = "lasting")]
    pub shelves: SharedMap<String, Shelf>,
    /// Whether each plugin is drawn as records, so a restored page knows
    /// before the plugins have started.
    pub drawn: BTreeMap<String, bool>,
    pub names: BTreeMap<String, String>,
    /// I reserve archived identities without retaining their payloads or
    /// duplicating the source prefix of every track's queue entry.
    pub cold_sources: BTreeMap<String, ColdSource>,
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
fn lasting<S: serde::Serializer>(shelves: &SharedMap<String, Shelf>, to: S) -> Result<S::Ok, S::Error> {
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
        self.cold_sources.remove(plugin);
    }

    /// Rebuilds what is not written down: the indexes by identity.
    pub fn index(&mut self) {
        self.card_ids = self.cards.values().map(|c| ((c.plugin.clone(), c.identity.clone()), c.id)).collect();
        self.track_ids = self.tracks.values().map(|t| (t.path.clone(), t.id)).collect();
        self.next_card = self.next_card.min(self.card_ids.values().copied().chain(self.cold_sources.values().flat_map(|source| source.cards.keys().copied())).min().unwrap_or(0));
        self.next_track = self.next_track.min(self.tracks.keys().copied().chain(self.cold_sources.values().flat_map(|source| source.tracks.keys().copied())).min().unwrap_or(0));
    }

    pub fn card_id(&mut self, plugin: &str, identity: &str) -> i64 {
        if let Some(id) = self.card_ids.get(&(plugin.to_string(), identity.to_string())) {
            let id = *id;
            if !self.cards.contains_key(&id) { self.cards.insert(id, Card { id, plugin: plugin.into(), identity: identity.into(), ..Card::default() }); }
            return id;
        }
        let id = self.cold_sources.get(plugin).and_then(|source| source.cards.iter().find(|(_, reference)| reference.as_str() == identity).map(|(id, _)| *id))
            .unwrap_or_else(|| { self.next_card -= 1; self.next_card });
        self.card_ids.insert((plugin.to_string(), identity.to_string()), id);
        self.cards.insert(id, Card { id, plugin: plugin.into(), identity: identity.into(), ..Card::default() });
        id
    }

    pub(super) fn track_id(&mut self, path: &str) -> i64 {
        if let Some(id) = self.track_ids.get(path) {
            let id = *id;
            if !self.tracks.contains_key(&id) { self.tracks.insert(id, Track { id, path: path.into(), ..Track::default() }); }
            return id;
        }
        let id = crate::source_opener::of_plugin(path).and_then(|(plugin, reference)| self.cold_sources.get(&plugin)
            .and_then(|source| source.tracks.iter().find(|(_, found)| found.as_str() == reference.as_str()).map(|(id, _)| *id)))
            .unwrap_or_else(|| { self.next_track -= 1; self.next_track });
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
        self.track_ids.get(path).and_then(|id| self.tracks.get(id))
            .or_else(|| self.tracks.values().find(|track| track.path == path))
    }

    pub(super) fn snapshot(&self) -> Self {
        Self { next_card: self.next_card, next_track: self.next_track,
            cards: self.cards.clone(), tracks: self.tracks.clone(), shelves: self.shelves.clone(),
            drawn: self.drawn.clone(), names: self.names.clone(), cold_sources: self.cold_sources.clone(), ..Self::default() }
    }

    pub(super) fn source_snapshot(&self, plugin: &str) -> Self {
        let mut snapshot = self.snapshot();
        snapshot.cards.retain(|_, card| card.plugin == plugin);
        snapshot.tracks.retain(|_, track| track.plugin == plugin);
        snapshot.shelves.retain(|key, _| super::split(key).0 == plugin);
        snapshot.drawn.retain(|id, _| id == plugin);
        snapshot.names.retain(|id, _| id == plugin);
        snapshot.cold_sources.clear();
        snapshot
    }

    pub(super) fn playlist_summaries(&self) -> Vec<PlaylistSummary> {
        self.shelves.iter().filter(|(key, _)| key.ends_with(" section:playlists")).flat_map(|(_, shelf)| {
            shelf.cards.iter().filter_map(|id| self.cards.get(id)).filter(|card| card.metadata["kind"] == "playlist" && card.metadata["ref"].as_str().is_some())
                .map(move |card| PlaylistSummary {
                    id:card.id, plugin:card.plugin.clone(), identity:card.identity.clone(), title:card.title.clone(),
                    description:card.metadata["description"].as_str().unwrap_or_default().into(), art:card.art.clone(),
                    track_count:card.metadata["tracks"].as_u64().unwrap_or_default() as usize, updated_at:card.metadata["updated_at"].as_i64(),
                    editable:card.metadata["editable"] == true, membership:shelf.membership.get(&card.id).cloned().unwrap_or_default(),
                })
        }).collect()
    }

    /// I reject a stale disk write before detaching any newer live rows.
    pub(super) fn same_source(&self, snapshot: &Self, plugin: &str) -> bool {
        self.cards.iter().filter(|(_, card)| card.plugin == plugin).count() == snapshot.cards.len()
            && self.tracks.iter().filter(|(_, track)| track.plugin == plugin).count() == snapshot.tracks.len()
            && self.shelves.iter().filter(|(key, _)| super::split(key).0 == plugin).count() == snapshot.shelves.len()
            && snapshot.cards.iter().all(|(id, card)| self.cards.get(id) == Some(card))
            && snapshot.tracks.iter().all(|(id, track)| self.tracks.get(id) == Some(track))
            && snapshot.shelves.iter().all(|(key, shelf)| self.shelves.get(key) == Some(shelf))
    }

    /// I merge existing ids before rebuilding indexes. Newer resident rows
    /// win over archived metadata; an unopened card can recover its rows.
    pub(super) fn merge_source(&mut self, source: &Self) -> std::io::Result<()> {
        if source.cards.iter().any(|(id, card)| self.cards.get(id).is_some_and(|live| live.plugin != card.plugin || live.identity != card.identity))
            || source.tracks.iter().any(|(id, track)| self.tracks.get(id).is_some_and(|live| live.plugin != track.plugin || live.path != track.path)) {
            return Err(std::io::Error::other("Archived catalog ids conflict with live entries"));
        }
        self.next_card = self.next_card.min(source.next_card);
        self.next_track = self.next_track.min(source.next_track);
        for (id, card) in &source.cards {
            if let Some(live) = self.cards.get_mut(id) {
                if live.title.is_empty() { *live = card.clone(); }
                else if live.tracks.is_none() && card.tracks.is_some() && live.metadata == card.metadata {
                    live.tracks = card.tracks.clone();
                    live.read_at = live.read_at.min(card.read_at);
                }
            } else { self.cards.insert_shared(*id, source.cards.shared(id).expect("archived card exists")); }
        }
        for (id, _) in &source.tracks { if !self.tracks.contains_key(id) { self.tracks.insert_shared(*id, source.tracks.shared(id).expect("archived track exists")); } }
        for (key, _) in &source.shelves { if !self.shelves.contains_key(key) { self.shelves.insert_shared(key.clone(), source.shelves.shared(key).expect("archived shelf exists")); } }
        for (id, drawn) in &source.drawn { self.drawn.entry(id.clone()).or_insert(*drawn); }
        for (id, name) in &source.names { self.names.entry(id.clone()).or_insert_with(|| name.clone()); }
        self.index();
        Ok(())
    }

    /// I keep open card rows and individual track roots, with their album navigation.
    pub(super) fn retain_source_roots(&mut self, plugin: &str, tracks: &HashSet<String>, cards: &HashSet<i64>, shelves: &HashSet<String>) {
        let families: HashSet<_> = shelves.iter().map(|key| key.split('|').next().unwrap_or(key)).collect();
        let mut full_cards = cards.clone();
        let mut kept_tracks: HashSet<_> = self.tracks.values().filter(|track| tracks.contains(&track.path)).map(|track| track.id).collect();
        for (key, shelf) in &self.shelves {
            if families.contains(key.split('|').next().unwrap_or(key)) { full_cards.extend(&shelf.cards); kept_tracks.extend(&shelf.tracks); }
        }
        kept_tracks.extend(full_cards.iter().filter_map(|id| self.cards.get(id)).flat_map(|card| card.tracks.iter().flatten()).copied());
        let mut kept_cards = full_cards.clone();
        kept_cards.extend(kept_tracks.iter().filter_map(|id| self.tracks.get(id)).map(|track| track.album_id).filter(|id| *id < 0));
        self.shelves.retain(|key, _| super::split(key).0 != plugin || families.contains(key.split('|').next().unwrap_or(key)));
        self.cards.retain(|id, card| card.plugin != plugin || kept_cards.contains(id));
        for card in self.cards.values_mut_where(|card| card.plugin == plugin && !full_cards.contains(&card.id) && (card.tracks.is_some() || card.read_at != 0)) {
            card.tracks = None;
            card.read_at = 0;
        }
        self.tracks.retain(|id, track| track.plugin != plugin || kept_tracks.contains(id));
        self.index();
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

    /// I keep collection membership and live roots, then evict historical pages first.
    pub fn prune(&mut self, protected: &std::collections::HashSet<String>, protected_cards: &std::collections::HashSet<i64>) {
        self.prune_with_shelves(protected, protected_cards, &Default::default());
    }

    pub(super) fn prune_with_shelves(&mut self, protected: &std::collections::HashSet<String>, protected_cards: &std::collections::HashSet<i64>, protected_shelves: &std::collections::HashSet<String>) {
        use std::collections::HashSet;
        const SHELVES_KEPT: usize = 64;
        const DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
        fn family(key: &str) -> &str { key.split('|').next().unwrap_or(key) }
        fn collection(key: &str) -> bool { super::split(key).1.starts_with("section:") || super::split(key).1 == "library" }
        fn weight(value: &serde_json::Value) -> usize {
            match value {
                serde_json::Value::String(text) => text.len() + 32,
                serde_json::Value::Array(values) => values.iter().map(weight).sum::<usize>() + values.len() * 32,
                serde_json::Value::Object(fields) => fields.iter().map(|(key, value)| key.len() + weight(value) + 64).sum(),
                _ => 16,
            }
        }
        let protected_families: HashSet<_> = protected_shelves.iter().map(|key| family(key).to_string()).collect();
        let live = |key: &str| collection(key) || protected_families.contains(family(key));
        let mut families: HashMap<String, (u64, usize, usize)> = HashMap::new();
        for (key, shelf) in &self.shelves {
            if live(key) { continue; }
            let item = families.entry(family(key).to_string()).or_default();
            item.0 = item.0.max(shelf.read_at);
            item.1 += 1;
            item.2 += weight(&shelf.document);
        }
        let mut count: usize = families.values().map(|entry| entry.1).sum();
        let mut bytes: usize = families.values().map(|entry| entry.2).sum();
        let mut oldest: Vec<_> = families.into_iter().collect();
        oldest.sort_by(|(first_key, first), (second_key, second)| (first.0, first_key).cmp(&(second.0, second_key)));
        let mut removed = HashSet::new();
        for (key, (_, shelves, size)) in oldest {
            if count <= SHELVES_KEPT && bytes <= DOCUMENT_BYTES { break; }
            removed.insert(key); count -= shelves; bytes -= size;
        }
        self.shelves.retain(|key, _| !removed.contains(family(key)));
        let pinned: HashSet<i64> = protected.iter().filter_map(|path| self.track_ids.get(path).copied()).collect();
        let mut roots: HashSet<i64> = self.shelves.iter().filter(|(key, _)| live(key)).flat_map(|(_, shelf)| shelf.cards.iter().copied())
            .chain(protected_cards.iter().copied()).collect();
        roots.extend(pinned.iter().filter_map(|id| self.tracks.get(id)).map(|track| track.album_id));
        // Loose tracks in an open page also keep their album navigation alive.
        roots.extend(self.shelves.iter().filter(|(key, _)| live(key)).flat_map(|(_, shelf)| shelf.tracks.iter())
            .filter_map(|id| self.tracks.get(id)).map(|track| track.album_id));
        let related: Vec<_> = roots.iter().filter_map(|id| self.cards.get(id)).flat_map(|card| card.tracks.iter().flatten())
            .filter_map(|id| self.tracks.get(id)).map(|track| track.album_id).filter(|id| *id < 0).collect();
        roots.extend(related);
        let mut old: Vec<_> = self.cards.values().filter(|card| !roots.contains(&card.id)).map(|card| (card.seen_at, card.id)).collect();
        old.sort();
        for (_, id) in old.into_iter().take(self.cards.len().saturating_sub(CARDS_KEPT)) { self.cards.remove(&id); }
        let kept_cards: HashSet<_> = self.cards.keys().copied().collect();
        let stale: HashSet<_> = self.shelves.iter().filter(|(key, shelf)| !live(key) && shelf.cards.iter().any(|id| !kept_cards.contains(id)))
            .map(|(key, _)| family(key).to_string()).collect();
        self.shelves.retain(|key, _| !stale.contains(family(key)));
        const TRACKS_KEPT: usize = 30_000;
        let mut references: HashMap<i64, usize> = HashMap::new();
        for id in self.cards.values().flat_map(|card| card.tracks.iter().flatten().copied())
            .chain(self.shelves.values().flat_map(|shelf| shelf.tracks.iter().copied())).chain(pinned.iter().copied()) {
            if self.tracks.contains_key(&id) { *references.entry(id).or_default() += 1; }
        }
        // I keep collection cards, but only active card consumers retain their full row lists.
        let protected_rows: HashSet<_> = self.cards.values().filter(|card| protected_cards.contains(&card.id)).flat_map(|card| card.tracks.iter().flatten().copied())
            .chain(self.shelves.iter().filter(|(key, _)| live(key)).flat_map(|(_, shelf)| shelf.tracks.iter().copied()))
            .chain(pinned).filter(|id| self.tracks.contains_key(id)).collect();
        let track_limit = TRACKS_KEPT.max(protected_rows.len());
        fn release(references: &mut HashMap<i64, usize>, ids: impl IntoIterator<Item = i64>) {
            for id in ids {
                if let Some(count) = references.get_mut(&id) {
                    *count -= 1;
                    if *count == 0 { references.remove(&id); }
                }
            }
        }
        if references.len() > track_limit {
            let mut cached: Vec<_> = self.cards.values().filter(|card| !protected_cards.contains(&card.id) && card.tracks.is_some())
                .map(|card| (card.read_at, card.id)).collect();
            cached.sort();
            for (_, id) in cached {
                if references.len() <= track_limit { break; }
                if let Some(card) = self.cards.get_mut(&id) {
                    release(&mut references, card.tracks.take().unwrap_or_default());
                    card.read_at = 0;
                }
            }
        }
        // Historical loose-track pages also share this budget; live collections stay complete.
        if references.len() > track_limit {
            let mut cached: Vec<_> = self.shelves.iter().filter(|(key, _)| !live(key))
                .map(|(key, shelf)| (shelf.read_at, key.clone())).collect();
            cached.sort();
            for (_, key) in cached {
                if references.len() <= track_limit { break; }
                if let Some(shelf) = self.shelves.get(&key) { release(&mut references, shelf.tracks.iter().copied()); }
                self.shelves.remove(&key);
            }
        }
        self.tracks.retain(|id, _| references.contains_key(id));
        self.card_ids.retain(|_, id| self.cards.contains_key(id));
        self.track_ids.retain(|_, id| self.tracks.contains_key(id));
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
