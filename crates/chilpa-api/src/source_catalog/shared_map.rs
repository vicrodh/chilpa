//! I share immutable catalog entries with disk snapshots and copy only edited entries.

use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::collections::btree_map::{Iter, IterMut};
use std::ops::Index;
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct SharedMap<K: Ord, V>(
    BTreeMap<K, Arc<V>>,
    #[serde(skip, default = "fresh_generation")] Arc<()>,
);

fn fresh_generation() -> Arc<()> { Arc::new(()) }

impl<K: Ord, V> Default for SharedMap<K, V> {
    fn default() -> Self { Self(BTreeMap::new(), fresh_generation()) }
}

impl<K: Ord, V: Clone> SharedMap<K, V> {
    pub fn generation(&self) -> Arc<()> { self.1.clone() }
    pub fn matches_generation(&self, generation: &Arc<()>) -> bool { Arc::ptr_eq(&self.1, generation) }
    fn changing(&mut self) { self.1 = fresh_generation(); }
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V> where K: Borrow<Q> {
        self.0.get(key).map(Arc::as_ref)
    }
    pub fn shared<Q: Ord + ?Sized>(&self, key: &Q) -> Option<Arc<V>> where K: Borrow<Q> { self.0.get(key).cloned() }
    pub fn get_mut<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<&mut V> where K: Borrow<Q> {
        self.changing();
        self.0.get_mut(key).map(Arc::make_mut)
    }
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool where K: Borrow<Q> { self.0.contains_key(key) }
    pub fn insert(&mut self, key: K, value: V) { self.changing(); self.0.insert(key, Arc::new(value)); }
    pub fn insert_shared(&mut self, key: K, value: Arc<V>) { self.changing(); self.0.insert(key, value); }
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) where K: Borrow<Q> { self.changing(); self.0.remove(key); }
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) { self.changing(); self.0.retain(|key, value| keep(key, value)); }
    pub fn keys(&self) -> impl Iterator<Item = &K> { self.0.keys() }
    pub fn values(&self) -> impl Iterator<Item = &V> { self.0.values().map(Arc::as_ref) }
    pub fn values_mut_where(&mut self, mut keep: impl FnMut(&V) -> bool) -> impl Iterator<Item = &mut V> { self.changing(); self.0.values_mut().filter(move |value| keep(value)).map(Arc::make_mut) }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> { self.0.iter().map(|(key, value)| (key, value.as_ref())) }
    pub fn iter_mut_where(&mut self, mut keep: impl FnMut(&K, &V) -> bool) -> impl Iterator<Item = (&K, &mut V)> { self.changing(); self.0.iter_mut().filter(move |(key, value)| keep(key, value)).map(|(key, value)| (key, Arc::make_mut(value))) }
    pub fn len(&self) -> usize { self.0.len() }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }

    pub fn changed_keys(&self, before: &Self, equal: impl Fn(&V, &V) -> bool) -> Vec<K> where K: Clone {
        self.0.iter().filter(|(key, value)| before.0.get(*key).is_none_or(|old| !Arc::ptr_eq(value, old) && !equal(value, old)))
            .map(|(key, _)| key.clone()).chain(before.0.keys().filter(|key| !self.0.contains_key(*key)).cloned()).collect()
    }
}

impl<K: Ord + Borrow<Q>, Q: Ord + ?Sized, V: Clone> Index<&Q> for SharedMap<K, V> {
    type Output = V;
    fn index(&self, key: &Q) -> &V { self.get(key).expect("catalog entry exists") }
}

impl<'a, K: Ord, V> IntoIterator for &'a SharedMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::iter::Map<Iter<'a, K, Arc<V>>, fn((&'a K, &'a Arc<V>)) -> (&'a K, &'a V)>;
    fn into_iter(self) -> Self::IntoIter { self.0.iter().map(|(key, value)| (key, value.as_ref())) }
}

impl<'a, K: Ord, V: Clone> IntoIterator for &'a mut SharedMap<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = std::iter::Map<IterMut<'a, K, Arc<V>>, fn((&'a K, &'a mut Arc<V>)) -> (&'a K, &'a mut V)>;
    fn into_iter(self) -> Self::IntoIter { self.changing(); self.0.iter_mut().map(|(key, value)| (key, Arc::make_mut(value))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_share_payloads_until_an_entry_changes() {
        let mut live = SharedMap::default();
        live.insert(1, "original".to_string());
        let snapshot = live.clone();
        assert!(Arc::ptr_eq(&live.0[&1], &snapshot.0[&1]));
        *live.get_mut(&1).unwrap() = "updated".into();
        assert_eq!(snapshot[&1], "original");
        assert_eq!(live[&1], "updated");
    }
}

#[cfg(test)]
mod generation_tests {
    use super::*;
    type Map = SharedMap<i64, String>;

    fn changed(action: impl FnOnce(&mut Map)) {
        let mut map = Map::default();
        map.insert(1, "original".into());
        let snapshot = map.clone();
        let token = map.generation();
        action(&mut map);
        assert!(!map.matches_generation(&token));
        assert!(snapshot.matches_generation(&token));
        assert_eq!(snapshot.get(&1).unwrap(), "original");
    }

    #[test]
    fn every_mutable_entry_point_replaces_the_generation_before_access() {
        changed(|map| *map.get_mut(&1).unwrap() = "edited".into());
        changed(|map| map.insert(2, "new".into()));
        changed(|map| map.insert_shared(1, Arc::new("shared".into())));
        changed(|map| map.remove(&1));
        changed(|map| map.retain(|_, _| false));
        changed(|map| { for value in map.values_mut_where(|_| true) { value.push('x'); } });
        changed(|map| { for (_, value) in map.iter_mut_where(|_, _| true) { value.push('x'); } });
        changed(|map| { for (_, value) in map { value.push('x'); } });
        // Creating an unconsumed lazy mutable iterator already invalidates.
        changed(|map| { drop(map.values_mut_where(|_| false)); });
        changed(|map| { drop(map.iter_mut_where(|_, _| false)); });
        changed(|map| { drop(map.into_iter()); });
        changed(|map| { assert!(map.get_mut(&999).is_none()); });
    }

    #[test]
    fn reads_and_clones_keep_identity_and_detached_arc_changes_stay_detached() {
        let mut map = Map::default();
        map.insert(1, "original".into());
        let token = map.generation();
        let snapshot = map.clone();
        assert!(map.contains_key(&1));
        assert_eq!(map.keys().count(), 1);
        assert_eq!(map.values().count(), 1);
        assert_eq!(map.iter().count(), 1);
        assert_eq!((&map).into_iter().count(), 1);
        assert_eq!(map.len(), 1);
        assert!(!map.is_empty());
        assert!(map.changed_keys(&snapshot, |a, b| a == b).is_empty());
        assert_eq!(map[&1], "original");
        let mut detached = map.shared(&1).unwrap();
        assert!(Arc::get_mut(&mut detached).is_none());
        Arc::make_mut(&mut detached).push('x');
        assert_eq!(map.get(&1).unwrap(), "original");
        assert!(map.matches_generation(&token));
        assert!(snapshot.matches_generation(&token));
        map.insert_shared(1, detached);
        assert!(!map.matches_generation(&token));
    }

    #[test]
    fn generation_is_absent_from_wire_and_fresh_after_deserialization_or_replacement() {
        let mut map = Map::default();
        map.insert(1, "original".into());
        let wire = serde_json::to_string(&map).unwrap();
        assert_eq!(wire, r#"{"1":"original"}"#);
        let token = map.generation();
        let restored: Map = serde_json::from_str(&wire).unwrap();
        assert_eq!(restored.get(&1), map.get(&1));
        assert!(!restored.matches_generation(&token));
        let weak = Arc::downgrade(&token);
        drop(token);
        let old = std::mem::replace(&mut map, restored);
        assert!(weak.upgrade().is_some());
        drop(old);
        assert!(weak.upgrade().is_none());
        let token = map.generation();
        let old = std::mem::take(&mut map);
        assert!(!map.matches_generation(&token));
        assert!(old.matches_generation(&token));
    }
}
