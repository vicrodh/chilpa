//! I retain the remote records named by open tabs before their pages mount.

use super::Catalog;
use crate::navigation::{Route, Screen};
use std::collections::HashSet;

#[derive(Default, PartialEq, Eq)]
pub(super) struct NavigationRoots {
    pub cards: HashSet<i64>,
    pub shelves: HashSet<String>,
    pub identities: HashSet<(String, String)>,
}

impl Catalog {
    /// I read routing metadata without looking up artwork on disk or scheduling it.
    pub(crate) fn navigation_card(&self, id: i64) -> Option<super::store::Card> {
        self.lock().store.cards.get(&id).cloned()
    }

    pub(crate) fn protect_navigation(&self, routes: &[Route]) {
        let mut roots = NavigationRoots::default();
        for route in routes {
            match route.screen {
                Screen::Album | Screen::Playlist if route.id < 0 => { roots.cards.insert(route.id); }
                Screen::Source => {
                    roots.shelves.insert(route.key.clone());
                    let (plugin, address) = super::split(&route.key);
                    if plugin.is_empty() { continue; }
                    if let Some(reference) = address.strip_prefix("native:").filter(|reference| !reference.is_empty()) {
                        roots.identities.insert((plugin.to_owned(), reference.to_owned()));
                    } else if let Some(id) = address.strip_prefix("card:").and_then(|id| id.parse::<i64>().ok()).filter(|id| *id < 0) {
                        roots.cards.insert(id);
                    }
                }
                _ => {}
            }
        }
        let mut inner = self.lock();
        let removed: HashSet<_> = inner.navigation.shelves.difference(&roots.shelves).map(|key| key.split('|').next().unwrap_or(key).to_string()).collect();
        let kept: HashSet<_> = roots.shelves.iter().map(|key| key.split('|').next().unwrap_or(key).to_string()).collect();
        let cancelled: HashSet<_> = inner.queue.queued.iter().filter(|job| match job {
            super::Job::Shelf(key) | super::Job::Append(key) => {
                let family = key.split('|').next().unwrap_or(key);
                removed.contains(family) && !kept.contains(family)
            }
            _ => false,
        }).cloned().collect();
        inner.queue.urgent.retain(|job| !cancelled.contains(job));
        inner.queue.later.retain(|job| !cancelled.contains(job));
        inner.queue.queued.retain(|job| !cancelled.contains(job));
        if inner.navigation != roots {
            inner.navigation = roots;
            inner.residency.changed();
            self.wake.notify_one();
        }
    }
}

#[cfg(test)]
mod tests;
