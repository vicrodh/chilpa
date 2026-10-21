//! Favorites a source keeps: the heart of a track that lives in a service.
//!
//! The listener's favorite tracks in a source are that source's own list,
//! the "tracks" tab of its library. A track of the source is a favorite
//! when it is on that list. Marking one asks the source, off the window's
//! thread, and shows it at once: what was changed here is laid over the
//! list until the source says it could not be done.

use crate::Chilpa;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

/// How long a source has to keep or drop a favorite.
const ASKED: Duration = Duration::from_secs(30);

/// How a change asked of a source went.
struct Answer {
    plugin: String,
    identity: String,
    result: Result<(), String>,
}

pub(crate) struct SourceFavorites {
    /// What was changed here, by source and track, over what its list says.
    changed: HashMap<(String, String), bool>,
    tx: Sender<Answer>,
    rx: Receiver<Answer>,
}

impl Default for SourceFavorites {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { changed: HashMap::new(), tx, rx }
    }
}

impl SourceFavorites {
    pub(crate) fn has_pending(&self) -> bool { !self.changed.is_empty() }

    pub(crate) fn pending(&self, plugin: &str, identity: &str) -> bool {
        self.has_pending() && self.changed.contains_key(&(plugin.into(), identity.into()))
    }
}

/// The tab of a source's library that lists the listener's tracks.
fn listing(plugin: &str) -> String {
    format!("{plugin} section:tracks")
}

/// Which of the tracks known are favorites, given the source's own lists
/// and what was changed here since. Each track is its id, its source and
/// what the source calls it.
fn favorites_among(known: &[(i64, String, String)], listed: &HashSet<(String, String)>, changed: &HashMap<(String, String), bool>) -> HashSet<i64> {
    known.iter()
        .filter(|(_, plugin, identity)| {
            let key = (plugin.clone(), identity.clone());
            changed.get(&key).copied().unwrap_or_else(|| listed.contains(&key))
        })
        .map(|(id, ..)| *id)
        .collect()
}

impl Chilpa {
    /// Whether that source keeps favorite tracks.
    fn keeps_favorite_tracks(&self, plugin: &str) -> bool {
        self.natives.has_favorites(plugin) && self.natives.library_kinds(plugin).iter().any(|kind| kind == "tracks")
    }

    /// The source tracks that can be marked, and the ones that are, by the
    /// id each has here. The lists are asked for when missing or stale.
    pub(crate) fn source_track_marks(&self) -> (HashSet<i64>, HashSet<i64>, HashSet<i64>) {
        let sources: Vec<String> = self.natives.ids().into_iter().filter(|plugin| self.keeps_favorite_tracks(plugin)).collect();
        if sources.is_empty() {
            return Default::default();
        }
        let known: Vec<(i64, String, String)> = self.catalog.rating_identities().into_iter()
            .filter(|(kind, _, plugin, identity)| kind == "track" && identity.starts_with("track:") && sources.contains(plugin))
            .map(|(_, id, plugin, identity)| (id, plugin, identity))
            .collect();
        let mut listed = HashSet::new();
        for plugin in &sources {
            let key = listing(plugin);
            self.catalog.shelf(&key);
            let kept = self.catalog.rating_identities_of(&key);
            listed.extend(kept.into_iter().map(|identity| (plugin.clone(), identity)));
        }
        let marked = favorites_among(&known, &listed, &self.source_favorites.changed);
        let pending = if self.source_favorites.changed.is_empty() { HashSet::new() }
            else { known.iter().filter(|(_, plugin, identity)| self.source_favorites.pending(plugin, identity)).map(|(id, ..)| *id).collect() };
        (known.into_iter().map(|(id, ..)| id).collect(), marked, pending)
    }

    pub fn source_favorite_pending(&self, plugin: &str, reference: &str) -> bool {
        self.sources.native_actions.get(plugin).is_some_and(|state| state["pending"] == true && state["favorite"]["ref"] == reference)
    }

    /// Marks those source tracks, or takes the mark off. Shown at once; the
    /// source is asked on a thread of its own.
    pub(crate) fn set_source_tracks_favorite(&mut self, ids: &[i64], on: bool) {
        let chosen: Vec<(String, String)> = self.catalog.rating_identities().into_iter()
            .filter(|(kind, id, plugin, identity)| kind == "track" && ids.contains(id) && identity.starts_with("track:") && self.keeps_favorite_tracks(plugin))
            .map(|(_, _, plugin, identity)| (plugin, identity))
            .collect::<HashSet<_>>().into_iter().collect();
        for (plugin, identity) in chosen {
            if self.source_favorites.changed.contains_key(&(plugin.clone(), identity.clone())) { continue; }
            self.source_favorites.changed.insert((plugin.clone(), identity.clone()), on);
            let (natives, tx) = (self.natives.clone(), self.source_favorites.tx.clone());
            std::thread::spawn(move || {
                let result = natives.call(&plugin, "favorites.set", json!({"ref": identity, "on": on}), ASKED).map(|_| ());
                let _ = tx.send(Answer { plugin, identity, result });
            });
        }
        self.catalog.touch();
    }

    /// Takes in how the changes asked of the sources went.
    pub(crate) fn advance_source_favorites(&mut self) {
        while let Ok(answer) = self.source_favorites.rx.try_recv() {
            match answer.result {
                // Its list changed: it is read again.
                Ok(()) => {
                    self.source_favorites.changed.remove(&(answer.plugin.clone(), answer.identity));
                    self.catalog.retry(&listing(&answer.plugin));
                }
                Err(error) => {
                    self.source_favorites.changed.remove(&(answer.plugin, answer.identity));
                    self.now.error = chilpa_i18n::t_args("the favorite could not be saved: {}", &[&error]);
                }
            }
            self.catalog.touch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(plugin: &str, identity: &str) -> (String, String) {
        (plugin.into(), identity.into())
    }

    #[test]
    fn a_track_is_a_favorite_when_its_source_lists_it_or_it_was_marked_here() {
        // The same song twice, as an album's track and as a playlist's.
        let known = vec![(-1, "one".to_string(), "track:10".to_string()), (-2, "one".into(), "track:10".into()),
            (-3, "one".into(), "track:11".into()), (-4, "two".into(), "track:10".into())];
        let listed: HashSet<_> = [key("one", "track:10")].into();
        let mut changed = HashMap::new();
        assert_eq!(favorites_among(&known, &listed, &changed), [-1, -2].into(), "every row of the song, and no other source's");
        changed.insert(key("one", "track:11"), true);
        changed.insert(key("one", "track:10"), false);
        assert_eq!(favorites_among(&known, &listed, &changed), [-3].into(), "what was changed here goes over the list");
    }

    #[test]
    fn a_finished_track_write_clears_only_its_pending_identity_on_success_or_failure() {
        for result in [Ok(()), Err("Write failed".into())] {
            let mut api = Chilpa::new();
            let previous_error = api.now.error.clone();
            api.source_favorites.changed.insert(key("one", "track:10"), true);
            api.source_favorites.changed.insert(key("two", "track:10"), false);
            assert!(api.source_favorites.pending("one", "track:10"));
            let failed = result.is_err();
            api.source_favorites.tx.send(Answer { plugin:"one".into(), identity:"track:10".into(), result }).unwrap();
            api.advance_source_favorites();
            assert!(!api.source_favorites.pending("one", "track:10"));
            assert!(api.source_favorites.pending("two", "track:10"));
            if failed { assert!(api.now.error.contains("Write failed")); }
            else { assert_eq!(api.now.error, previous_error); }
        }
    }

    #[test]
    fn pending_page_favorites_match_the_source_action_and_reference() {
        let mut api = Chilpa::new();
        api.sources.native_actions.insert("one".into(), json!({"pending":true,"favorite":{"ref":"album:10","on":true}}));
        assert!(api.source_favorite_pending("one", "album:10"));
        assert!(!api.source_favorite_pending("two", "album:10"));
        assert!(!api.source_favorite_pending("one", "album:11"));
        api.sources.native_actions.insert("one".into(), json!({"pending":false,"error":"Write failed"}));
        assert!(!api.source_favorite_pending("one", "album:10"));
    }
}
