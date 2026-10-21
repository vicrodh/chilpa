//! I read the query's track counts in bulk and discard them after grouping.

use super::{groups::{arrange, Filed}, AlbumGroup, SourceIndex};
use crate::albums_view::AlbumsFound;
use chilpa_library::Library;

pub(crate) fn file(count: i64) -> Filed {
    let (rank, label) = match count {
        10.. => (0, chilpa_i18n::t("Played 10 times or more")),
        1..=9 => (1, chilpa_i18n::t("Played")),
        _ => (2, chilpa_i18n::t("Never played")),
    };
    Filed { rank: (0, rank, String::new()), label }
}

pub(crate) fn label(count: i64) -> String { file(count).label }

// I keep at most one count per record or track, only for this request.
fn grouped(lib: &Library, ids: Vec<i64>, index: &SourceIndex, albums: bool) -> AlbumsFound {
    let mut counts = if albums { lib.album_play_counts() } else { lib.track_play_counts() }.unwrap_or_default();
    for item in &index.items {
        let id = if albums { item.album } else { item.track };
        if id == 0 { continue; }
        let count = item.plays.unwrap_or(0);
        if albums {
            let total = counts.entry(id).or_default();
            *total = total.saturating_add(count);
        } else { counts.insert(id, count); }
    }
    let filed = ids.into_iter().map(|id| (file(counts.get(&id).copied().unwrap_or(0)), id)).collect();
    arrange(filed, AlbumGroup::Plays, false)
}

/// I file each record by its tracks' total; the `plays:` filter matches per track.
pub(crate) fn albums(lib: &Library, ids: Vec<i64>, index: &SourceIndex) -> AlbumsFound {
    grouped(lib, ids, index, true)
}

pub(crate) fn tracks(lib: &Library, ids: Vec<i64>, index: &SourceIndex) -> AlbumsFound {
    grouped(lib, ids, index, false)
}

#[cfg(test)]
mod tests;
