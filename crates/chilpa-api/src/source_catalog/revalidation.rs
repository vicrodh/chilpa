//! I check complete personal shelves without publishing a foreground loading cycle.

use super::{Catalog, Inner, Job};

/// `urgent`: a page waits for it, so it goes first; it stays quiet unless it
/// was already queued as a foreground read.
pub(super) fn queue(catalog: &Catalog, inner: &mut Inner, key: String, urgent: bool) {
    let job = Job::Shelf(key.clone());
    if inner.running.contains(&job) { return; }
    let quiet = inner.revalidating.contains(&job) || !inner.queue.queued.contains(&job);
    let cached = inner.store.shelves.get(&key).is_some_and(|shelf| shelf.epoch > 0)
        && !inner.shelf_errors.contains_key(&key);
    catalog.queue(inner, job.clone(), urgent);
    if quiet && cached && inner.queue.queued.contains(&job) { inner.revalidating.insert(job); }
}

pub(super) fn loading(inner: &Inner, key: &str) -> bool {
    let job = Job::Shelf(key.into());
    let cached = inner.revalidating.contains(&job)
        && inner.store.shelves.get(key).is_some_and(|shelf| shelf.epoch > 0);
    !inner.loaded || !cached && (inner.running.contains(&job) || inner.queue.queued.contains(&job))
}

#[cfg(test)]
mod tests;
