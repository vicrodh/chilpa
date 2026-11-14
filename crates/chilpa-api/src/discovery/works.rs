//! Merge ordered SQL works with integrated source tags, without fetching a
//! catalog or retaining the library's complete inventory. At most 30,000 source
//! keys and borrowed entries live for this query, bounded by the source index;
//! only 60 owned work cards cross the facade. No cache or worker is added.
use super::{Selection, PAGE};
use crate::{library_sources::{SourceIndex, SourceItem}, Reader, Snapshot, Totals};
use chilpa_library::store::{compositions::{Composition, work_key}, query::fold};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

fn key(item: &SourceItem) -> String {
    work_key(&item.work, &item.composers, json!(format!("{}:{}", item.plugin, if item.album != 0 { item.album } else { item.track })))
}

fn rank(title: &str, key: &str) -> (String, String) { (title.to_lowercase(), key.to_string()) }

struct Remote<'a> { first: &'a SourceItem, albums: HashSet<i64>, tracks: usize }

fn remote<'a>(index: &'a SourceIndex, composer: &str, text: &str) -> BTreeMap<(String, String), Remote<'a>> {
    let text = fold(text);
    let mut works = BTreeMap::<(String, String), Remote<'a>>::new();
    for item in index.items.iter().filter(|item| item.track != 0 && !item.work.trim().is_empty()) {
        if !composer.is_empty() && !item.composers.iter().any(|name| name.to_lowercase() == composer.to_lowercase()) { continue; }
        if !fold(&item.work).contains(&text) && !item.composers.iter().any(|name| fold(name).contains(&text)) { continue; }
        let work = works.entry(rank(item.work.trim_matches(' '), &key(item)))
            .or_insert_with(|| Remote { first: item, albums: HashSet::new(), tracks: 0 });
        if item.work < work.first.work { work.first = item; }
        if item.album != 0 { work.albums.insert(item.album); }
        work.tracks += 1;
    }
    works
}

fn owned(((_, key), work): ((String, String), Remote<'_>)) -> Composition {
    Composition { key, title: work.first.work.trim_matches(' ').to_string(), composers: work.first.composers.clone(), albums: work.albums.len(), tracks: work.tracks }
}

impl Reader {
    pub(super) fn discovery_compositions_ordered(&self, composer: &str, text: &str, page: usize, descending: bool) -> Result<(Vec<Composition>, usize), String> {
        self.compositions_ordered_using(composer, text, page, &self.source_index().1, descending)
    }

    #[cfg(test)]
    pub(super) fn discovery_compositions_using(&self, composer: &str, text: &str, page: usize, index: &SourceIndex) -> Result<(Vec<Composition>, usize), String> {
        self.compositions_ordered_using(composer, text, page, index, false)
    }

    fn compositions_ordered_using(&self, composer: &str, text: &str, page: usize, index: &SourceIndex, descending: bool) -> Result<(Vec<Composition>, usize), String> {
        let extra = remote(index, composer, text).into_iter();
        let extra: Box<dyn Iterator<Item = _>> = if descending { Box::new(extra.rev()) } else { Box::new(extra) };
        let mut extra = extra.peekable();
        let (mut rows, mut total) = (Vec::with_capacity(PAGE), 0usize);
        let offset = page.saturating_mul(PAGE);
        let mut take = |work: Composition| {
            if total >= offset && rows.len() < PAGE { rows.push(work); }
            total += 1;
        };
        self.lib.visit_compositions_ordered(composer, text, descending, |mut local| {
            let at = rank(&local.title, &local.key);
            while extra.peek().is_some_and(|(remote, _)| if descending { *remote > at } else { *remote < at }) { take(owned(extra.next().unwrap())); }
            if extra.peek().is_some_and(|(remote, _)| *remote == at) {
                let remote = extra.next().unwrap().1;
                local.albums += remote.albums.len();
                local.tracks += remote.tracks;
            }
            take(local);
        }).map_err(|error| error.to_string())?;
        for work in extra { take(owned(work)); }
        Ok((rows, total))
    }

    fn work_parts(&self, selected: &Selection) -> Result<Vec<Part>, String> {
        self.work_parts_using(selected, &self.source_index().1)
    }

    fn work_parts_using(&self, selected: &Selection, index: &SourceIndex) -> Result<Vec<Part>, String> {
        let ids = self.lib.composition_tracks(&selected.work).map_err(|error| error.to_string())?;
        let mut parts: Vec<_> = self.lib.tracks_by_ids(&ids).map_err(|error| error.to_string())?.into_iter().map(|row| Part {
            id: row.id, album: row.album_id.unwrap_or(0), name: if row.album.is_empty() { row.title } else { row.album }, artist: row.artist,
            disc: row.disc_number.unwrap_or(0) as i64, number: row.track_number.unwrap_or(0) as i64,
            duration: row.duration_ms, source: "library".into(), source_name: String::new(),
        }).collect();
        for item in index.items.iter().filter(|item| item.track != 0 && !item.work.is_empty() && key(item) == selected.work) {
            parts.push(Part { id:item.track, album:item.album, name:if item.album_title.is_empty() { item.title.clone() } else { item.album_title.clone() },
                artist:item.artists.first().cloned().unwrap_or_default(), disc:item.disc.unwrap_or(0), number:item.number.unwrap_or(0),
                duration:item.duration_ms, source:format!("source:{}", item.plugin),
                source_name:index.sources.iter().find(|source| source.key == item.source_key()).map(|source| source.label.clone()).unwrap_or_default() });
        }
        parts.retain(|part| selected.recording.is_empty() || part.recording() == selected.recording);
        parts.sort_by_cached_key(|part| (part.name.to_ascii_lowercase(), part.album, part.artist.to_ascii_lowercase(), part.disc, part.number, part.id));
        Ok(parts)
    }

    pub(super) fn work_snapshot(&self, selected: &Selection) -> Snapshot {
        snapshot(self.work_parts(selected).unwrap_or_default())
    }

    pub(super) fn recordings_page_ordered(&self, selected: &Selection, text: &str, page: usize, descending: bool) -> Value {
        match self.work_parts(selected) {
            Ok(parts) => recordings_ordered(parts, text, page, descending),
            Err(error) => json!({"error":error,"items":[],"total":0}),
        }
    }
}

struct Part {
    id:i64, album:i64, name:String, artist:String, disc:i64, number:i64, duration:u64, source:String, source_name:String,
}
impl Part {
    fn recording(&self) -> String { json!([self.source, if self.album != 0 { self.album } else { -self.id }, self.artist.to_lowercase()]).to_string() }
}

fn snapshot(parts: Vec<Part>) -> Snapshot {
    let mut seen = HashSet::new();
    let album_ids: Vec<_> = parts.iter().map(|part| part.album).filter(|id| *id != 0 && seen.insert(*id)).collect();
    Snapshot { totals: Totals { albums:album_ids.len(), tracks:parts.len(), duration_ms:parts.iter().map(|part| part.duration).sum() },
        ids:parts.into_iter().map(|part| part.id).collect(), album_ids }
}

#[cfg(test)]
fn recordings(parts: Vec<Part>, text: &str, page: usize) -> Value {
    recordings_ordered(parts, text, page, false)
}

fn recordings_ordered(parts: Vec<Part>, text: &str, page: usize, descending: bool) -> Value {
    let text = text.to_lowercase();
    let mut found = BTreeMap::<String, Value>::new();
    for part in parts {
        if !format!("{} {} {}", part.name, part.artist, part.source_name).to_lowercase().contains(&text) { continue; }
        let key = part.recording();
        let row = found.entry(key.clone()).or_insert_with(|| json!({"kind":"recording", "recording":key, "name":part.name,
            "artist":part.artist,"album":part.album,"source":part.source_name,"tracks":0,"duration":0}));
        row["tracks"] = json!(row["tracks"].as_u64().unwrap_or(0) + 1);
        row["duration"] = json!(row["duration"].as_u64().unwrap_or(0) + part.duration);
    }
    let mut rows: Vec<_> = found.into_values().collect();
    rows.sort_by_cached_key(|row| (row["name"].as_str().unwrap_or_default().to_lowercase(), row["recording"].as_str().unwrap_or_default().to_string()));
    if descending { rows.reverse(); }
    let total = rows.len();
    json!({"total":total,"items":rows.into_iter().skip(page.saturating_mul(PAGE)).take(PAGE).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests;
