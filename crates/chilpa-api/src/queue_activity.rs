//! I retain queue activity until path resolution and insertion both finish.
use rand::seq::{IndexedRandom, SliceRandom};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

pub struct QueueWork(Arc<AtomicUsize>);
pub struct AlbumQueueWork {
    pub ids: Vec<i64>,
    action: String,
    shuffled: bool,
    label: String,
    origin: Option<crate::queue_origin::QueueOrigin>,
    _work: QueueWork,
}
pub struct CollectionQueueAction {
    action: String,
    shuffled: bool,
    limit: Option<usize>,
    label: String,
}

impl CollectionQueueAction {
    pub fn new(action: String, shuffled: bool, limit: i32, label: String) -> Self {
        Self { action, shuffled, limit: usize::try_from(limit).ok().filter(|n| *n > 0), label }
    }

    pub fn album_selection(&self, mut ids: Vec<i64>) -> Vec<i64> {
        if self.action == "random-album" {
            let chosen = ids.choose(&mut rand::rng()).copied();
            ids.clear();
            ids.extend(chosen);
        }
        ids
    }

    /// Selection happens after resolving availability. A limited shuffle
    /// draws from the entire collection, in one pass over its paths.
    pub fn select(&self, paths: impl Iterator<Item = String>) -> Vec<String> {
        use rand::RngExt as _;
        let Some(limit) = self.limit else { return paths.collect(); };
        if !self.shuffled { return paths.take(limit).collect(); }
        let mut rng = rand::rng();
        let mut sample = Vec::new();
        for (at, path) in paths.enumerate() {
            if at < limit {
                sample.push(path);
            } else {
                let slot = rng.random_range(0..=at);
                if slot < limit { sample[slot] = path; }
            }
        }
        sample.shuffle(&mut rng);
        sample
    }

    pub fn apply(&self, api: &mut crate::Chilpa, paths: &[String]) {
        if paths.is_empty() { return; }
        match self.action.as_str() {
            "play" => api.play_list(paths, self.shuffled),
            "random-album" => api.play_list(paths, false),
            "queue" => api.enqueue_here(paths),
            "next" => api.play_next_here(paths),
            "later" => { api.play_later_here(paths, &self.label); }
            act if act.starts_with("buffer:") => api.listen_paths_action(act, paths),
            _ => {}
        }
    }
}

impl crate::Reader {
    pub fn collection_track_paths(&self, ids: &[i64], action: &CollectionQueueAction) -> Vec<String> {
        action.select(ids.chunks(500).flat_map(|chunk| {
            let mut paths = self.lib.playable_track_paths(chunk).unwrap_or_default();
            if let Some(catalog) = &self.catalog { paths.extend(catalog.playable_track_paths(chunk)); }
            chunk.iter().filter_map(move |id| paths.get(id).cloned())
        }))
    }

    pub fn collection_album_paths(&self, ids: &[i64], mut remote: crate::source_catalog::AlbumPaths, action: &CollectionQueueAction) -> Vec<String> {
        action.select(ids.chunks(500).flat_map(|chunk| {
            let mut local = self.lib.playable_album_paths(chunk).unwrap_or_default();
            chunk.iter().map(|id| {
                if *id > 0 { local.remove(id) } else { remote.remove(id) }
            }).collect::<Vec<_>>().into_iter().flatten().flatten()
        }))
    }
}

impl Drop for QueueWork {
    fn drop(&mut self) { self.0.fetch_sub(1, Ordering::Relaxed); }
}
impl crate::Chilpa {
    pub fn read_collection_album_paths(&self, ids: Vec<i64>, ready: impl FnOnce(crate::source_catalog::AlbumPaths) + Send + 'static) {
        self.catalog.with_album_paths(ids, Box::new(ready));
    }

    pub fn begin_queue_work(&self) -> QueueWork {
        self.queue_work.fetch_add(1, Ordering::Relaxed);
        QueueWork(self.queue_work.clone())
    }
    pub fn queue_busy(&self) -> bool {
        self.queue_work.load(Ordering::Relaxed) > 0 || self.catalog.actions_pending()
    }
    pub fn prepare_album_queue(&mut self, ids: Vec<i64>, action: String, shuffled: bool, label: String) -> Option<AlbumQueueWork> {
        if ids.is_empty() { return None; }
        if ids.iter().any(|id| *id < 0) {
            match action.as_str() {
                "later" => self.album_later(&ids, &label),
                act if act.starts_with("buffer:") => self.album_listen_action(&ids, act),
                act => self.album_action(&ids, act, shuffled),
            }
            return None;
        }
        Some(AlbumQueueWork { ids, action, shuffled, label, origin:self.browse_origin(), _work:self.begin_queue_work() })
    }
    pub fn finish_album_queue(&mut self, request: AlbumQueueWork, paths: &[String]) {
        self.with_queue_origin(request.origin, |api| match request.action.as_str() {
            "play" => api.play_list(paths, request.shuffled),
            "next" => api.play_next_here(paths),
            "later" => { api.play_later_here(paths, &request.label); }
            act if act.starts_with("buffer:") => api.listen_paths_action(act, paths),
            _ => api.enqueue_here(paths),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limited_shuffle_consumes_the_whole_candidate_set_and_keeps_distinct_occurrences() {
        let action = CollectionQueueAction::new("play".into(), true, 50, String::new());
        let visited = std::cell::Cell::new(0);
        let chosen = action.select((0..2500).map(|at| {
            visited.set(visited.get() + 1);
            at.to_string()
        }));
        assert_eq!(visited.get(), 2500);
        assert_eq!(chosen.len(), 50);
        assert_eq!(chosen.iter().collect::<std::collections::HashSet<_>>().len(), 50);
    }

    #[test]
    fn collection_limits_follow_availability_and_album_order() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let local = lib.source_id("local", "default").unwrap();
        lib.connection().execute_batch("INSERT INTO albums(id, title, album_artist) VALUES (1, 'One', 'Artist'), (2, 'Two', 'Artist')").unwrap();
        for (id, album, number, reachable) in [(1, 1, 2, 1), (2, 1, 1, 1), (3, 2, 1, 0), (4, 2, 2, 1)] {
            lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, album_id, track_number, reachable, added_at)
                VALUES(?1, ?2, ?3, 'Song', ?4, ?5, ?6, 1)", rusqlite::params![id, local, format!("/{id}.flac"), album, number, reachable]).unwrap();
        }
        let reader = crate::Reader::of(lib);
        let limited = CollectionQueueAction::new("queue".into(), false, 2, String::new());
        assert_eq!(reader.collection_track_paths(&[3, 1, 4, 2], &limited), ["/1.flac", "/4.flac"]);
        let remote = std::collections::HashMap::from([(-1, vec!["plugin:fixture:track:1".to_string()])]);
        let all = CollectionQueueAction::new("queue".into(), false, 0, String::new());
        assert_eq!(reader.collection_album_paths(&[2, -1, 1], remote, &all), ["/4.flac", "plugin:fixture:track:1", "/2.flac", "/1.flac"]);
    }

    #[test]
    fn random_album_uses_one_album_from_the_candidate_set_and_preserves_its_order() {
        let action = CollectionQueueAction::new("random-album".into(), false, 0, String::new());
        assert!(action.album_selection(Vec::new()).is_empty());
        let candidates = vec![-5, 42, 209];
        for _ in 0..20 {
            let selected = action.album_selection(candidates.clone());
            assert_eq!(selected.len(), 1);
            assert!(candidates.contains(&selected[0]));
        }
        let tracks = ["disc1track1", "disc1track2", "disc2track1"];
        assert_eq!(action.select(tracks.into_iter().map(String::from)), tracks);
    }

    #[test]
    fn overlapping_work_stays_busy_until_completion_or_cancellation() {
        let pending = Arc::new(AtomicUsize::new(2));
        let first = QueueWork(pending.clone());
        let second = QueueWork(pending.clone());
        drop(first);
        assert_eq!(pending.load(Ordering::Relaxed), 1);
        std::thread::spawn(move || drop(second)).join().unwrap();
        assert_eq!(pending.load(Ordering::Relaxed), 0);
    }
}
