//! Merge ordered SQL works with integrated source tags, without fetching a
//! catalog or retaining the library's complete inventory. Each matching source
//! work retains sort keys, a borrowed entry and its album ids for this query.
//! Live collection roots may exceed the source index's pruning target; only
//! 60 owned work cards cross the facade. No cache or worker is added.
use super::{classical::{self, Classical}, Selection, PAGE};
use crate::{library_sources::{SourceIndex, SourceItem}, Reader, Snapshot, Totals};
use chilpa_library::store::{compositions::{Composition, work_key}, query::fold};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The work a source track names: its tag, or the work its title prints
/// before a part marker when it has a composer.
fn work_of(item: &SourceItem) -> Option<String> {
    let tagged = item.work.trim_matches(' ');
    if !tagged.trim().is_empty() { return Some(tagged.to_string()); }
    chilpa_library::work_titles::work_from_title(&item.title, &item.composers)
}

fn key_of(item: &SourceItem, work: &str) -> String {
    work_key(work, &item.composers, json!(format!("{}:{}", item.plugin, if item.album != 0 { item.album } else { item.track })))
}

#[cfg(test)]
fn key(item: &SourceItem) -> String { key_of(item, &work_of(item).unwrap_or_default()) }

fn rank(title: &str, key: &str) -> (String, String) { (title.to_lowercase(), key.to_string()) }

struct Remote<'a> {
    first: &'a SourceItem,
    title: String,
    albums: HashSet<i64>,
    recordings: HashSet<(&'a str, i64, String)>,
    sources: BTreeSet<&'a str>,
    tracks: usize,
    duration_ms: u64,
    classical_genre: bool,
}

fn source_labels(index: &SourceIndex) -> HashMap<&str, &str> {
    index.sources.iter().filter_map(|source| Some((source.key.split_once(':')?.1, source.label.as_str()))).collect()
}

fn source_label<'a>(labels: &HashMap<&str, &'a str>, plugin: &'a str) -> &'a str {
    labels.get(plugin).copied().filter(|label| !label.is_empty()).unwrap_or(plugin)
}

fn remote<'a>(index: &'a SourceIndex, composer: &str, text: &str, classical: &Classical) -> BTreeMap<(String, String), Remote<'a>> {
    let text = fold(text);
    let composer = composer.to_lowercase();
    let mut works = BTreeMap::<(String, String), Remote<'a>>::new();
    for item in index.items.iter().filter(|item| item.track != 0) {
        if !composer.is_empty() && !item.composers.iter().any(|name| name.to_lowercase() == composer) { continue; }
        let Some(title) = work_of(item) else { continue };
        if !fold(&title).contains(&text) && !item.composers.iter().any(|name| fold(name).contains(&text)) { continue; }
        let at = rank(&title, &key_of(item, &title));
        let work = works.entry(at).or_insert_with(|| Remote { first: item, title: title.clone(), albums: HashSet::new(), recordings: HashSet::new(),
            sources: BTreeSet::new(), tracks: 0, duration_ms: 0, classical_genre: false });
        if title < work.title { work.first = item; work.title = title; }
        if item.album != 0 { work.albums.insert(item.album); }
        let recording_album = if item.album != 0 { item.album } else { -item.track };
        work.recordings.insert((&item.plugin, recording_album, item.artists.first().map_or_else(String::new, |artist| artist.to_lowercase())));
        work.sources.insert(&item.plugin);
        work.tracks += 1;
        work.duration_ms += item.duration_ms;
        work.classical_genre |= classical.genre(&item.genres);
    }
    works
}

fn owned(((_, key), work): ((String, String), Remote<'_>), labels: &HashMap<&str, &str>) -> Composition {
    Composition { key, title: work.title, composers: work.first.composers.clone(),
        albums: work.albums.len(), tracks: work.tracks, recordings: work.recordings.len(), duration_ms: work.duration_ms,
        example_track: work.first.track, sources: work.sources.into_iter().map(|plugin| source_label(labels, plugin).to_owned()).collect(),
        classical_genre: work.classical_genre }
}

/// What a page of works reports beside its rows: whether any work of the
/// whole result has a date or an instrumentation, so empty columns stay hidden.
#[derive(Default)]
struct Columns { date: bool, instrumentation: bool }

impl Reader {
    /// The classical genres of the library's tags and sources, and who
    /// composes in them.
    pub(super) fn classical_using(&self, index: &SourceIndex) -> (Vec<String>, Classical) {
        let tags: Vec<String> = self.discovery_names_using(false, &[], index).into_iter().map(|(name, _)| name).collect();
        let genres = super::genres::Inventory::new(&tags).matching("Classical");
        let mut found = Classical::new(&genres, self.lib.composers_in_genres(&genres).unwrap_or_default());
        let credited: Vec<&[String]> = index.items.iter().filter(|item| found.genre(&item.genres)).map(|item| item.composers.as_slice()).collect();
        for names in credited { found.add_composers(names); }
        (genres, found)
    }

    pub(super) fn compositions_page(&self, composer: &str, text: &str, page: usize, descending: bool, only_classical: bool, index: &SourceIndex) -> Value {
        let (genres, classical) = self.classical_using(index);
        if page == 0 { self.read_ahead_classical_records(index, &classical); }
        let mut columns = Columns::default();
        let (works, total) = match self.compositions_ordered_using(composer, text, page, index, descending, &genres, &classical, only_classical, &mut columns) {
            Ok(page) => page,
            Err(error) => return json!({"error":error,"items":[],"total":0}),
        };
        let artwork = self.composition_artwork(works.iter().map(|(work, _)| work.example_track));
        let local = chilpa_i18n::t("Local");
        let items: Vec<_> = works.into_iter().map(|(work, is_classical)| {
            let mut sources: Vec<_> = work.sources.into_iter().map(|name| if name.is_empty() { local.clone() } else { name }).collect();
            sources.sort();
            sources.dedup();
            let facts = classical::facts(&work.title, &work.composers);
            json!({"kind":"work","name":work.title,"work":work.key,"composers":work.composers,
                "albums":work.albums,"tracks":work.tracks,"recordings":work.recordings,"duration":work.duration_ms,
                "sources":sources,"artwork":artwork.get(&work.example_track),"classical":is_classical,
                "form":facts.form,"period":facts.period,"date":facts.date,"instrumentation":facts.instrumentation})
        }).collect();
        let about = (!composer.is_empty()).then(|| classical::composer(composer)).flatten().map(|known| json!({
            "name":known.name,"years":known.years(),"period":classical::period_label(known.period)}));
        json!({"total":total,"items":items,"classical":only_classical,"composer":about,
            "columns":{"date":columns.date,"instrumentation":columns.instrumentation}})
    }

    /// Saved source records of a classical genre whose tracks were never
    /// read carry no works yet: their tracks are read ahead while a work
    /// page is shown, 16 records per request.
    fn read_ahead_classical_records(&self, index: &SourceIndex, classical: &Classical) {
        let Some(catalog) = &self.catalog else { return };
        let records: Vec<i64> = index.items.iter().filter(|item| item.track == 0 && item.album < 0 && classical.genre(&item.genres))
            .map(|item| item.album).take(16).collect();
        if !records.is_empty() { catalog.read_ahead_tracks(&records); }
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
        let (rows, total) = self.compositions_ordered_using(composer, text, page, index, false, &[], &Classical::default(), false, &mut Columns::default())?;
        Ok((rows.into_iter().map(|(work, _)| work).collect(), total))
    }

    #[allow(clippy::too_many_arguments)]
    fn compositions_ordered_using(&self, composer: &str, text: &str, page: usize, index: &SourceIndex, descending: bool,
        genres: &[String], classical: &Classical, only_classical: bool, columns: &mut Columns) -> Result<(Vec<(Composition, bool)>, usize), String> {
        let labels = source_labels(index);
        let extra = remote(index, composer, text, classical).into_iter();
        let extra: Box<dyn Iterator<Item = _>> = if descending { Box::new(extra.rev()) } else { Box::new(extra) };
        let mut extra = extra.peekable();
        let (mut rows, mut total) = (Vec::with_capacity(PAGE), 0usize);
        let offset = page.saturating_mul(PAGE);
        let mut take = |work: Composition| {
            let is_classical = classical.work(&work.composers, work.classical_genre);
            if only_classical && !is_classical { return; }
            let facts = classical::facts(&work.title, &work.composers);
            columns.date |= facts.date.is_some();
            columns.instrumentation |= facts.instrumentation.is_some();
            if total >= offset && rows.len() < PAGE { rows.push((work, is_classical)); }
            total += 1;
        };
        self.lib.visit_compositions_ordered(composer, text, descending, genres, |mut local| {
            let at = rank(&local.title, &local.key);
            while extra.peek().is_some_and(|(remote, _)| if descending { *remote > at } else { *remote < at }) { take(owned(extra.next().unwrap(), &labels)); }
            if extra.peek().is_some_and(|(remote, _)| *remote == at) {
                let remote = extra.next().unwrap().1;
                local.albums += remote.albums.len();
                local.tracks += remote.tracks;
                local.recordings += remote.recordings.len();
                local.duration_ms += remote.duration_ms;
                local.classical_genre |= remote.classical_genre;
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
        for item in index.items.iter().filter(|item| item.track != 0 && work_of(item).is_some_and(|work| key_of(item, &work) == selected.work)) {
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
