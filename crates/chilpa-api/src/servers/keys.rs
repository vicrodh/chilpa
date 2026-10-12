//! The servers' credentials: sealed on disk, one per server, and held in
//! memory once opened so the player, the sync and the watcher do not ask the
//! keyring each time (ADR-017 §7).

use chilpa_secrets::SecretBox;
use chilpa_servers::Credential;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

pub(crate) struct Keys {
    root: PathBuf,
    /// Opened on a thread of its own. Reading the keyring can wait on a
    /// dialog -- a Mac asks again for every new build of the program -- and
    /// the window started the servers and froze until somebody answered it:
    /// 35 s on the first start of a build. Only a thread that needs a
    /// credential waits for it now.
    secrets: Arc<OnceLock<Option<SecretBox>>>,
    open: RwLock<HashMap<String, Credential>>,
}

impl Keys {
    /// `root` is `<data>/servers`: each server's credential goes in the
    /// folder of its instance.
    pub(crate) fn new(root: PathBuf) -> Self {
        let secrets = Arc::new(OnceLock::new());
        let (cell, dir) = (Arc::clone(&secrets), root.clone());
        let opened = std::thread::Builder::new()
            .name("chilpa-server-keys".into())
            .spawn(move || drop(cell.set(SecretBox::open("chilpa", &dir).ok())));
        if opened.is_err() {
            // No thread to wait on: opened here, rather than never.
            let _ = secrets.set(SecretBox::open("chilpa", &root).ok());
        }
        Self { root, secrets, open: RwLock::new(HashMap::new()) }
    }

    /// Keys sealed with a key derived from this machine only, for tests: the
    /// keyring is never touched.
    #[cfg(test)]
    pub(crate) fn derived(root: PathBuf) -> Self {
        let secrets = Arc::new(OnceLock::from(SecretBox::derived_only("chilpa", &root).ok()));
        Self { root, secrets, open: RwLock::new(HashMap::new()) }
    }

    /// The box, once it is open: this waits for the keyring.
    fn secrets(&self) -> Option<&SecretBox> {
        self.secrets.wait().as_ref()
    }

    /// An instance is sixteen hex digits, minted here: anything else never
    /// becomes a path.
    fn file(&self, instance: &str) -> Option<PathBuf> {
        (instance.len() == 16 && instance.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| self.root.join(instance).join("credential.sealed"))
    }

    /// The credential of that server, opening it the first time.
    pub(crate) fn get(&self, instance: &str) -> Option<Credential> {
        if let Some(key) = self.open.read().ok()?.get(instance) {
            return Some(key.clone());
        }
        let sealed = std::fs::read(self.file(instance)?).ok()?;
        let plain = self.secrets()?.open_sealed(&sealed).ok()?;
        let key: Credential = serde_json::from_slice(&plain).ok()?;
        self.open.write().ok()?.insert(instance.to_string(), key.clone());
        Some(key)
    }

    /// Seals and keeps it. `false` when it could not be kept, and then the
    /// server is not added: one that cannot sign in again after a restart is
    /// worse than none.
    pub(crate) fn put(&self, instance: &str, key: &Credential) -> bool {
        let (Some(file), Some(secrets)) = (self.file(instance), self.secrets()) else { return false };
        let Ok(plain) = serde_json::to_vec(key) else { return false };
        let Ok(sealed) = secrets.seal(&plain) else { return false };
        let written = file.parent().is_some_and(|dir| std::fs::create_dir_all(dir).is_ok()) && std::fs::write(&file, sealed).is_ok();
        if written {
            if let Ok(mut open) = self.open.write() {
                open.insert(instance.to_string(), key.clone());
            }
        }
        written
    }

    /// Forgets it, on disk and in memory.
    /// `false` when the sealed file is still on disk.
    pub(crate) fn forget(&self, instance: &str) -> bool {
        if let Ok(mut open) = self.open.write() {
            open.remove(instance);
        }
        let Some(dir) = self.file(instance).and_then(|file| file.parent().map(std::path::Path::to_path_buf)) else { return true };
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => true,
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        }
    }
}

/// A new instance id: sixteen hex digits nobody can guess.
pub(crate) fn mint() -> String {
    use rand::RngExt;
    let mut bytes = [0u8; 8];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_is_kept_sealed_and_read_back_after_a_restart() {
        let root = std::env::temp_dir().join(format!("chilpa-keys-{}", mint()));
        let instance = mint();
        let key = Credential::Password { user: "me".into(), password: "hunter2".into() };
        {
            let keys = Keys::derived(root.clone());
            assert!(keys.put(&instance, &key));
        }
        let on_disk = std::fs::read(root.join(&instance).join("credential.sealed")).unwrap();
        assert!(!String::from_utf8_lossy(&on_disk).contains("hunter2"), "never in the clear");
        let keys = Keys::derived(root.clone());
        assert_eq!(keys.get(&instance), Some(key));
        keys.forget(&instance);
        assert!(keys.get(&instance).is_none());
        assert!(!root.join(&instance).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_instance_that_is_not_one_of_mine_never_becomes_a_path() {
        let keys = Keys::new(std::env::temp_dir().join("chilpa-keys-never"));
        for bad in ["../../etc", "", "ABCDEF0123456789Z", "abc/def0123456789"] {
            assert!(keys.file(bad).is_none(), "{bad}");
        }
        assert_eq!(mint().len(), 16);
    }
}
