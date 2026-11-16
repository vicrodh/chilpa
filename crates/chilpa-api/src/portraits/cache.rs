//! At most 256 identity records and 1 MiB, shared by two workers. The atomic
//! disk index has the same bounds; downloaded rasters use the artwork budget.
use chilpa_meta::portrait::Portrait;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, io::{Read, Write}, sync::{Mutex, OnceLock}, time::{SystemTime, UNIX_EPOCH}};

const LIMIT: usize = 1024 * 1024;
#[derive(Deserialize, Serialize)]
struct Entry { key: String, expires: u64, info: Option<Portrait> }
impl Entry {
    fn available(&self, at: u64, offline: bool) -> bool { self.expires > at || offline && self.info.is_some() }
}
fn path() -> std::path::PathBuf { chilpa_core::artwork_cache().join("portrait-index.json") }
fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }
fn entries() -> &'static Mutex<VecDeque<Entry>> {
    static ENTRIES: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let read = || -> Option<VecDeque<Entry>> {
            let file = std::fs::File::open(path()).ok()?;
            if file.metadata().ok()?.len() > LIMIT as u64 { return None; }
            let mut bytes = Vec::new(); file.take(LIMIT as u64).read_to_end(&mut bytes).ok()?;
            let rows: VecDeque<Entry> = serde_json::from_slice(&bytes).ok()?;
            (rows.len() <= 256).then_some(rows)
        };
        Mutex::new(read().unwrap_or_default())
    })
}
pub(super) fn get(key: &str, offline: bool) -> Option<Option<Portrait>> {
    entries().lock().unwrap_or_else(|error| error.into_inner()).iter()
        .find(|entry| entry.key == key && entry.available(now(), offline)).map(|entry| entry.info.clone())
}
pub(super) fn put(key: String, info: Option<Portrait>, transient: bool) {
    let mut entries = entries().lock().unwrap_or_else(|error| error.into_inner());
    entries.retain(|entry| entry.key != key && (entry.info.is_some() || entry.expires > now()));
    let lifetime = if transient { 600 } else if info.is_some() { 30 * 86400 } else { 86400 };
    entries.push_back(Entry { key, info, expires:now().saturating_add(lifetime) });
    while entries.len() > 256 { entries.pop_front(); }
    let bytes = loop {
        let Ok(bytes) = serde_json::to_vec(&*entries) else { return; };
        if bytes.len() <= LIMIT { break bytes; }
        entries.pop_front();
    };
    let path = path();
    let save = || -> Option<()> {
        std::fs::create_dir_all(path.parent()?).ok()?;
        let mut file = tempfile::NamedTempFile::new_in(path.parent()?).ok()?;
        file.write_all(&bytes).ok()?; file.persist(path).ok()?; Some(())
    };
    let _ = save();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_identity_still_serves_offline_after_its_refresh_date() {
        let image = Entry { key:"artist".into(),expires:10,info:Some(Portrait::default()) };
        assert!(!image.available(20,false));
        assert!(image.available(20,true));
        let absent = Entry { key:"missing".into(),expires:10,info:None };
        assert!(!absent.available(20,true));
    }
}
