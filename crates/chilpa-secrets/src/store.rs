//! Bounded, encrypted records whose names never become guest-controlled paths.

use crate::SecretBox;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_RECORD: usize = 1024 * 1024;
const MAX_SEALED: usize = MAX_RECORD + 2048;
const MAX_RECORDS: usize = 128;
const MAX_STORE: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    InvalidScope,
    TooLarge,
    Unavailable,
    Corrupt,
}

/// The host shares one instance among workers. Guests receive values through
/// the private channel, never the encryption key or a filesystem path.
pub struct PrivateStore {
    root: PathBuf,
    secrets: Arc<SecretBox>,
    writes: Mutex<()>,
}

impl PrivateStore {
    pub fn new(root: PathBuf, secrets: Arc<SecretBox>) -> Self {
        Self { root, secrets, writes: Mutex::new(()) }
    }

    pub fn get(&self, owner: &str, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let path = self.path(owner, key)?;
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(StoreError::Unavailable),
        };
        let mut sealed = Vec::new();
        file.take(MAX_SEALED as u64 + 1).read_to_end(&mut sealed)
            .map_err(|_| StoreError::Unavailable)?;
        if sealed.len() > MAX_SEALED { return Err(StoreError::TooLarge); }
        let plain = self.secrets.open_sealed(&sealed).map_err(|_| StoreError::Corrupt)?;
        let binding = binding(owner, key);
        let value = plain.strip_prefix(binding.as_slice()).ok_or(StoreError::Corrupt)?;
        if value.len() > MAX_RECORD { return Err(StoreError::TooLarge); }
        Ok(Some(value.to_vec()))
    }

    pub fn set(&self, owner: &str, key: &str, value: &[u8]) -> Result<(), StoreError> {
        let path = self.path(owner, key)?;
        if value.len() > MAX_RECORD { return Err(StoreError::TooLarge); }
        let _guard = self.writes.lock().map_err(|_| StoreError::Unavailable)?;
        let mut plain = binding(owner, key);
        plain.extend_from_slice(value);
        let sealed = self.secrets.seal(&plain).map_err(|_| StoreError::Unavailable)?;
        let directory = path.parent().ok_or(StoreError::InvalidScope)?;
        std::fs::create_dir_all(directory).map_err(|_| StoreError::Unavailable)?;
        self.check_quota(directory, &path, sealed.len() as u64)?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)
            .map_err(|_| StoreError::Unavailable)?;
        temporary.write_all(&sealed).map_err(|_| StoreError::Unavailable)?;
        temporary.as_file().sync_all().map_err(|_| StoreError::Unavailable)?;
        temporary.persist(path).map_err(|_| StoreError::Unavailable)?;
        Ok(())
    }

    pub fn remove_owner(&self, owner: &str) -> Result<(), StoreError> {
        validate(owner)?;
        let _guard = self.writes.lock().map_err(|_| StoreError::Unavailable)?;
        match std::fs::remove_dir_all(self.root.join(digest(owner))) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(StoreError::Unavailable),
        }
    }

    fn path(&self, owner: &str, key: &str) -> Result<PathBuf, StoreError> {
        validate(owner)?;
        validate(key)?;
        Ok(self.root.join(digest(owner)).join(format!("{}.sealed", digest(key))))
    }

    fn check_quota(&self, directory: &Path, replacing: &Path, size: u64) -> Result<(), StoreError> {
        let mut count = 1;
        let mut bytes = size;
        for entry in std::fs::read_dir(directory).map_err(|_| StoreError::Unavailable)? {
            let entry = entry.map_err(|_| StoreError::Unavailable)?;
            if entry.path() == replacing { continue; }
            count += 1;
            bytes = bytes.saturating_add(entry.metadata().map_err(|_| StoreError::Unavailable)?.len());
        }
        if count > MAX_RECORDS || bytes > MAX_STORE { return Err(StoreError::TooLarge); }
        Ok(())
    }
}

fn validate(value: &str) -> Result<(), StoreError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(StoreError::InvalidScope);
    }
    Ok(())
}

fn digest(value: &str) -> String { format!("{:x}", Sha256::digest(value.as_bytes())) }

fn binding(owner: &str, key: &str) -> Vec<u8> {
    let mut bytes = b"chilpa-private-record-v1".to_vec();
    for text in [owner, key] {
        bytes.extend_from_slice(&(text.len() as u32).to_be_bytes());
        bytes.extend_from_slice(text.as_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(root: &Path) -> PrivateStore {
        let secrets = Arc::new(SecretBox::derived_only("private-store-test", root).unwrap());
        PrivateStore::new(root.join("records"), secrets)
    }

    #[test]
    fn records_survive_reopening_without_plaintext_on_disk() {
        let root = tempfile::tempdir().unwrap();
        let first = store(root.path());
        first.set("plugin-one", "plugin.preferences", b"private token").unwrap();
        let sealed = std::fs::read(first.path("plugin-one", "plugin.preferences").unwrap()).unwrap();
        assert!(!sealed.windows(13).any(|part| part == b"private token"));
        assert_eq!(store(root.path()).get("plugin-one", "plugin.preferences").unwrap(), Some(b"private token".to_vec()));
        first.set("plugin-one", "plugin.preferences", b"renewed").unwrap();
        assert_eq!(first.get("plugin-one", "plugin.preferences").unwrap(), Some(b"renewed".to_vec()));
    }

    #[test]
    fn copying_a_sealed_record_cannot_change_its_owner_or_key() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        store.set("one", "account", b"token").unwrap();
        store.set("two", "account", b"other").unwrap();
        let source = store.path("one", "account").unwrap();
        for (owner, key) in [("two", "account"), ("one", "cookies")] {
            std::fs::copy(&source, store.path(owner, key).unwrap()).unwrap();
            assert_eq!(store.get(owner, key), Err(StoreError::Corrupt));
        }
    }

    #[test]
    fn failures_do_not_erase_an_existing_record() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        store.set("one", "account", b"token").unwrap();
        assert_eq!(store.set("one", "account", &vec![0; MAX_RECORD + 1]), Err(StoreError::TooLarge));
        assert_eq!(store.get("one", "account").unwrap(), Some(b"token".to_vec()));
        store.set("two", "account", b"other").unwrap();
        store.remove_owner("one").unwrap();
        assert_eq!(store.get("one", "account").unwrap(), None);
        assert_eq!(store.get("two", "account").unwrap(), Some(b"other".to_vec()));
    }
}
