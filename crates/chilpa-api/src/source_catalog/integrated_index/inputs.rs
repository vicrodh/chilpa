use super::{hint::Hint, Source};
use super::super::{composers, genres, library_index, store::{Card, Shelf, Store, Track}, Catalog};
#[cfg(test)]
use super::super::Inner;
use crate::{FacetEntry, library_sources::SourceIndex};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

pub(super) struct Inputs {
    #[cfg(test)]
    requested: Vec<Source>,
    sources: Vec<Source>,
    shelves: Vec<(String, Arc<Shelf>)>,
    cards: BTreeMap<i64, Arc<Card>>,
    tracks: BTreeMap<i64, Arc<Track>>,
    genres: genres::State,
    composers: composers::State,
    at: u64,
    hint: Hint,
}

impl Inputs {
    pub(super) fn capture(catalog: &Catalog, sources: &[Source], at: u64) -> Self {
        #[cfg(test)]
        let requested = sources.to_vec();
        #[cfg(test)]
        catalog.index_captures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut shelves = Vec::new();
        let mut card_ids = HashSet::new();
        let mut track_ids = HashSet::new();
        let (sources, cards, tracks, genres, composers, mut hint) = {
            let inner = catalog.lock();
            for source in sources {
                for section in [source.albums.then_some("albums"), source.tracks.then_some("tracks")].into_iter().flatten() {
                    let key = format!("{} section:{section}", source.id);
                    if let Some(shelf) = inner.store.shelves.shared(&key) {
                        card_ids.extend(shelf.cards.iter().copied());
                        track_ids.extend(shelf.tracks.iter().copied());
                        shelves.push((key, shelf));
                    }
                }
            }
            card_ids.extend(track_ids.iter().filter_map(|id| inner.store.tracks.get(id)).map(|track| track.album_id));
            let cards: BTreeMap<_, _> = card_ids.into_iter().filter_map(|id| inner.store.cards.shared(&id).map(|card| (id, card))).collect();
            track_ids.extend(cards.values().flat_map(|card| card.tracks.iter().flatten()).copied());
            let tracks = track_ids.into_iter().filter_map(|id| inner.store.tracks.shared(&id).map(|track| (id, track))).collect();
            let sources: Vec<_> = sources.iter().cloned().map(|mut source| {
                if let Some(name) = inner.store.names.get(&source.id) { source.name = name.clone(); }
                source
            }).collect();
            let hint = Hint::capture(&inner, &sources, at);
            (sources, cards, tracks, inner.genres.clone(), inner.composers.clone(), hint)
        };
        hint.until = genres.next_change(at).min(composers.next_change(at));
        Self { #[cfg(test)] requested, sources, shelves, cards, tracks, genres, composers, at, hint }
    }

    #[cfg(test)]
    pub(super) fn is_current(&self, inner: &Inner, now: u64) -> bool {
        self.hint.matches(inner, &self.requested, now)
    }

    pub(super) fn into_hint(self) -> Hint { self.hint }

    pub(super) fn fingerprint(&self) -> [u8; 32] {
        let mut writer = Fingerprint(Sha256::new());
        write(&mut writer, &self.sources);
        for (key, shelf) in &self.shelves { write(&mut writer, &(key, &shelf.cards, &shelf.tracks)); }
        for card in self.cards.values() {
            let metadata = ["favorite", "in_library", "tracks", "duration_ms", "genres", "genre", "label", "composers", "composer",
                "added_at", "release_type", "year", "release_date", "upc", "explicit", "ai_generated"].map(|key| &card.metadata[key]);
            write(&mut writer, &(card.id, &card.plugin, &card.identity, &card.kind, &card.title, &card.artist,
                card.year, &card.genre, &card.format, &card.tracks, metadata));
        }
        for track in self.tracks.values() {
            let metadata = ["kind", "ref", "plays", "work", "genres", "genre", "label", "composers", "composer", "added_at",
                "release_type", "year", "release_date", "isrc"].map(|key| &track.metadata[key]);
            write(&mut writer, &(track.id, &track.plugin, track.album_id, &track.path, &track.artist, &track.album, &track.title,
                track.disc, track.number, track.duration_ms, &track.format, track.playable, metadata));
        }
        write(&mut writer, &(self.genres.signature(self.at), self.composers.signature(self.at)));
        writer.0.finalize().into()
    }

    pub(super) fn build(&self) -> SourceIndex {
        let mut store = Store::default();
        for (key, shelf) in &self.shelves { store.shelves.insert_shared(key.clone(), shelf.clone()); }
        for (id, card) in &self.cards { store.cards.insert_shared(*id, card.clone()); }
        for (id, track) in &self.tracks { store.tracks.insert_shared(*id, track.clone()); }
        let albums = self.sources.iter().filter(|source| source.albums).map(|source| source.id.clone()).collect::<Vec<_>>();
        let tracks = self.sources.iter().filter(|source| source.tracks).map(|source| source.id.clone()).collect::<Vec<_>>();
        let chosen = self.sources.iter().map(|source| (source.id.clone(), source.albums, source.tracks)).collect::<Vec<_>>();
        let items = library_index::items_of(&store, &albums, &tracks, &self.genres, &self.composers, self.at);
        let records = library_index::records_of(&store, &chosen);
        let mut sources: Vec<FacetEntry> = Vec::new();
        for item in &items {
            let key = item.source_key();
            if let Some(source) = sources.iter_mut().find(|source| source.key == key) { source.albums += item.tracks; }
            else if let Some(source) = self.sources.iter().find(|source| source.id == item.plugin) {
                sources.push(FacetEntry { key, label: source.name.clone(), albums: item.tracks, year: None, icon: source.icon.clone() });
            }
        }
        let mut index = SourceIndex::new(items, sources);
        index.versions.extend(records.into_iter().map(|record| (record.id, record.version())));
        index
    }
}

struct Fingerprint(Sha256);
impl std::io::Write for Fingerprint {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> { self.0.update(bytes); Ok(bytes.len()) }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
fn write(writer: &mut Fingerprint, value: &impl serde::Serialize) {
    serde_json::to_writer(writer, value).expect("catalog fingerprint writes into an infallible digest");
}
