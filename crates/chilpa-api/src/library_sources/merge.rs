//! The integrated items in a snapshot, in the order the database gave the
//! library's own tracks: both sides are ordered by the same keys and merged,
//! so nothing of the library is read again to place them.

use super::{Prepared, SourceIndex, SourceItem, Stars};
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

/// The letter a merged row is filed under for `by` ("album", "artist", or
/// the title), as `Library::letters` says it for the library's own.
fn letter(key: &OrderKey, by: &str) -> String {
    chilpa_library::letter_of(match by { "album" => &key.album, "artist" => &key.artist, _ => &key.title })
}

/// The library's snapshot with what the filter leaves of the index merged
/// in. With `letters_by`, also where each letter starts among the tracks.
#[allow(clippy::too_many_arguments)]
pub(crate) fn snapshot(local: Snapshot, keys: Vec<OrderKey>, f: &Filter, order: TrackOrder, index: &SourceIndex, stars: &Stars, scope: Scope, letters_by: Option<&str>)
    -> (Snapshot, Vec<(String, usize)>) {
    let filter = Prepared::new(f);
    let mut extra: Vec<(&OrderKey, &SourceItem)> = index.keys.iter().zip(&index.items)
        .filter(|(_, item)| scope.takes(item) && filter.passes(item, stars)).collect();
    let group = f.grouped_by.as_str();
    let mut letters: Vec<(String, usize)> = Vec::new();
    let mut note = |key: &OrderKey, at: usize| if let Some(by) = letters_by {
        let found = letter(key, by);
        if !letters.iter().any(|(seen, _)| *seen == found) { letters.push((found, at)); }
    };
    if extra.is_empty() {
        for (at, key) in keys.iter().enumerate() { note(key, at); }
        return (local, letters);
    }
    extra.sort_by(|a, b| order.compare(group, a.0, b.0));
    let mut ids = Vec::with_capacity(local.ids.len() + extra.len());
    // Where each source record goes among the library's records: after the
    // ones whose first track came before its own.
    let (mut seen, mut placed, mut inserts) = (HashSet::new(), HashSet::new(), Vec::new());
    let (mut tracks, mut duration) = (0, local.totals.duration_ms);
    let mut local_rows = keys.iter().zip(local.ids.iter().copied()).peekable();
    let mut extra = extra.into_iter().peekable();
    loop {
        let take_local = match (local_rows.peek(), extra.peek()) {
            (Some((a, _)), Some((b, _))) => order.compare(group, a, b) != Ordering::Greater,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        if take_local {
            let (key, id) = local_rows.next().expect("peeked");
            if key.album_id != 0 { seen.insert(key.album_id); }
            note(key, ids.len());
            ids.push(id);
            continue;
        }
        let (key, item) = extra.next().expect("peeked");
        if item.album != 0 && placed.insert(item.album) { inserts.push((seen.len(), item.album)); }
        duration += item.duration_ms;
        if item.track != 0 {
            note(key, ids.len());
            ids.push(item.track);
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
    (Snapshot { ids, album_ids, totals }, letters)
}
