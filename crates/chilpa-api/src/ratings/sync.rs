//! Ratings that go both ways between Chilpa and the source of an item: a
//! server of the library whose kind keeps ratings, or a plugin that
//! declares `rating` (`scale`).
//!
//! What a source says comes in where it is read: a server's with its
//! catalogue (`servers::ratings`), a plugin's from its lists as the catalog
//! holds them (here). What the listener changes here is queued in the
//! library and sent by a thread of its own (`writer`), again and again
//! while the source cannot take it, the newest change of an item replacing
//! an older one. Nothing is ever written to a source unless the listener
//! changed it here: a value Chilpa only shows rounded stays as it is.

use super::scale::Scale;
use super::writer::Writer;
use crate::Chilpa;
use chilpa_library::store::rating_sync::{Holder, Incoming};
use chilpa_library::store::ratings::RatingTarget;
use std::collections::{BTreeMap, HashMap};

/// An origin is what a rating is kept in step with: a server, by its
/// instance (never its source row, which a later server may reuse), or a
/// plugin, by its id.
pub(crate) fn server_origin(instance: &str) -> String {
    format!("server:{instance}")
}

pub(crate) fn plugin_origin(plugin: &str) -> String {
    format!("plugin:{plugin}")
}

/// The time now in unix milliseconds: when a listing was asked for, or a
/// change was sent.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// What the window keeps of it between polls.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) writer: Option<Writer>,
    /// When the next change is due, in unix seconds, as last known. `None`
    /// once nothing waits; unknown until the library was asked.
    next_due: Option<i64>,
    looked: bool,
    /// The thread is sending: it is not woken again until it says how it went.
    sending: bool,
    /// How many changes failed at the last run: the window is told once
    /// when some start failing, not at every retry.
    failing: usize,
    /// The catalog revision the plugins' ratings were last taken in at, and
    /// each item's value then: only what changed is taken in again.
    imported_rev: Option<i32>,
    imported: HashMap<(String, String, String), f64>,
}

impl State {
    /// Something was queued: the writer is due now.
    pub(crate) fn due_now(&mut self) {
        self.next_due = Some(0);
        self.looked = true;
    }
}

impl Chilpa {
    /// The listener changed an item's stars here, from `before` to `stars`:
    /// it goes to the item's source when that source keeps ratings and they
    /// are synced with it. Only queued here; the network is the writer's.
    pub(crate) fn rating_changed(&mut self, kind: &str, id: i64, before: u8, stars: u8) {
        if before == stars || !matches!(kind, "track" | "album") {
            return;
        }
        let queued = if id > 0 { self.queue_server_rating(kind, id, stars) } else { self.queue_plugin_rating(kind, id, stars) };
        if queued {
            self.servers.ratings.due_now();
        }
    }

    fn queue_server_rating(&mut self, kind: &str, id: i64, stars: u8) -> bool {
        let target = if kind == "track" { RatingTarget::Track(id) } else { RatingTarget::Album(id) };
        let Some(lib) = self.library() else { return false };
        let Some((source, local)) = lib.rated_on_server(target).ok().flatten() else { return false };
        let Some(stored) = lib.server(source).ok().flatten() else { return false };
        let Some(scale) = chilpa_servers::Kind::from_provider(&stored.provider).and_then(chilpa_servers::Kind::rating_scale).and_then(Scale::of_server) else { return false };
        let origin = server_origin(&stored.instance);
        if !lib.rating_sync_enabled(&origin).unwrap_or(false) {
            return false;
        }
        let own = if kind == "track" {
            match chilpa_models::RemoteEntry::parse(&local) {
                Some(entry) => entry.external_id,
                None => return false,
            }
        } else {
            local
        };
        queue(lib, &origin, kind, &own, stars, scale)
    }

    fn queue_plugin_rating(&mut self, kind: &str, id: i64, stars: u8) -> bool {
        let Some((actual, plugin, identity)) = self.catalog.rating_identities().into_iter()
            .find(|(actual, found, _, _)| *found == id && actual == kind)
            .map(|(actual, _, plugin, identity)| (actual, plugin, identity)) else { return false };
        let Some((scale, kinds)) = super::scale::declared(&self.natives.capability(&plugin, "rating")) else { return false };
        if !kinds.contains(&actual) {
            return false;
        }
        let origin = plugin_origin(&plugin);
        let Some(lib) = self.library() else { return false };
        if !lib.rating_sync_enabled(&origin).unwrap_or(false) {
            return false;
        }
        queue(lib, &origin, &actual, &identity, stars, scale)
    }

    /// The stars an item has here before a change: a library id's, or a
    /// plugin item's.
    pub(crate) fn stars_before(&mut self, kind: &str, id: i64) -> u8 {
        if id > 0 {
            let target = match kind {
                "track" => RatingTarget::Track(id),
                "album" => RatingTarget::Album(id),
                "playlist" => RatingTarget::Playlist(id),
                _ => return 0,
            };
            return self.library().and_then(|lib| lib.stars_of(target).ok()).unwrap_or(0);
        }
        let Some((actual, _, plugin, identity)) = self.catalog.rating_identities().into_iter()
            .find(|(actual, found, _, _)| *found == id && (actual == kind || kind == "album" && actual == "playlist")) else { return 0 };
        self.library().and_then(|lib| lib.external_stars(&plugin, &actual, &identity).ok()).unwrap_or(0)
    }

    /// What the polls of the window do for ratings: take in what plugins'
    /// lists say, read how the writer did, and wake it when a change is due.
    pub(crate) fn poll_ratings(&mut self) {
        self.import_plugin_ratings();
        let outcomes = self.servers.ratings.writer.as_ref().map(Writer::poll).unwrap_or_default();
        for outcome in outcomes {
            let state = &mut self.servers.ratings;
            state.sending = false;
            state.next_due = outcome.next_due;
            state.looked = true;
            if outcome.failing > 0 && state.failing == 0 {
                self.now.error = chilpa_i18n::t("Some ratings could not reach their source. Chilpa will try again.");
            }
            self.servers.ratings.failing = outcome.failing;
            if outcome.sent > 0 || outcome.dropped > 0 {
                self.servers.changed = true;
            }
        }
        if !self.servers.ratings.looked {
            self.servers.ratings.looked = true;
            self.servers.ratings.next_due = self.library().and_then(|lib| lib.next_rating_due().ok()).flatten();
        }
        let due = self.servers.ratings.next_due.is_some_and(|at| at <= crate::now_unix());
        if !due || self.servers.ratings.sending || self.offline() {
            return;
        }
        if self.servers.ratings.writer.is_none() {
            let Some(keys) = self.servers.keys.clone() else { return };
            self.servers.ratings.writer = Some(Writer::start(super::writer::Tools::new(keys, self.natives.clone(), self.catalog.clone())));
        }
        if self.servers.ratings.writer.as_ref().is_some_and(Writer::wake) {
            self.servers.ratings.sending = true;
        }
    }

    /// Takes in the ratings plugins' lists carry, for the plugins that
    /// declare `rating`, when the catalog changed. Only an item whose value
    /// changed since the last time is looked at again.
    pub(crate) fn import_plugin_ratings(&mut self) {
        let revision = self.catalog.revisions().0;
        if self.servers.ratings.imported_rev == Some(revision) {
            return;
        }
        self.servers.ratings.imported_rev = Some(revision);
        let scales: HashMap<String, (Scale, Vec<String>)> = self.natives.with_capability("rating").into_iter()
            .filter_map(|plugin| Some((plugin.clone(), super::scale::declared(&self.natives.capability(&plugin, "rating"))?)))
            .collect();
        if scales.is_empty() {
            return;
        }
        let mut groups: BTreeMap<(String, String), BTreeMap<String, f64>> = BTreeMap::new();
        for (kind, plugin, identity, value) in self.catalog.source_ratings() {
            let Some((_, kinds)) = scales.get(&plugin) else { continue };
            if !kinds.contains(&kind) || self.servers.ratings.imported.get(&(plugin.clone(), kind.clone(), identity.clone())) == Some(&value) {
                continue;
            }
            groups.entry((plugin, kind)).or_default().insert(identity, value);
        }
        let read_ms = now_ms();
        let mut changed = false;
        for ((plugin, kind), items) in groups {
            let scale = scales[&plugin].0;
            let reads: Vec<Incoming> = items.iter()
                .map(|(identity, value)| Incoming { id: identity.clone(), local: identity.clone(), raw: *value, stars: scale.stars(*value), read_ms })
                .collect();
            let Some(lib) = self.library() else { return };
            match lib.apply_remote_ratings(&plugin_origin(&plugin), &Holder::Plugin(plugin.clone()), &kind, &reads, false) {
                Ok(applied) => {
                    changed |= applied.changed > 0;
                    for (identity, value) in items {
                        self.servers.ratings.imported.insert((plugin.clone(), kind.clone(), identity), value);
                    }
                }
                Err(error) => eprintln!("ratings: the ratings of plugin {plugin} could not be taken in: {error}"),
            }
        }
        if changed {
            self.catalog.touch();
            self.servers.ratings.imported_rev = Some(self.catalog.revisions().0);
        }
    }

    /// Per origin, how many changes wait after failing and why the last
    /// one failed.
    pub(crate) fn rating_failures(&mut self) -> HashMap<String, (usize, String)> {
        self.library().and_then(|lib| lib.rating_failures().ok()).unwrap_or_default()
            .into_iter().map(|(origin, count, why)| (origin, (count, why))).collect()
    }
}

/// Queues one change for its source, unless the source already holds it.
fn queue(lib: &chilpa_library::Library, origin: &str, kind: &str, id: &str, stars: u8, scale: Scale) -> bool {
    let remote = lib.remote_rating(origin, kind, id).ok().flatten().map(|raw| scale.stars(raw));
    match lib.queue_rating(origin, kind, id, stars, remote) {
        Ok(queued) => queued,
        Err(error) => {
            eprintln!("ratings: a change for {origin} could not be queued: {error}");
            false
        }
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
