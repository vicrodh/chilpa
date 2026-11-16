//! One coalesced, fixed-size counter cell per session. No rows or paths cross
//! this mailbox. Reading progress never publishes a page or changes its revision.
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataBatchPhase {
    #[default]
    Idle,
    Opening,
    Working,
    Saving,
    Finalizing,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataBatchProgress {
    pub session: u64,
    pub operation_id: u64,
    pub revision: u64,
    pub phase: MetadataBatchPhase,
    pub completed: u64,
    pub total: u64,
    pub total_known: bool,
    pub saved: u64,
    pub failed: u64,
    pub cancelled: bool,
}

#[derive(Clone, Default)]
pub(super) struct Progress(Arc<Mutex<MetadataBatchProgress>>);
impl Progress {
    pub fn read(&self) -> MetadataBatchProgress {
        *self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
    pub fn update(&self, change: impl FnOnce(&mut MetadataBatchProgress)) {
        let mut held = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let before = *held;
        change(&mut held);
        if *held != before { held.revision = before.revision.wrapping_add(1); }
    }
    pub fn begin(&self, session: u64, phase: MetadataBatchPhase) {
        self.update(|held| *held = MetadataBatchProgress {
            session, phase, operation_id: held.operation_id.wrapping_add(1),
            revision: held.revision, ..Default::default()
        });
    }
    pub fn finish(&self, cancelled: bool, failed: bool) {
        self.update(|held| {
            held.cancelled = cancelled;
            held.phase = if failed || held.failed > 0 { MetadataBatchPhase::Failed }
                else if cancelled { MetadataBatchPhase::Cancelled } else { MetadataBatchPhase::Complete };
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counters_are_bounded_and_cancellation_keeps_partial_completion() {
        assert!(std::mem::size_of::<MetadataBatchProgress>() <= 128);
        let progress = Progress::default();
        progress.begin(7, MetadataBatchPhase::Saving);
        progress.update(|p| { p.total = 6; p.total_known = true; p.completed = 2; p.saved = 1; p.failed = 1; });
        progress.finish(true, false);
        let stopped = progress.read();
        assert_eq!((stopped.completed, stopped.total, stopped.saved, stopped.failed), (2, 6, 1, 1));
        assert!(stopped.cancelled);
        progress.update(|_| {});
        assert_eq!(progress.read().revision, stopped.revision);
        progress.begin(7, MetadataBatchPhase::Saving);
        assert_eq!(progress.read().operation_id, stopped.operation_id + 1);
        assert_eq!(progress.read().saved, 0);
    }
}
