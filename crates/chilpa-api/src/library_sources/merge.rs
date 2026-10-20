//! The integrated items in a snapshot, in the order the database gave the
//! library's own tracks: both sides are ordered by the same keys and merged,
//! so nothing of the library is read again to place them.

use super::{passes, SourceIndex, SourceItem, Stars};
use crate::{Filter, Snapshot, TrackOrder};
use chilpa_library::OrderKey;
use std::cmp::Ordering;
use std::collections::HashSet;

/// Which items a screen takes: Library takes every integrated record and
/// track, Tracks only the saved tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Library,
    Tracks,
}

impl Scope {
    fn takes(self, item: &SourceItem) -> bool {
        match self {
            Scope::Library => true,
            Scope::Tracks => item.saved_track && item.track != 0,
        }
    }
}

/// What an item is ordered by, as a track of the library would be.
pub(crate) fn key_of(item: &SourceItem) -> OrderKey {
    OrderKey {
        album_id: item.album,
        album_artist: item.album_artist.clone(),
        album: item.album_title.clone(),
        year: item.year,
        disc: item.disc,
        number: item.number,
        title: item.title.clone(),
        artist: item.artists.first().cloned().unwrap_or_default(),
        added_at: item.added_at,
        duration_ms: item.duration_ms as i64,
        plays: 0,
    }
}

/// The library's snapshot with what the filter leaves of the index merged
/// in, and the keys of the tracks it holds, aligned with its ids.
pub(crate) fn snapshot(local: Snapshot, keys: Vec<OrderKey>, f: &Filter, order: TrackOrder, index: &SourceIndex, stars: &Stars, scope: Scope) -> (Snapshot, Vec<OrderKey>) {
    let mut extra: Vec<(OrderKey, &SourceItem)> = index.items.iter()
        .filter(|item| scope.takes(item) && passes(item, f, stars))
        .map(|item| (key_of(item), item)).collect();
    if extra.is_empty() { return (local, keys); }
    let group = f.grouped_by.as_str();
    extra.sort_by(|a, b| order.compare(group, &a.0, &b.0));
    let mut ids = Vec::with_capacity(local.ids.len() + extra.len());
    let mut merged = Vec::with_capacity(ids.capacity());
    // Where each source record goes among the library's records: after the
    // ones whose first track came before its own.
    let (mut seen, mut placed, mut inserts) = (HashSet::new(), HashSet::new(), Vec::new());
    let (mut tracks, mut duration) = (0, local.totals.duration_ms);
    let mut local_keys = keys.into_iter().zip(local.ids.iter().copied()).peekable();
    let mut extra = extra.into_iter().peekable();
    loop {
        let take_local = match (local_keys.peek(), extra.peek()) {
            (Some((a, _)), Some((b, _))) => order.compare(group, a, b) != Ordering::Greater,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        if take_local {
            let (key, id) = local_keys.next().expect("peeked");
            if key.album_id != 0 { seen.insert(key.album_id); }
            ids.push(id);
            merged.push(key);
            continue;
        }
        let (key, item) = extra.next().expect("peeked");
        if item.album != 0 && placed.insert(item.album) { inserts.push((seen.len(), item.album)); }
        duration += item.duration_ms;
        if item.track != 0 {
            ids.push(item.track);
            merged.push(key);
        } else {
            tracks += item.tracks;
        }
    }
    let mut album_ids = Vec::with_capacity(local.album_ids.len() + inserts.len());
    let mut inserts = inserts.into_iter().peekable();
    for (at, album) in local.album_ids.into_iter().enumerate() {
        while let Some((_, id)) = inserts.next_if(|(before, _)| *before <= at) { album_ids.push(id); }
        album_ids.push(album);
    }
    album_ids.extend(inserts.map(|(_, id)| id));
    let totals = crate::Totals { tracks: ids.len() + tracks, albums: album_ids.len(), duration_ms: duration };
    (Snapshot { ids, album_ids, totals }, merged)
}

/// Where each letter starts in a merged list, as `Library::letters` says
/// it for the library's own: the record's, the artist's or the title's.
pub(crate) fn letters(keys: &[OrderKey], by: &str) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for (at, key) in keys.iter().enumerate() {
        let named = match by { "album" => &key.album, "artist" => &key.artist, _ => &key.title };
        let letter = chilpa_library::letter_of(named);
        if !out.iter().any(|(seen, _)| *seen == letter) { out.push((letter, at)); }
    }
    out
}
