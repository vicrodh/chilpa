//! I keep at most 6,000 barcodes, five names of 128 bytes each, per catalog.
//! The JSON read is capped at 8 MiB; writes stream into an atomic replacement.
use chilpa_meta::musicbrainz::barcode_key;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::Path};

pub(super) const ENTRIES: usize = 6_000;
const BYTES: u64 = 8 * 1024 * 1024;
const DAY: u64 = 86_400;
const FOUND: u64 = 90 * DAY;
const NONE: u64 = 30 * DAY;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Entry {
    at: u64,
    genres: Vec<String>,
}

impl Entry {
    fn expires(&self) -> u64 { self.at.saturating_add(if self.genres.is_empty() { NONE } else { FOUND }) }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct Cache(BTreeMap<String, Entry>);

impl Cache {
    pub fn read(path: &Path, now: u64) -> Self {
        let read = || -> Option<Self> {
            let file = std::fs::File::open(path).ok()?;
            if file.metadata().ok()?.len() > BYTES { return None; }
            let mut bytes = Vec::new();
            file.take(BYTES + 1).read_to_end(&mut bytes).ok()?;
            if bytes.len() as u64 > BYTES { return None; }
            serde_json::from_slice(&bytes).ok()
        };
        let mut cache = read().unwrap_or_default();
        cache.0.retain(|key, value| barcode_key(key).as_ref() == Some(key) && value.at <= now
            && value.expires() > now && value.genres.len() <= 5
            && value.genres.iter().all(|name| !name.trim().is_empty() && name.len() <= 128));
        while cache.0.len() > ENTRIES { cache.0.pop_last(); }
        cache
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        let directory = path.parent().ok_or_else(|| std::io::Error::other("No cache directory"))?;
        std::fs::create_dir_all(directory)?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        {
            let mut writer = std::io::BufWriter::new(file.as_file_mut());
            serde_json::to_writer(&mut writer, self).map_err(std::io::Error::other)?;
            writer.flush()?;
        }
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    pub fn get(&self, barcode: &str, now: u64) -> Option<&[String]> {
        self.0.get(&barcode_key(barcode)?).filter(|e| e.at <= now && now < e.expires()).map(|e| e.genres.as_slice())
    }

    pub fn insert(&mut self, barcode: &str, genres: Vec<String>, now: u64) {
        let Some(key) = barcode_key(barcode) else { return; };
        if self.full() && !self.0.contains_key(&key) { return; }
        let genres = genres.into_iter().filter(|s| !s.trim().is_empty() && s.len() <= 128).take(5).collect();
        self.0.insert(key, Entry { at: now, genres });
    }

    pub fn full(&self) -> bool { self.0.len() >= ENTRIES }
    pub fn retain(&mut self, barcodes: &std::collections::HashSet<String>) {
        self.0.retain(|barcode, _| barcodes.contains(barcode));
    }
    pub fn has_genres(&self) -> bool { self.0.values().any(|entry| !entry.genres.is_empty()) }
    pub fn expiry(&self) -> u64 { self.0.values().map(Entry::expires).min().unwrap_or(u64::MAX) }
    pub fn expire(&mut self, now: u64) -> bool {
        let changed = self.0.values().any(|entry| !entry.genres.is_empty() && (entry.at > now || now >= entry.expires()));
        self.0.retain(|_, entry| entry.at <= now && now < entry.expires());
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_and_none_have_separate_ttls_and_share_upc_ean_keys() {
        let mut cache = Cache::default();
        cache.insert("0123456789012", vec!["Jazz".into()], 100);
        cache.insert("223456789012", vec![], 100);
        assert_eq!(cache.get("123456789012", 100 + FOUND - 1).unwrap(), ["Jazz"]);
        assert!(cache.get("123456789012", 100 + FOUND).is_none());
        assert_eq!(cache.get("223456789012", 100 + NONE - 1).unwrap().len(), 0);
        assert!(cache.get("223456789012", 100 + NONE).is_none());
        assert!(cache.get("123456789012", 99).is_none());
        assert!(!cache.expire(100 + NONE));
        assert_eq!(cache.expiry(), 100 + FOUND);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("genres.json");
        cache.write(&path).unwrap();
        assert_eq!(Cache::read(&path, 101).get("123456789012", 101).unwrap(), ["Jazz"]);
        assert!(Cache::read(&path, 100 + FOUND).0.is_empty());
    }

    #[test]
    fn the_cache_has_a_fixed_entry_budget() {
        let mut cache = Cache::default();
        for n in 0..ENTRIES + 10 { cache.insert(&format!("{n:012}"), vec![], 1); }
        assert_eq!(cache.0.len(), ENTRIES);
        assert!(cache.full());
        cache.retain(&["000000000001".to_string()].into_iter().collect());
        cache.insert("999999999999", vec!["Jazz".into()], 2);
        assert!(cache.get("000000000001", 2).is_some());
        assert_eq!(cache.get("999999999999", 2).unwrap(), ["Jazz"]);
    }
}
