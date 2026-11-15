//! The manager's published document and its small, independently changing activity state.
use serde_json::{json, Value};
use std::sync::{Mutex, atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}};

pub(super) struct View {
    document: Mutex<Value>,
    revision: AtomicU64,
    busy: AtomicUsize,
    foreground: AtomicBool,
    operation: Mutex<String>,
    #[cfg(test)]
    document_reads: AtomicUsize,
}

impl Default for View {
    fn default() -> Self {
        Self {
            document: Mutex::new(json!({})),
            revision: AtomicU64::new(1),
            busy: AtomicUsize::new(0),
            foreground: AtomicBool::new(false),
            operation: Mutex::new(String::new()),
            #[cfg(test)]
            document_reads: AtomicUsize::new(0),
        }
    }
}

impl View {
    // Every writer publishes its revision after changing the observable fields.
    fn changed(&self) { self.revision.fetch_add(1, Ordering::Release); }

    pub(super) fn publish(&self, document: &Value) {
        if let Ok(mut previous) = self.document.lock() {
            if *previous != *document {
                *previous = document.clone();
                self.changed();
            }
        }
    }

    pub(super) fn begin(&self, background: bool, action: &str) -> bool {
        if background {
            if self.foreground.load(Ordering::Acquire)
                || self.busy.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_err() { return false; }
        } else {
            if self.foreground.swap(true, Ordering::AcqRel) { return false; }
            self.busy.fetch_add(1, Ordering::AcqRel);
            if let Ok(mut operation) = self.operation.lock() { *operation = action.into(); }
        }
        self.changed();
        true
    }

    pub(super) fn finish(&self, background: bool) {
        if !background {
            if let Ok(mut operation) = self.operation.lock() { operation.clear(); }
            self.foreground.store(false, Ordering::Release);
        }
        self.busy.fetch_sub(1, Ordering::AcqRel);
        self.changed();
    }

    pub(super) fn reject(&self, background: bool) {
        // Preserve the legacy rejected-command document, including its operation label.
        if !background { self.foreground.store(false, Ordering::Release); }
        self.busy.fetch_sub(1, Ordering::AcqRel);
        self.changed();
    }

    #[cfg(test)]
    pub(super) fn working(&self) -> usize { self.busy.load(Ordering::Acquire) }

    pub(super) fn snapshot(&self) -> Value {
        #[cfg(test)]
        self.document_reads.fetch_add(1, Ordering::Relaxed);
        let mut value = self.document.lock().map(|value| value.clone()).unwrap_or(json!({}));
        value["operation"] = self.operation.lock().map(|operation| operation.clone()).unwrap_or_default().into();
        value["busy"] = self.foreground.load(Ordering::Acquire).into();
        value["working"] = (self.busy.load(Ordering::Acquire) != 0).into();
        value
    }

    pub(super) fn snapshot_since(&self, seen: Option<u64>) -> Option<(u64, String)> {
        // Capture before reading the document. A concurrent publication may enter this
        // snapshot, but its later revision remains visible to the next call.
        let revision = self.revision.load(Ordering::Acquire);
        if seen == Some(revision) { return None; }
        Some((revision, self.snapshot().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::{Duration, Instant}};

    #[test]
    fn unchanged_reads_skip_the_document_and_equal_publications_keep_the_revision() {
        let view = View::default();
        let document = json!({"catalog":[{"id":"fixture", "description":"x".repeat(1024 * 1024)}]});
        view.publish(&document);
        let (revision, serialized) = view.snapshot_since(None).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&serialized).unwrap()["catalog"], document["catalog"]);
        assert_eq!(view.document_reads.load(Ordering::Relaxed), 1);
        for _ in 0..100 {
            assert!(view.snapshot_since(Some(revision)).is_none());
        }
        view.publish(&document);
        assert!(view.snapshot_since(Some(revision)).is_none());
        assert_eq!(view.document_reads.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn activity_changes_publish_even_when_the_document_is_identical() {
        let view = View::default();
        let (initial, _) = view.snapshot_since(None).unwrap();
        assert!(view.begin(true, "tick"));
        let (maintenance, text) = view.snapshot_since(Some(initial)).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(doc["busy"], false);
        assert_eq!(doc["working"], true);
        assert!(view.begin(false, "settings"));
        let (queued, text) = view.snapshot_since(Some(maintenance)).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(doc["busy"], true);
        assert_eq!(doc["operation"], "settings");
        assert!(!view.begin(false, "install"));
        assert!(!view.begin(true, "downloads_tick"));
        assert!(view.snapshot_since(Some(queued)).is_none());
        view.finish(true);
        let (maintenance_done, text) = view.snapshot_since(Some(queued)).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(doc["busy"], true);
        assert_eq!(doc["working"], true);
        assert_eq!(doc["operation"], "settings");
        view.finish(false);
        let (_, text) = view.snapshot_since(Some(maintenance_done)).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(doc["busy"], false);
        assert_eq!(doc["working"], false);
        assert_eq!(doc["operation"], "");
    }

    #[test]
    fn a_publication_during_a_document_read_remains_visible_on_the_next_read() {
        let view = Arc::new(View::default());
        let expected = view.revision.load(Ordering::Acquire);
        let mut document = view.document.lock().unwrap();
        let reader = view.clone();
        let thread = std::thread::spawn(move || reader.snapshot_since(None).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        while view.document_reads.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
            std::thread::yield_now();
        }
        let entered = view.document_reads.load(Ordering::Acquire) != 0;
        *document = json!({"form":{"status":"completed"}});
        view.changed();
        drop(document);
        let (revision, text) = thread.join().unwrap();
        assert!(entered, "reader reached the document while publication was held");
        assert_eq!(revision, expected, "revision was captured before reading the document");
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["form"]["status"], "completed");
        let (next, _) = view.snapshot_since(Some(revision)).unwrap();
        assert_ne!(next, revision);
        assert!(view.snapshot_since(Some(next)).is_none());
    }
}
