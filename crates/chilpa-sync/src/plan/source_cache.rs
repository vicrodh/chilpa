//! I retain at most one selection's source hashes per profile (50k entries, 16 MiB JSON).
//! A review owns the in-memory map; no worker, timer or process-global cache survives it.
use crate::{conflict, model::fingerprint, Cancellation, Fingerprint, Result, State, MAX_ASSETS, MAX_METADATA_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::{File, Metadata}, path::Path};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp { identity: [u64; 6], size: u64 }

#[cfg(unix)]
fn stamp(metadata: &Metadata) -> Option<Stamp> {
    use std::os::unix::fs::MetadataExt;
    if metadata.ino() == 0 || metadata.ctime_nsec() == 0 { return None; }
    Some(Stamp { identity:[metadata.dev(), metadata.ino(), metadata.mtime() as u64,
        metadata.mtime_nsec() as u64, metadata.ctime() as u64, metadata.ctime_nsec() as u64], size:metadata.len() })
}

#[cfg(not(unix))]
fn stamp(_metadata: &Metadata) -> Option<Stamp> {
    // Size and modification time alone cannot prove a cache hit. Until a platform exposes
    // both file identity and change time here, I hash its local sources on every review.
    None
}

#[derive(Clone, Serialize, Deserialize)]
struct Cached { stamp: Stamp, digest: Fingerprint }

// I stop before deserializing an extra value, not after allocating an oversized map.
struct Entries(BTreeMap<String, Cached>);
impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Bounded;
        impl<'de> serde::de::Visitor<'de> for Bounded {
            type Value = Entries;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a bounded source fingerprint map")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> std::result::Result<Entries, A::Error> {
                use serde::de::Error;
                let mut entries = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if entries.len() >= MAX_ASSETS { return Err(A::Error::custom("source cache entry budget exceeded")); }
                    if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) || entries.contains_key(&key) {
                        return Err(A::Error::custom("invalid or duplicate source cache key"));
                    }
                    entries.insert(key, map.next_value::<Cached>()?);
                }
                Ok(Entries(entries))
            }
        }
        deserializer.deserialize_map(Bounded)
    }
}

pub(super) struct Cache {
    old: BTreeMap<String, Cached>, next: BTreeMap<String, Cached>,
    encoded_bytes: usize, pub bytes_hashed: u64,
}
impl Cache {
    pub fn load(state: &State) -> Result<Self> {
        let old = state.source_cache()?.and_then(|raw| serde_json::from_str::<Entries>(&raw).ok())
            .map_or_else(BTreeMap::new, |entries| entries.0);
        Ok(Self { old, next:BTreeMap::new(), encoded_bytes:2, bytes_hashed:0 })
    }
    pub fn fingerprint(&mut self, path: &Path, cancel: &Cancellation, reuse: bool) -> Result<Fingerprint> {
        cancel.check()?;
        let mut file = File::open(path)?;
        let before = file.metadata()?;
        if !before.is_file() { return Err(conflict("sync source is not a regular file")); }
        let before_stamp = stamp(&before);
        let key = format!("{:x}", Sha256::digest(path.as_os_str().as_encoded_bytes()));
        let cached = self.old.remove(&key).filter(|cached| reuse && Some(&cached.stamp) == before_stamp.as_ref()
            && cached.digest.size == before.len() && cached.digest.sha256.len() == 64
            && cached.digest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let digest = if let Some(cached) = cached { cached.digest } else {
            let digest = fingerprint(&mut file, cancel)?;
            self.bytes_hashed = self.bytes_hashed.saturating_add(digest.size);
            digest
        };
        cancel.check()?;
        let after = file.metadata()?;
        let named = std::fs::metadata(path)?;
        if before.len() != digest.size || after.len() != digest.size || named.len() != digest.size
            || stamp(&after) != before_stamp || stamp(&named) != before_stamp
            || before.modified().ok() != after.modified().ok() || before.modified().ok() != named.modified().ok() {
            return Err(conflict("sync source changed while inspecting it"));
        }
        if let Some(stamp) = before_stamp {
            let cached = Cached { stamp, digest:digest.clone() };
            // The map stores only SHA-256 path keys, never unbounded native paths.
            let bytes = key.len() + serde_json::to_vec(&cached).expect("serializable source stamp").len() + 4;
            if self.next.len() < MAX_ASSETS && self.encoded_bytes.saturating_add(bytes) <= MAX_METADATA_BYTES {
                self.encoded_bytes += bytes;
                self.next.insert(key, cached);
            }
        }
        Ok(digest)
    }
    pub fn save(&self, state: &State) -> Result<()> {
        let raw = serde_json::to_string(&self.next).expect("serializable source cache");
        state.save_source_cache(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_cache_stops_before_deserializing_the_extra_value() {
        let record = serde_json::to_string(&Cached { stamp:Stamp { identity:[0; 6], size:0 },
            digest:Fingerprint { size:0, sha256:"0".repeat(64) } }).unwrap();
        let mut raw = String::from("{");
        for index in 0..MAX_ASSETS {
            if index != 0 { raw.push(','); }
            raw.push_str(&format!("\"{index:064x}\":{record}"));
        }
        // This value is deliberately not a Cached: the entry budget must fail first.
        raw.push_str(&format!(",\"{:064x}\":null}}", MAX_ASSETS));
        assert!(raw.len() < MAX_METADATA_BYTES);
        let error = serde_json::from_str::<Entries>(&raw).err().unwrap();
        assert!(error.to_string().contains("source cache entry budget exceeded"));
    }

    #[test]
    fn damaged_optional_cache_is_discarded_and_the_source_is_hashed_again() {
        let directory = tempfile::tempdir().unwrap();
        let state = State::open(&directory.path().join("sync.db")).unwrap();
        let source = directory.path().join("fixture.flac");
        std::fs::write(&source, b"original").unwrap();
        // A legacy-compatible six-element identity remains an array in JSON. Extra elements
        // are invalid and cannot allocate an unbounded identity vector during deserialization.
        let raw = format!("{{\"{}\":{{\"stamp\":{{\"identity\":[0,0,0,0,0,0,0],\"size\":8}},\"digest\":{{\"size\":8,\"sha256\":\"{}\"}}}}}}", "0".repeat(64), "0".repeat(64));
        state.save_source_cache(&raw).unwrap();
        let mut cache = Cache::load(&state).unwrap();
        assert!(cache.old.is_empty());
        let digest = cache.fingerprint(&source, &Cancellation::default(), true).unwrap();
        assert_eq!(digest.size, 8);
        assert_eq!(cache.bytes_hashed, 8);
    }
}
