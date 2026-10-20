//! I keep at most 6,000 ISRCs and 8 MiB of credits per catalog, without truncation.
//! The JSON read is capped at 8 MiB; writes stream into an atomic replacement.
use chilpa_meta::musicbrainz::{isrc_key, credit_bytes, COMPOSER_BYTES};
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
    composers: Vec<String>,
}

impl Entry {
    fn expires(&self) -> u64 { self.at.saturating_add(if self.composers.is_empty() { NONE } else { FOUND }) }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct Cache {
    entries: BTreeMap<String, Entry>,
    #[serde(skip)]
    bytes: usize,
}

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
        cache.entries.retain(|key, value| isrc_key(key).as_ref() == Some(key) && value.at <= now
            && value.expires() > now && credit_bytes(&value.composers) <= COMPOSER_BYTES
            && value.composers.iter().all(|name| !name.trim().is_empty()));
        cache.recount();
        while cache.entries.len() > ENTRIES || cache.bytes > BYTES as usize {
            if let Some((_, entry)) = cache.entries.pop_last() { cache.bytes -= 256 + credit_bytes(&entry.composers); }
        }
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

    pub fn get(&self, isrc: &str, now: u64) -> Option<&[String]> {
        self.entries.get(&isrc_key(isrc)?).filter(|e| e.at <= now && now < e.expires()).map(|e| e.composers.as_slice())
    }

    pub fn insert(&mut self, isrc: &str, composers: Vec<String>, now: u64) {
        let Some(key) = isrc_key(isrc) else { return; };
        if self.full() && !self.entries.contains_key(&key) { return; }
        if credit_bytes(&composers) > COMPOSER_BYTES || composers.iter().any(|s| s.trim().is_empty()) { return; }
        let old_bytes = self.entries.get(&key).map_or(0, |entry| 256 + credit_bytes(&entry.composers));
        if self.bytes - old_bytes + 256 + credit_bytes(&composers) > BYTES as usize { return; }
        if let Some(old) = self.entries.remove(&key) { self.bytes -= 256 + credit_bytes(&old.composers); }
        self.bytes += 256 + credit_bytes(&composers);
        self.entries.insert(key, Entry { at: now, composers });
    }

    pub fn full(&self) -> bool { self.entries.len() >= ENTRIES || self.bytes + COMPOSER_BYTES + 256 > BYTES as usize }
    fn recount(&mut self) { self.bytes = self.entries.values().map(|entry| 256 + credit_bytes(&entry.composers)).sum(); }
    pub fn retain(&mut self, isrcs: &std::collections::HashSet<String>) {
        self.entries.retain(|isrc, _| isrcs.contains(isrc));
        self.recount();
    }
    pub fn has_composers(&self) -> bool { self.entries.values().any(|entry| !entry.composers.is_empty()) }
    pub fn expiry(&self) -> u64 { self.entries.values().map(Entry::expires).min().unwrap_or(u64::MAX) }
    pub fn expire(&mut self, now: u64) -> bool {
        let changed = self.entries.values().any(|entry| !entry.composers.is_empty() && (entry.at > now || now >= entry.expires()));
        self.entries.retain(|_, entry| entry.at <= now && now < entry.expires());
        self.recount();
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_and_none_expire_after_ninety_and_thirty_days_and_fold_isrc_case() {
        let mut cache = Cache::default();
        cache.insert("usabc1200001", vec!["Composer".into()], 100);
        cache.insert("USABC1200002", vec![], 100);
        assert_eq!(cache.get("USABC1200001", 100 + FOUND - 1).unwrap(), ["Composer"]);
        assert!(cache.get("USABC1200001", 100 + FOUND).is_none());
        assert!(cache.get("usabc1200002", 100 + NONE - 1).unwrap().is_empty());
        assert!(cache.get("USABC1200002", 100 + NONE).is_none());
        assert!(cache.get("USABC1200001", 99).is_none());
        assert!(!cache.expire(100 + NONE));
        assert_eq!(cache.expiry(), 100 + FOUND);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("composers.json");
        cache.write(&path).unwrap();
        assert_eq!(Cache::read(&path, 101).get("usabc1200001", 101).unwrap(), ["Composer"]);
        assert!(Cache::read(&path, 100 + FOUND).entries.is_empty());
        assert!(cache.expire(100 + FOUND));
    }

    #[test]
    fn entry_and_byte_budgets_bound_the_cache_without_truncating_names() {
        let mut cache = Cache::default();
        for n in 0..ENTRIES + 10 { cache.insert(&format!("USABC{n:07}"), vec![], 1); }
        assert_eq!(cache.entries.len(), ENTRIES);
        assert!(cache.full());
        cache.retain(&["USABC0000001".to_string()].into_iter().collect());
        assert!(!cache.full());
        let names: Vec<_> = (0..100).map(|n| format!("Composer {n}")).collect();
        cache.insert("USABC9999999", names.clone(), 2);
        assert_eq!(cache.get("USABC9999999", 2).unwrap(), names);
        for n in 0..ENTRIES {
            cache.insert(&format!("USDEF{n:07}"), vec!["x".repeat(9_000)], 2);
        }
        assert!(cache.full());
        assert!(cache.entries.len() < ENTRIES);
        assert!(cache.bytes <= BYTES as usize);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("composers.json");
        cache.write(&path).unwrap();
        assert!(path.metadata().unwrap().len() <= BYTES);
        assert_eq!(Cache::read(&path, 2).entries.len(), cache.entries.len());
    }

    #[test]
    fn disk_reads_discard_invalid_expired_future_and_oversized_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("composers.json");
        std::fs::write(&path, serde_json::json!({
            "invalid":{"at":1,"composers":["Composer"]},
            "usabc1200001":{"at":1,"composers":["Composer"]},
            "USABC1200002":{"at":200,"composers":["Composer"]},
            "USABC1200003":{"at":1,"composers":[""]},
            "USABC1200004":{"at":1,"composers":["x".repeat(COMPOSER_BYTES)]}
        }).to_string()).unwrap();
        assert!(Cache::read(&path, 100).entries.is_empty());
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(BYTES + 1).unwrap();
        assert!(Cache::read(&path, 100).entries.is_empty());
    }
}
