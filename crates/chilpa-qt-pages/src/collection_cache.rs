//! Eight projected chunks, four MiB total, addressed by absolute row offsets.
use std::collections::VecDeque;

const MAX_CHUNKS: usize = 8;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const CHUNK_BYTES: usize = 512 * 1024;

struct Chunk { first: usize, rows: Vec<String>, bytes: usize }
#[derive(Default)]
pub(crate) struct CollectionCache { chunks: VecDeque<Chunk>, bytes: usize }
impl CollectionCache {
    pub fn clear(&mut self) { self.chunks.clear(); self.bytes = 0; }
    pub fn get(&self, index: usize) -> Option<&str> {
        self.chunks.iter().rev().find_map(|chunk| index.checked_sub(chunk.first).and_then(|local| chunk.rows.get(local)).map(String::as_str))
    }
    pub fn first_missing(&self, first: usize, last: usize) -> Option<usize> {
        (first..=last).find(|index| self.get(*index).is_none())
    }
    pub fn insert(&mut self, first: usize, rows: Vec<String>, visible: (usize, usize)) {
        let bytes = rows.iter().map(String::len).sum::<usize>();
        if rows.is_empty() || rows.len() > 100 || bytes > CHUNK_BYTES { return; }
        if let Some(at) = self.chunks.iter().position(|chunk| chunk.first == first) {
            self.bytes -= self.chunks.remove(at).expect("existing chunk").bytes;
        }
        while self.chunks.len() >= MAX_CHUNKS || self.bytes + bytes > MAX_BYTES {
            let at = self.chunks.iter().position(|chunk| chunk.first > visible.1 || chunk.first + chunk.rows.len() <= visible.0).unwrap_or(0);
            let Some(old) = self.chunks.remove(at) else { break };
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
        self.chunks.push_back(Chunk { first, rows, bytes });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_short_chunks_keep_their_absolute_offsets() {
        let mut cache = CollectionCache::default();
        cache.insert(300, vec!["first".into(), "second".into()], (300, 304));
        cache.insert(302, vec!["third".into()], (300, 304));
        assert_eq!(cache.get(302), Some("third"));
        assert_eq!(cache.first_missing(300, 304), Some(303));
        assert_eq!(cache.get(399), None);
    }
    #[test]
    fn scrolling_a_million_positions_keeps_only_eight_chunks() {
        let mut cache = CollectionCache::default();
        for first in (0..1_000_000).step_by(100) {
            cache.insert(first, vec!["row".into(); 100], (0, 10));
        }
        assert_eq!(cache.chunks.len(), MAX_CHUNKS);
        assert_eq!(cache.get(0), Some("row"));
        assert_eq!(cache.get(999_999), Some("row"));
        assert!(cache.bytes <= MAX_BYTES);
        cache.clear();
        assert_eq!((cache.bytes, cache.chunks.len()), (0, 0));
    }
    #[test]
    fn oversized_projection_does_not_displace_visible_rows() {
        let mut cache = CollectionCache::default();
        cache.insert(0, vec!["kept".into()], (0, 1));
        cache.insert(1, vec!["x".repeat(CHUNK_BYTES + 1)], (0, 1));
        assert_eq!(cache.get(0), Some("kept"));
        assert_eq!(cache.get(1), None);
    }
}
