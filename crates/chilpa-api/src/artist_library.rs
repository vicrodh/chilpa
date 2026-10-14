//! I read artist collections off the window thread and retain recent visits.
use crate::{Album, Chilpa, QueryWorker, Reader, TrackRow};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CAPACITY: usize = 16;
const FRESH_FOR: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
pub(crate) struct Collection {
    pub records: Vec<Album>,
    pub appearances: Vec<Album>,
    pub popular: Vec<TrackRow>,
    pub labels: Vec<String>,
    pub art: String,
}
struct Entry {
    collection: Option<Collection>,
    ticket: u64,
    pending: bool,
    touched: Instant,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<String, Entry>,
    next: u64,
}
impl Cache {
    fn ask(&mut self, name: &str, now: Instant) -> (Option<Collection>, Option<u64>) {
        if let Some(entry) = self.entries.get_mut(name) {
            if entry.pending || now.duration_since(entry.touched) < FRESH_FOR {
                return (entry.collection.clone(), None);
            }
        }
        if !self.entries.contains_key(name) && self.entries.len() >= CAPACITY {
            if let Some(oldest) = self.entries.iter().min_by_key(|(_, entry)| entry.touched).map(|(name, _)| name.clone()) {
                self.entries.remove(&oldest);
            }
        }
        self.next = self.next.wrapping_add(1);
        let entry = self.entries.entry(name.into()).or_insert(Entry { collection: None, ticket: 0, pending: false, touched: now });
        entry.ticket = self.next;
        entry.pending = true;
        (entry.collection.clone(), Some(entry.ticket))
    }
    fn finish(&mut self, name: &str, ticket: u64, collection: Collection, now: Instant) -> bool {
        let Some(entry) = self.entries.get_mut(name).filter(|entry| entry.ticket == ticket) else { return false };
        entry.collection = Some(collection);
        entry.pending = false;
        entry.touched = now;
        true
    }
}
#[derive(Default)]
pub(crate) struct ArtistLibrary {
    cache: Arc<Mutex<Cache>>,
    worker: Option<QueryWorker>,
}

pub(crate) fn releases(lib: &chilpa_library::Library, name: &str, appearances: bool) -> Vec<Album> {
    let records = if appearances { lib.albums_appearing(name) }
        else { lib.albums_filtered(chilpa_library::AlbumSort::Year, Some(name), 10_000, 0) }.unwrap_or_default();
    let Some(catalog) = lib.group_artist_versions().then(|| lib.version_catalog().ok()).flatten() else {
        return records.into_iter().map(crate::browse::album_of).collect();
    };
    let ids = catalog.collapse(&records.iter().map(|(album, _)| album.id).collect::<Vec<_>>());
    let mut by_id: HashMap<_, _> = records.into_iter().map(crate::browse::album_of).map(|album| (album.id, album)).collect();
    let missing: Vec<_> = ids.iter().copied().filter(|id| !by_id.contains_key(id)).collect();
    for chunk in missing.chunks(500) {
        by_id.extend(lib.albums_by_ids(chunk).unwrap_or_default().into_iter().map(crate::browse::album_of).map(|album| (album.id, album)));
    }
    ids.into_iter().filter_map(|id| by_id.remove(&id)).collect()
}
impl Reader {
    fn artist_collection(&self, name: &str) -> Collection {
        let records = releases(&self.lib, name, false);
        let appearances = releases(&self.lib, name, true);
        let ids = records.iter().map(|album| album.id).collect::<Vec<_>>();
        let labels = self.lib.albums_labels(&ids).unwrap_or_default();
        let art = self.lib.artist_artwork(name).ok().flatten().filter(|art| !art.is_empty())
            .or_else(|| records.first().or(appearances.first()).map(|album| album.artwork_path.clone())).unwrap_or_default();
        let filter = crate::Filter { performer: Some(name.into()), ..Default::default() };
        let mut popular: Vec<_> = self.lib.browse_tracks(&filter, crate::TrackOrder::Plays, 10).unwrap_or_default().into_iter().map(crate::browse::row_of).collect();
        popular.retain(|track| track.plays > 0);
        if popular.is_empty() { popular = self.lib.browse_tracks(&filter, crate::TrackOrder::Artist, 10).unwrap_or_default().into_iter().map(crate::browse::row_of).collect(); }
        Collection { records, appearances, labels, art, popular }
    }
}
impl Chilpa {
    pub fn invalidate_artist_collections(&mut self) { self.artist_library.cache.lock().unwrap().entries.clear(); }

    pub(crate) fn artist_collection(&mut self, name: &str) -> Option<Collection> {
        let name = name.trim();
        if name.is_empty() { return None; }
        let (cached, ticket) = self.artist_library.cache.lock().unwrap().ask(name, Instant::now());
        if let Some(ticket) = ticket {
            if self.artist_library.worker.is_none() { self.artist_library.worker = self.query_worker(); }
            let (cache, catalog, requested_name) = (self.artist_library.cache.clone(), self.catalog.clone(), name.to_string());
            let submitted = self.artist_library.worker.as_ref().is_some_and(|worker| worker.submit(move |reader| {
                let collection = reader.artist_collection(&requested_name);
                if cache.lock().unwrap().finish(&requested_name, ticket, collection, Instant::now()) { catalog.changed(); }
            }));
            if !submitted {
                self.artist_library.worker = None;
                self.artist_library.cache.lock().unwrap().entries.remove(name);
            }
        }
        cached
    }
    pub fn cached_artist_releases(&mut self, name: &str, appearances: bool) -> Vec<Album> {
        self.artist_collection(name).map(|collection| if appearances { collection.appearances } else { collection.records }).unwrap_or_default()
    }
    pub fn cached_artist_top_tracks(&mut self, name: &str) -> Vec<TrackRow> {
        self.artist_collection(name).map(|collection| collection.popular).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_queries_are_shared_and_old_tickets_cannot_replace_a_new_visit() {
        let mut cache = Cache::default();
        let now = Instant::now();
        let (_, ticket) = cache.ask("Band", now);
        assert!(cache.ask("Band", now).1.is_none());
        cache.entries.clear();
        let (_, current) = cache.ask("Band", now);
        assert!(!cache.finish("Band", ticket.unwrap(), Collection::default(), now));
        assert!(cache.finish("Band", current.unwrap(), Collection::default(), now));
        assert!(cache.ask("Band", now).0.is_some());
        assert!(cache.ask("Band", now + FRESH_FOR).1.is_some());
    }
    #[test]
    fn recent_artist_collections_have_a_fixed_capacity() {
        let mut cache = Cache::default();
        let now = Instant::now();
        for index in 0..CAPACITY + 5 { cache.ask(&index.to_string(), now + Duration::from_millis(index as u64)); }
        assert_eq!(cache.entries.len(), CAPACITY);
        assert!(!cache.entries.contains_key("0"));
    }
}
