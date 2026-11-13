//! Walking the queue: the stop mark and the next track that can play.

use std::path::PathBuf;

use super::{Order, Repeat, RepeatMode, SharedQueue};

/// A queue position after which playback stops. It is consumed when reached,
/// so repeating does not inherit an old stop point. Every play has its own:
/// one that is being replaced, still failing to open a track or walking past
/// removed positions, has no way to spend the mark of the play replacing it.
#[derive(Clone)]
pub struct StopAfter(std::sync::Arc<std::sync::Mutex<usize>>);

impl Default for StopAfter {
    fn default() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(Self::NONE)))
    }
}

impl StopAfter {
    const NONE: usize = usize::MAX;

    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, usize> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Sets or clears the mark.
    pub fn set(&self, index: Option<usize>) {
        *self.lock() = index.unwrap_or(Self::NONE);
    }

    pub fn take_at(&self, index: usize) -> bool {
        let mut mark = self.lock();
        let reached = index != Self::NONE && *mark == index;
        if reached {
            *mark = Self::NONE;
        }
        reached
    }

    pub fn clear(&self) {
        self.set(None);
    }
}

/// The track at `index`, skipping the removed ones.
///
/// Advances `index` until it finds a live one. Returns `None` when none
/// are left, counting laps so as not to spin forever if the whole queue
/// was removed with repeat turned on, and at a removed one that carries the
/// `stop_after` mark: a position passed without opening still holds it. A
/// copy of an entry in `passed` is removed here, as it is reached.
pub(super) fn next_live(
    queue: &SharedQueue,
    order: &Order,
    repeat: &RepeatMode,
    index: &mut usize,
    stop_after: Option<&StopAfter>,
    passed: &std::collections::HashSet<PathBuf>,
) -> Option<PathBuf> {
    let wrap = repeat.get() == Repeat::All;
    for _ in 0..=queue.len() {
        match queue.get(*index) {
            Some(path) if passed.contains(&path) => { queue.remove(*index); }
            Some(path) => return Some(path),
            None => {}
        }
        if *index >= queue.len() || stop_after.is_some_and(|mark| mark.take_at(*index)) {
            return None;
        }
        *index = order.next(*index, queue.len(), wrap)?;
    }
    None
}
