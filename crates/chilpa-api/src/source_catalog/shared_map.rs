//! I share immutable catalog entries with disk snapshots and copy only edited entries.

use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::collections::btree_map::{Iter, IterMut};
use std::ops::Index;
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct SharedMap<K: Ord, V>(BTreeMap<K, Arc<V>>);

impl<K: Ord, V> Default for SharedMap<K, V> {
    fn default() -> Self { Self(BTreeMap::new()) }
}

impl<K: Ord, V: Clone> SharedMap<K, V> {
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V> where K: Borrow<Q> {
        self.0.get(key).map(Arc::as_ref)
    }
    pub fn shared<Q: Ord + ?Sized>(&self, key: &Q) -> Option<Arc<V>> where K: Borrow<Q> { self.0.get(key).cloned() }
    pub fn get_mut<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<&mut V> where K: Borrow<Q> {
        self.0.get_mut(key).map(Arc::make_mut)
    }
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool where K: Borrow<Q> { self.0.contains_key(key) }
    pub fn insert(&mut self, key: K, value: V) { self.0.insert(key, Arc::new(value)); }
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) where K: Borrow<Q> { self.0.remove(key); }
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) { self.0.retain(|key, value| keep(key, value)); }
    pub fn keys(&self) -> impl Iterator<Item = &K> { self.0.keys() }
    pub fn values(&self) -> impl Iterator<Item = &V> { self.0.values().map(Arc::as_ref) }
    pub fn values_mut_where(&mut self, mut keep: impl FnMut(&V) -> bool) -> impl Iterator<Item = &mut V> { self.0.values_mut().filter(move |value| keep(value)).map(Arc::make_mut) }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> { self.0.iter().map(|(key, value)| (key, value.as_ref())) }
    pub fn iter_mut_where(&mut self, mut keep: impl FnMut(&K, &V) -> bool) -> impl Iterator<Item = (&K, &mut V)> { self.0.iter_mut().filter(move |(key, value)| keep(key, value)).map(|(key, value)| (key, Arc::make_mut(value))) }
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
    fn into_iter(self) -> Self::IntoIter { self.0.iter_mut().map(|(key, value)| (key, Arc::make_mut(value))) }
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
