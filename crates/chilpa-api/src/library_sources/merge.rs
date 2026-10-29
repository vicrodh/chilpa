//! The integrated items in a snapshot, in the order the database gave the
//! library's own tracks: both sides are ordered by the same keys and merged,
//! so nothing of the library is read again to place them.

use super::{Grouping, Prepared, SourceIndex, SourceItem, Stars};
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
        plays: item.plays.unwrap_or(0),
        group_year: item.year.filter(|y| *y > 0),
        genre: chilpa_library::first_name(&item.genres),
        label: item.label.trim_matches(' ').to_string(),
        composer: chilpa_library::first_name(&item.composers),
    }
}

/// I name snapshot sections and loaded rows with the same labels.
pub(crate) fn group_label(key: &OrderKey, by: &str) -> String {
    match by {
        "album" => if key.album_id == 0 { chilpa_i18n::t("No album tracks") } else { key.album.clone() },
        "artist" => key.artist.clone(),
        "year" | "decade" | "genre" | "label" | "composer" =>
            crate::tracks_view::metadata_group(by, key.group_year, &key.genre, &key.label, &key.composer),
        _ => chilpa_library::letter_of(&key.title),
    }
}

/// The library's snapshot with what the filter leaves of the index merged
/// in. With `letters_by`, also where each group starts among the tracks.
/// With `grouping`, a record the library and a source both keep counts once.
#[allow(clippy::too_many_arguments)]
pub(crate) fn snapshot(mut local: Snapshot, mut keys: Vec<OrderKey>, f: &Filter, order: TrackOrder, index: &SourceIndex, stars: &Stars, scope: Scope, letters_by: Option<&str>, grouping: Option<Grouping>)
    -> (Snapshot, Vec<(usize, String)>) {
    let filter = Prepared::new(f);
    let mut extra: Vec<(&OrderKey, &SourceItem)> = index.keys.iter().zip(&index.items)
        .filter(|(_, item)| scope.takes(item) && filter.passes(item, stars)).collect();
    if let Some(grouping) = grouping.filter(|_| !extra.is_empty()) {
        let mut albums = local.album_ids.clone();
        albums.extend(extra.iter().map(|(_, item)| item.album).filter(|id| *id != 0));
        let dropped = super::dropped_copies(&albums, grouping, &index.versions);
        if !dropped.is_empty() {
            extra.retain(|(_, item)| !dropped.contains(&item.album));
            let mut gone_ms = 0;
            let (mut ids, mut kept) = (Vec::with_capacity(local.ids.len()), Vec::with_capacity(keys.len()));
            for (id, key) in local.ids.into_iter().zip(keys) {
                if dropped.contains(&key.album_id) { gone_ms += key.duration_ms.max(0) as u64; } else { ids.push(id); kept.push(key); }
            }
            local.album_ids.retain(|id| !dropped.contains(id));
            local.totals = crate::Totals { tracks: ids.len(), albums: local.album_ids.len(), duration_ms: local.totals.duration_ms.saturating_sub(gone_ms) };
            local.ids = ids;
            keys = kept;
        }
    }
    let group = f.grouped_by.as_str();
    let mut sections: Vec<(usize, String)> = Vec::new();
    let mut previous: Option<&OrderKey> = None;
    let mut note = |key, at: usize| if let Some(by) = letters_by {
        if previous.is_none_or(|last| order.compare_group(group, last, key) != Ordering::Equal) {
            sections.push((at, group_label(key, by)));
        }
        previous = Some(key);
    };
    if extra.is_empty() {
        for (at, key) in keys.iter().enumerate() { note(key, at); }
        return (local, sections);
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
    (Snapshot { ids, album_ids, totals }, sections)
}
