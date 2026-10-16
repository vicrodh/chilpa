//! I retain queue activity until path resolution and insertion both finish.
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
impl Drop for QueueWork {
    fn drop(&mut self) { self.0.fetch_sub(1, Ordering::Relaxed); }
}
impl crate::Chilpa {
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
