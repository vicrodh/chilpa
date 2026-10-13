//! I keep small Home sections in memory and refresh them on a separate reader.
use crate::{Album, Chilpa, QueryWorker, Reader, TrackRow};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FRESH_FOR: Duration = Duration::from_secs(5);
const SECTIONS: usize = 24;
const ROWS: usize = 128;

#[derive(Clone, Debug)]
pub enum HomeRows {
    Albums(Vec<Album>),
    Tracks(Vec<TrackRow>),
    Json(String),
}
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key(String, usize);
struct Entry {
    rows: Option<HomeRows>,
    stamp: (i32, i32),
    fetched: Instant,
    ticket: u64,
    pending: bool,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    next: u64,
}
impl Cache {
    fn ask(&mut self, key: &Key, stamp: (i32, i32), now: Instant) -> (Option<HomeRows>, Option<u64>) {
        if let Some(entry) = self.entries.get(key) {
            if !entry.pending && entry.rows.is_some() && entry.stamp == stamp && now.duration_since(entry.fetched) < FRESH_FOR {
                return (entry.rows.clone(), None);
            }
        }
        if !self.entries.contains_key(key) && self.entries.len() >= SECTIONS {
            let oldest = self.entries.iter().min_by_key(|(_, entry)| entry.fetched).map(|(key, _)| key.clone()).unwrap();
            self.entries.remove(&oldest);
        }
        self.next = self.next.wrapping_add(1);
        let entry = self.entries.entry(key.clone()).or_insert(Entry { rows: None, stamp, fetched: now, ticket: 0, pending: false });
        entry.ticket = self.next;
        entry.stamp = stamp;
        entry.pending = true;
        (entry.rows.clone(), Some(entry.ticket))
    }
    fn finish(&mut self, key: &Key, ticket: u64, rows: HomeRows, now: Instant) {
        if let Some(entry) = self.entries.get_mut(key).filter(|entry| entry.ticket == ticket) {
            entry.rows = Some(rows);
            entry.fetched = now;
            entry.pending = false;
        }
    }
}
#[derive(Default)]
pub(crate) struct Home {
    cache: Arc<Mutex<Cache>>,
    worker: Option<QueryWorker>,
    revision: i32,
}
impl Home {
    pub(crate) fn changed(&mut self) { self.revision = self.revision.wrapping_add(1); }
    pub(crate) fn clear(&mut self) { self.cache.lock().unwrap().entries.clear(); }
}
impl Chilpa {
    pub fn listening_revision(&self) -> i32 { self.home.revision }

    /// I reuse small JSON sections between visits and invalidate them on an explicit revision.
    pub fn cached_home_json(&mut self, kind: &str, limit: usize, revision: i32, load: impl FnOnce(&mut Self) -> String) -> String {
        let key = Key(format!("json:{kind}"), limit.clamp(1, ROWS));
        let (cached, ticket) = self.home.cache.lock().unwrap().ask(&key, (revision, self.home.revision), Instant::now());
        if ticket.is_none() {
            if let Some(HomeRows::Json(rows)) = cached { return rows; }
        }
        let rows = load(self);
        self.home.cache.lock().unwrap().finish(&key, ticket.unwrap(), HomeRows::Json(rows.clone()), Instant::now());
        rows
    }

    /// I return cached rows immediately, then deliver a refreshed section off the UI thread.
    /// A result has at most 128 rows; I retain at most 24 sections and no image pixels.
    pub fn home_section(&mut self, kind: &str, limit: usize, revision: i32,
        arrived: impl FnOnce(HomeRows) + Send + 'static) -> Option<HomeRows> {
        if !matches!(kind, "recentlyPlayed" | "recentlyPlayedEverywhere" | "recentlyAdded" | "mostPlayed") { return None; }
        let key = Key(kind.into(), limit.clamp(1, ROWS));
        let recent = kind.starts_with("recentlyPlayed");
        let current = if recent && self.in_progress.is_some() && self.now.album_id != 0
            && (kind == "recentlyPlayedEverywhere" || self.now.album_id > 0) {
            self.album(self.now.album_id)
        } else { None };
        let stamp = (revision, if kind == "recentlyAdded" { 0 } else { self.home.revision });
        let (cached, ticket) = self.home.cache.lock().unwrap().ask(&key, stamp, Instant::now());
        if let Some(ticket) = ticket {
            if self.home.worker.is_none() { self.home.worker = self.query_worker(); }
            if let Some(worker) = &self.home.worker {
                let (cache, current) = (self.home.cache.clone(), current.clone());
                let _ = worker.submit(move |reader| {
                    let rows = reader.home_section(&key.0, key.1);
                    cache.lock().unwrap().finish(&key, ticket, rows.clone(), Instant::now());
                    arrived(with_current(rows, current, key.1));
                });
            }
        }
        cached.or_else(|| current.as_ref().map(|_| HomeRows::Albums(Vec::new())))
            .map(|rows| with_current(rows, current, limit.clamp(1, ROWS)))
    }
}
fn with_current(mut rows: HomeRows, current: Option<Album>, limit: usize) -> HomeRows {
    if let (HomeRows::Albums(albums), Some(current)) = (&mut rows, current) {
        albums.retain(|album| album.id != current.id);
        albums.insert(0, current);
        albums.truncate(limit);
    }
    rows
}
impl Reader {
    fn home_section(&self, kind: &str, limit: usize) -> HomeRows {
        if kind == "mostPlayed" {
            return HomeRows::Tracks(self.lib.browse_tracks(&crate::Filter::default(), crate::TrackOrder::Plays, limit)
                .unwrap_or_default().into_iter().filter(|track| track.plays > 0).map(crate::browse::row_of).collect());
        }
        if kind == "recentlyPlayedEverywhere" {
            let releases = self.lib.recent_releases(limit.saturating_mul(2)).unwrap_or_default();
            let ids: Vec<_> = releases.iter().filter_map(|release| release.album_id).collect();
            let local: HashMap<_, _> = self.lib.albums_by_ids(&ids).unwrap_or_default().into_iter()
                .map(crate::browse::album_of).map(|album| (album.id, album)).collect();
            return HomeRows::Albums(releases.iter().filter_map(|release| match release.album_id {
                Some(id) => local.get(&id).cloned(),
                None if release.provider == crate::source_opener::PROVIDER => self.catalog.as_ref()?
                    .card_by_identity(&release.instance, &release.album_external_id).map(|card| crate::source_catalog::view::album_of(&card)),
                None => None,
            }).take(limit).collect());
        }
        let order = if kind == "recentlyAdded" { crate::AlbumSort::Added } else { crate::AlbumSort::LastPlayed };
        let candidates = self.lib.albums_sorted(order, limit.saturating_mul(2), 0).unwrap_or_default();
        let ids: Vec<_> = candidates.iter().filter(|(_, plays)| kind == "recentlyAdded" || *plays > 0).map(|(album, _)| album.id).collect();
        let mut ids = self.lib.grouped_album_ids(&ids).unwrap_or(ids);
        ids.truncate(limit);
        HomeRows::Albums(self.lib.albums_by_ids(&ids).unwrap_or_default().into_iter().map(crate::browse::album_of).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn empty() -> HomeRows { HomeRows::Albums(Vec::new()) }
    #[test]
    fn warm_sections_are_immediate_and_expired_sections_remain_visible_during_refresh() {
        let (mut cache, key, now) = (Cache::default(), Key("recentlyPlayed".into(), 18), Instant::now());
        let (rows, ticket) = cache.ask(&key, (0, 0), now);
        assert!(rows.is_none());
        cache.finish(&key, ticket.unwrap(), empty(), now);
        let (rows, ticket) = cache.ask(&key, (0, 0), now + Duration::from_secs(1));
        assert!(rows.is_some() && ticket.is_none());
        let (rows, ticket) = cache.ask(&key, (0, 0), now + FRESH_FOR);
        assert!(rows.is_some() && ticket.is_some());
    }
    #[test]
    fn a_listen_invalidates_freshness_and_an_older_query_cannot_overwrite_its_result() {
        let (mut cache, key, now) = (Cache::default(), Key("recentlyPlayed".into(), 18), Instant::now());
        let old = cache.ask(&key, (0, 0), now).1.unwrap();
        let new = cache.ask(&key, (0, 1), now).1.unwrap();
        cache.finish(&key, old, empty(), now);
        assert!(cache.entries[&key].rows.is_none());
        cache.finish(&key, new, empty(), now);
        assert!(cache.ask(&key, (0, 1), now).1.is_none());
        assert!(cache.ask(&key, (0, 2), now).1.is_some());
    }
    #[test]
    fn section_count_stays_bounded() {
        let (mut cache, now) = (Cache::default(), Instant::now());
        for limit in 1..100 { cache.ask(&Key("recentlyAdded".into(), limit), (0, 0), now); }
        assert_eq!(cache.entries.len(), SECTIONS);
    }
    #[test]
    fn the_current_album_moves_to_the_front_without_duplicate_entries() {
        let album = |id| crate::browse::album_of((chilpa_library::StoredAlbum {
            id, title: format!("Album {id}"), album_artist: "Artist".into(), year: None, artwork_path: None, track_count: 1,
        }, 1));
        let HomeRows::Albums(rows) = with_current(HomeRows::Albums(vec![album(1), album(2), album(3)]), Some(album(2)), 2) else { panic!() };
        assert_eq!(rows.iter().map(|album| album.id).collect::<Vec<_>>(), [2, 1]);
    }
}
