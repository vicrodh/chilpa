use super::{Inner, Source};
use super::super::{composers, genres};
use std::sync::Arc;

/// A publication owns generation tokens and its small source context.
pub(super) struct Hint {
    cards: Arc<()>,
    tracks: Arc<()>,
    shelves: Arc<()>,
    genres: genres::Hint,
    composers: composers::Hint,
    sources: Vec<Source>,
    epoch: u64,
    at: u64,
    pub(super) until: u64,
}

impl Hint {
    pub(super) fn capture(inner: &Inner, effective_sources: &[Source], at: u64) -> Self {
        Self {
            cards: inner.store.cards.generation(), tracks: inner.store.tracks.generation(), shelves: inner.store.shelves.generation(),
            genres: inner.genres.hint(), composers: inner.composers.hint(), sources: effective_sources.to_vec(),
            epoch: inner.index_epoch, at, until: u64::MAX,
        }
    }

    pub(super) fn matches(&self, inner: &Inner, requested: &[Source], now: u64) -> bool {
        self.epoch == inner.index_epoch && now >= self.at && now < self.until
            && inner.store.cards.matches_generation(&self.cards)
            && inner.store.tracks.matches_generation(&self.tracks)
            && inner.store.shelves.matches_generation(&self.shelves)
            && self.genres.matches(&inner.genres) && self.composers.matches(&inner.composers)
            && self.sources.len() == requested.len()
            && self.sources.iter().zip(requested).all(|(held, current)| {
                held.id == current.id && held.albums == current.albums && held.tracks == current.tracks && held.icon == current.icon
                    && &held.name == inner.store.names.get(&current.id).unwrap_or(&current.name)
            })
    }
}
