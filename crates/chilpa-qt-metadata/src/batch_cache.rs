//! Sparse absolute offsets keep byte-limited responses from leaving page holes.
use std::collections::{BTreeMap, VecDeque};

const MAX_ROWS: usize = 400;
const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct BatchCache {
    rows: BTreeMap<usize, String>,
    order: VecDeque<usize>,
    bytes: usize,
}
impl BatchCache {
    pub fn clear(&mut self) { self.rows.clear(); self.order.clear(); self.bytes = 0; }
    pub fn get(&self, index: usize) -> Option<&str> { self.rows.get(&index).map(String::as_str) }
    pub fn first_missing(&self, first: usize, last: usize) -> Option<usize> {
        (first..=last).find(|index| !self.rows.contains_key(index))
    }
    pub fn insert(&mut self, index: usize, row: String, visible: (usize, usize)) {
        if row.len() > MAX_BYTES { return; }
        if let Some(old) = self.rows.remove(&index) { self.bytes -= old.len(); }
        self.order.retain(|held| *held != index);
        while self.rows.len() >= MAX_ROWS || self.bytes + row.len() > MAX_BYTES {
            let position = self.order.iter().position(|held| *held < visible.0 || *held > visible.1).unwrap_or(0);
            let Some(old) = self.order.remove(position) else { break };
            if let Some(value) = self.rows.remove(&old) { self.bytes -= value.len(); }
        }
        self.bytes += row.len();
        self.rows.insert(index,row);
        self.order.push_back(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_short_response_continues_at_the_missing_absolute_row() {
        let mut cache = BatchCache::default();
        for index in 300..303 { cache.insert(index,index.to_string(),(300,325)); }
        assert_eq!(cache.first_missing(300,325),Some(303));
        cache.insert(303,"next".into(),(300,325));
        assert_eq!(cache.get(303),Some("next"));
        assert_eq!(cache.get(399),None);
    }
    #[test]
    fn eviction_releases_rows_outside_the_view_before_visible_rows() {
        let mut cache = BatchCache::default();
        for index in 0..MAX_ROWS { cache.insert(index,"row".into(),(0,10)); }
        cache.insert(MAX_ROWS,"new".into(),(0,10));
        assert_eq!(cache.get(0),Some("row"));
        assert_eq!(cache.get(11),None);
        assert_eq!(cache.rows.len(),MAX_ROWS);
        cache.clear();
        assert!(cache.rows.is_empty());
        assert_eq!(cache.bytes,0);
    }
    #[test]
    fn byte_budget_releases_large_offscreen_rows() {
        let mut cache = BatchCache::default();
        for index in 0..10 { cache.insert(index,"x".repeat(1024 * 1024),(0,1)); }
        assert_eq!(cache.bytes,MAX_BYTES);
        assert_eq!(cache.rows.len(),4);
        assert!(cache.get(0).is_some() && cache.get(1).is_some());
        assert!(cache.get(9).is_some());
    }
}
