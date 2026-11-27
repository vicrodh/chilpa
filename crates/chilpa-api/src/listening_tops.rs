//! The artists, records and tracks a period was mostly made of, as the
//! stats report and the recap both name them: each with its plays, the time
//! it was listened to and a cover, from the library or from the source the
//! listen came from.

use chilpa_library::store::listening::{ListenBucket, ListenDimension, ListenFilter, ListenOrder};
use chilpa_library::Library;
use serde_json::{json, Value};

use crate::source_catalog::Catalog;

/// I read the source id and a length-checked reference, including its colons.
fn source_key<'a>(key: &'a str, prefix: &str) -> Option<(i64, &'a str)> {
    let (source, rest) = key.strip_prefix(prefix)?.split_once(':')?;
    let (length, reference) = rest.split_once(':')?;
    (reference.chars().count() == length.parse::<usize>().ok()?)
        .then_some((source.parse().ok()?, reference))
}

fn top(lib: &Library, filter: &ListenFilter, dimension: ListenDimension, limit: usize) -> impl Iterator<Item = ListenBucket> {
    lib.listening_buckets(filter, dimension, ListenOrder::Plays, limit).unwrap_or_default().into_iter().filter(|b| b.key.is_some())
}

pub(crate) fn artists(lib: &Library, filter: &ListenFilter, limit: usize) -> Vec<Value> {
    artist_rows(lib, top(lib, filter, ListenDimension::Artist, limit))
}

/// Each artist of a ranking with its plays, its time and its picture.
pub(crate) fn artist_rows(lib: &Library, buckets: impl IntoIterator<Item = ListenBucket>) -> Vec<Value> {
    let pictures = lib.artist_pictures().unwrap_or_default();
    buckets.into_iter()
        .map(|b| {
            let art = pictures.of(&b.label).map(str::to_string).or_else(|| lib.artist_cover(&b.label).ok().flatten()).unwrap_or_default();
            json!({"name": b.label, "plays": b.metrics.plays, "listened_ms": b.metrics.listened_ms, "art": art})
        })
        .collect()
}

pub(crate) fn albums(lib: &Library, catalog: &Catalog, filter: &ListenFilter, limit: usize) -> Vec<Value> {
    album_rows(lib, catalog, top(lib, filter, ListenDimension::Album, limit))
}

/// Each record of a ranking with its plays, its time, its cover and where
/// it opens: the library's record, or the source's card.
pub(crate) fn album_rows(lib: &Library, catalog: &Catalog, buckets: impl IntoIterator<Item = ListenBucket>) -> Vec<Value> {
    buckets.into_iter()
        .map(|b| {
            let id = b.key.as_deref().and_then(|k| k.strip_prefix("album:")).and_then(|id| id.parse::<i64>().ok());
            let album = id.and_then(|id| lib.album(id).ok().flatten()).map(|(a, _)| a);
            let source = b.key.as_deref().and_then(|key| source_key(key, "source:"))
                .and_then(|(source_id, reference)| {
                    let (provider, plugin) = lib.listening_source(source_id).ok().flatten()?;
                    if provider != crate::source_opener::PROVIDER { return None; }
                    catalog.card_by_identity(&plugin, reference).and_then(|card| catalog.card(card.id))
                });
            let art = album.as_ref().and_then(|a| a.artwork_path.clone()).filter(|art| !art.is_empty())
                .or_else(|| source.as_ref().map(|c| c.art.clone()).filter(|art| !art.is_empty()))
                .or_else(|| b.key.as_deref().and_then(|key| source_key(key, "source:"))
                    .and_then(|(source_id, reference)| lib.listening_source_art(source_id, reference, true).ok().flatten()))
                .unwrap_or_default();
            json!({
                "title": album.as_ref().map(|a| a.title.clone()).or_else(|| source.as_ref().map(|c| c.title.clone())).unwrap_or(b.label.clone()),
                "artist": album.as_ref().map(|a| a.album_artist.clone()).or_else(|| source.as_ref().map(|c| c.artist.clone())).unwrap_or_default(),
                "art": art,
                "id": id.or_else(|| source.as_ref().map(|c| c.id)).unwrap_or(0),
                "plays": b.metrics.plays,
                "listened_ms": b.metrics.listened_ms,
            })
        })
        .collect()
}

pub(crate) fn tracks(lib: &Library, catalog: &Catalog, filter: &ListenFilter, limit: usize) -> Vec<Value> {
    top(lib, filter, ListenDimension::Track, limit)
        .map(|b| {
            let id = b.key.as_deref().and_then(|k| k.strip_prefix("track:")).and_then(|id| id.parse::<i64>().ok());
            let track = id.and_then(|id| lib.tracks_by_ids(&[id]).ok()).and_then(|mut rows| rows.pop());
            let source = b.key.as_deref().and_then(|key| source_key(key, ""))
                .and_then(|(source_id, reference)| {
                    let (provider, plugin) = lib.listening_source(source_id).ok().flatten()?;
                    (provider == crate::source_opener::PROVIDER).then(|| {
                        let path = crate::source_opener::entry_of(&plugin, reference);
                        let track = catalog.track_by_path(&path);
                        if track.as_ref().is_none_or(|(_, art)| art.is_empty()) { catalog.want_art_of(&path); }
                        track
                    }).flatten()
                });
            let art = track.as_ref().and_then(|t| t.artwork_path.clone()).filter(|art| !art.is_empty())
                .or_else(|| source.as_ref().map(|(_, art)| art.clone()).filter(|art| !art.is_empty()))
                .or_else(|| b.key.as_deref().and_then(|key| source_key(key, ""))
                    .and_then(|(source_id, reference)| lib.listening_source_art(source_id, reference, false).ok().flatten()))
                .unwrap_or_default();
            json!({
                "title": b.label,
                "artist": track.as_ref().map(|t| t.artist.clone()).or_else(|| source.as_ref().map(|(t, _)| t.artist.clone())).unwrap_or_default(),
                "art": art,
                "plays": b.metrics.plays,
                "listened_ms": b.metrics.listened_ms,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_keys_keep_the_plugin_reference_intact() {
        assert_eq!(source_key("source:3:10:album:mx:ñ", "source:"), Some((3, "album:mx:ñ")));
        assert_eq!(source_key("3:8:track:42", ""), Some((3, "track:42")));
        assert_eq!(source_key("source:3:9:album:mx:ñ", "source:"), None);
        assert_eq!(source_key("track:12", ""), None);
    }
}
