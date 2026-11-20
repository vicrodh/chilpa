//! Merge ordered SQL works with integrated source tags, without fetching a
//! catalog or retaining the library's complete inventory. Each matching source
//! work retains sort keys, a borrowed entry and its album ids for this query.
//! Live collection roots may exceed the source index's pruning target; only
//! 60 owned work cards cross the facade. No cache or worker is added.
use super::{Selection, PAGE};
use crate::{library_sources::{SourceIndex, SourceItem}, Reader, Snapshot, Totals};
use chilpa_library::store::{compositions::{Composition, work_key}, query::fold};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

fn key(item: &SourceItem) -> String {
    work_key(&item.work, &item.composers, json!(format!("{}:{}", item.plugin, if item.album != 0 { item.album } else { item.track })))
}

fn rank(title: &str, key: &str) -> (String, String) { (title.to_lowercase(), key.to_string()) }

struct Remote<'a> {
    first: &'a SourceItem,
    albums: HashSet<i64>,
    recordings: HashSet<(&'a str, i64, String)>,
    sources: BTreeSet<&'a str>,
    tracks: usize,
    duration_ms: u64,
}

fn source_labels(index: &SourceIndex) -> HashMap<&str, &str> {
    index.sources.iter().filter_map(|source| Some((source.key.split_once(':')?.1, source.label.as_str()))).collect()
}

fn source_label<'a>(labels: &HashMap<&str, &'a str>, plugin: &'a str) -> &'a str {
    labels.get(plugin).copied().filter(|label| !label.is_empty()).unwrap_or(plugin)
}

fn remote<'a>(index: &'a SourceIndex, composer: &str, text: &str) -> BTreeMap<(String, String), Remote<'a>> {
    let text = fold(text);
    let composer = composer.to_lowercase();
    let mut works = BTreeMap::<(String, String), Remote<'a>>::new();
    for item in index.items.iter().filter(|item| item.track != 0 && !item.work.trim().is_empty()) {
        if !composer.is_empty() && !item.composers.iter().any(|name| name.to_lowercase() == composer) { continue; }
        if !fold(&item.work).contains(&text) && !item.composers.iter().any(|name| fold(name).contains(&text)) { continue; }
        let work = works.entry(rank(item.work.trim_matches(' '), &key(item)))
            .or_insert_with(|| Remote { first: item, albums: HashSet::new(), recordings: HashSet::new(), sources: BTreeSet::new(), tracks: 0, duration_ms: 0 });
        if item.work < work.first.work { work.first = item; }
        if item.album != 0 { work.albums.insert(item.album); }
        let recording_album = if item.album != 0 { item.album } else { -item.track };
        work.recordings.insert((&item.plugin, recording_album, item.artists.first().map_or_else(String::new, |artist| artist.to_lowercase())));
        work.sources.insert(&item.plugin);
        work.tracks += 1;
        work.duration_ms += item.duration_ms;
    }
    works
}

fn owned(((_, key), work): ((String, String), Remote<'_>), labels: &HashMap<&str, &str>) -> Composition {
    Composition { key, title: work.first.work.trim_matches(' ').to_string(), composers: work.first.composers.clone(),
        albums: work.albums.len(), tracks: work.tracks, recordings: work.recordings.len(), duration_ms: work.duration_ms,
        example_track: work.first.track, sources: work.sources.into_iter().map(|plugin| source_label(labels, plugin).to_owned()).collect() }
}

impl Reader {
    pub(super) fn compositions_page(&self, composer: &str, text: &str, page: usize, descending: bool, index: &SourceIndex) -> Value {
        let (works, total) = match self.compositions_ordered_using(composer, text, page, index, descending) {
            Ok(page) => page,
            Err(error) => return json!({"error":error,"items":[],"total":0}),
        };
        let artwork = self.composition_artwork(works.iter().map(|work| work.example_track));
        let local = chilpa_i18n::t("Local");
        let items: Vec<_> = works.into_iter().map(|work| {
            let mut sources: Vec<_> = work.sources.into_iter().map(|name| if name.is_empty() { local.clone() } else { name }).collect();
            sources.sort();
            sources.dedup();
            json!({"kind":"work","name":work.title,"work":work.key,"composers":work.composers,
                "albums":work.albums,"tracks":work.tracks,"recordings":work.recordings,"duration":work.duration_ms,
                "sources":sources,"artwork":artwork.get(&work.example_track)})
        }).collect();
        json!({"total":total,"items":items})
    }

    fn composition_artwork(&self, ids: impl Iterator<Item = i64>) -> HashMap<i64, String> {
        let (local, remote): (Vec<_>, _) = ids.partition(|id| *id > 0);
        let mut covers: HashMap<_, _> = self.lib.tracks_by_ids(&local).unwrap_or_default().into_iter()
            .filter_map(|track| Some((track.id, track.artwork_path?))).collect();
        if let Some(catalog) = &self.catalog {
            covers.extend(catalog.tracks_by_ids(&remote).into_iter().filter(|(_, cover)| !cover.is_empty())
                .map(|(track, cover)| (track.id, cover)));
        }
        covers
    }

    #[cfg(test)]
    pub(super) fn discovery_compositions_using(&self, composer: &str, text: &str, page: usize, index: &SourceIndex) -> Result<(Vec<Composition>, usize), String> {
        self.compositions_ordered_using(composer, text, page, index, false)
    }

    fn compositions_ordered_using(&self, composer: &str, text: &str, page: usize, index: &SourceIndex, descending: bool) -> Result<(Vec<Composition>, usize), String> {
        let labels = source_labels(index);
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
            while extra.peek().is_some_and(|(remote, _)| if descending { *remote > at } else { *remote < at }) { take(owned(extra.next().unwrap(), &labels)); }
            if extra.peek().is_some_and(|(remote, _)| *remote == at) {
                let remote = extra.next().unwrap().1;
                local.albums += remote.albums.len();
                local.tracks += remote.tracks;
                local.recordings += remote.recordings.len();
                local.duration_ms += remote.duration_ms;
                local.sources.extend(remote.sources.into_iter().map(|plugin| source_label(&labels, plugin).to_owned()));
            }
            take(local);
        }).map_err(|error| error.to_string())?;
        for work in extra { take(owned(work, &labels)); }
        Ok((rows, total))
    }

    fn work_parts(&self, selected: &Selection) -> Result<Vec<Part>, String> {
        self.work_parts_using(selected, &self.source_index().1)
    }

    fn work_parts_using(&self, selected: &Selection, index: &SourceIndex) -> Result<Vec<Part>, String> {
        let local = chilpa_i18n::t("Local");
        let labels = source_labels(index);
        let mut parts: Vec<_> = self.lib.composition_parts(&selected.work).map_err(|error| error.to_string())?.into_iter().map(|row| Part {
            id: row.id, album: row.album, name: row.name, artist: row.artist, disc: row.disc, number: row.number,
            duration: row.duration_ms, source: "library".into(),
            source_name: if row.source.is_empty() { local.clone() } else { row.source },
            year: row.year,
        }).collect();
        for item in index.items.iter().filter(|item| item.track != 0 && !item.work.is_empty() && key(item) == selected.work) {
            parts.push(Part { id:item.track, album:item.album, name:if item.album_title.is_empty() { item.title.clone() } else { item.album_title.clone() },
                artist:item.artists.first().cloned().unwrap_or_default(), disc:item.disc.unwrap_or(0), number:item.number.unwrap_or(0),
                duration:item.duration_ms, source:format!("source:{}", item.plugin),
                source_name:source_label(&labels, &item.plugin).to_owned(), year:item.year });
        }
        parts.retain(|part| selected.recording.is_empty() || part.recording() == selected.recording);
        parts.sort_by_cached_key(|part| (part.name.to_ascii_lowercase(), part.album, part.artist.to_ascii_lowercase(), part.disc, part.number, part.id));
        Ok(parts)
    }

    pub(super) fn work_snapshot(&self, selected: &Selection) -> Snapshot {
        snapshot(self.work_parts(selected).unwrap_or_default())
    }

    pub(super) fn recordings_page_ordered(&self, selected: &Selection, text: &str, page: usize, descending: bool, index: &SourceIndex) -> Value {
        let parts = match self.work_parts_using(selected, index) {
            Ok(parts) => parts,
            Err(error) => return json!({"error":error,"items":[],"total":0}),
        };
        let (rows, total) = recordings_ordered(parts, text, page, descending);
        let artwork = self.composition_artwork(rows.iter().map(|(_, row)| row.first.id));
        let items: Vec<_> = rows.into_iter().map(|(key, row)| {
            let first = row.first;
            json!({"kind":"recording","recording":key,"name":first.name,"artist":first.artist,"album":first.album,
                "source":row.sources.into_iter().collect::<Vec<_>>().join(", "),"year":first.year,"tracks":row.tracks,
                "duration":row.duration,"artwork":artwork.get(&first.id)})
        }).collect();
        json!({"total":total,"items":items})
    }
}

struct Part {
    id:i64, album:i64, name:String, artist:String, disc:i64, number:i64, duration:u64, source:String, source_name:String, year:Option<i32>,
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

struct Recording {
    first: Part,
    sources: BTreeSet<String>,
    tracks: usize,
    duration: u64,
}

fn recordings_ordered(parts: Vec<Part>, text: &str, page: usize, descending: bool) -> (Vec<(String, Recording)>, usize) {
    use std::collections::btree_map::Entry;
    let mut found = BTreeMap::<String, Recording>::new();
    for mut part in parts {
        let source = std::mem::take(&mut part.source_name);
        match found.entry(part.recording()) {
            Entry::Occupied(mut entry) => {
                let row = entry.get_mut();
                row.tracks += 1;
                row.duration += part.duration;
                row.sources.insert(source);
            }
            Entry::Vacant(entry) => {
                entry.insert(Recording { duration:part.duration, first:part, sources:BTreeSet::from([source]), tracks:1 });
            }
        }
    }
    let mut rows: Vec<_> = found.into_iter().filter(|(_, row)| {
        if text.is_empty() { return true; }
        let first = &row.first;
        let year = first.year.map(|year| year.to_string()).unwrap_or_default();
        fold(&format!("{} {} {year}", first.name, first.artist)).contains(text)
            || row.sources.iter().any(|source| fold(source).contains(text))
    }).collect();
    // The BTreeMap already uses the recording identity to break title ties.
    rows.sort_by_cached_key(|(_, row)| row.first.name.to_lowercase());
    if descending { rows.reverse(); }
    let total = rows.len();
    (rows.into_iter().skip(page.saturating_mul(PAGE)).take(PAGE).collect(), total)
}

#[cfg(test)]
mod tests;
