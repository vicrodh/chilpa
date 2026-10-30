//! The servers' credentials: sealed on disk, one per server, and held in
//! memory once opened so the player, the sync and the watcher do not ask the
//! keyring each time (ADR-017 §7).
//!
//! When one cannot be opened or kept, the step that failed goes to stderr
//! with the instance and the backend in use, never what is sealed nor a
//! path: a restart that asks for the sign-in again was mute otherwise. The
//! lines avoid the words the session log blanks a whole line for
//! ("secret", "credential", "password", "token"), or the export would carry
//! nothing of them.

use chilpa_secrets::{BackendKind, SecretBox};
use chilpa_servers::Credential;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

pub(crate) struct Keys {
    root: PathBuf,
    /// Opened on a thread of its own. Reading the keyring can wait on a
    /// dialog -- a Mac asks again for every new build of the program -- and
    /// the window started the servers and froze until somebody answered it:
    /// 35 s on the first start of a build. Only a thread that needs a
    /// credential waits for it now.
    secrets: Arc<OnceLock<Option<SecretBox>>>,
    open: RwLock<HashMap<String, Credential>>,
    /// The last step each instance failed at, so the watcher's minute does
    /// not repeat it.
    reported: Mutex<HashMap<String, String>>,
}

impl Keys {
    /// `root` is `<data>/servers`: each server's credential goes in the
    /// folder of its instance.
    pub(crate) fn new(root: PathBuf) -> Self {
        let secrets = Arc::new(OnceLock::new());
        let (cell, dir) = (Arc::clone(&secrets), root.clone());
        let opened = std::thread::Builder::new()
            .name("chilpa-server-keys".into())
            .spawn(move || drop(cell.set(open_box(&dir))));
        if opened.is_err() {
            // No thread to wait on: opened here, rather than never.
            let _ = secrets.set(open_box(&root));
        }
        Self { root, secrets, open: RwLock::new(HashMap::new()), reported: Mutex::new(HashMap::new()) }
    }

    /// Keys sealed with a key derived from this machine only, for tests: the
    /// keyring is never touched.
    #[cfg(test)]
    pub(crate) fn derived(root: PathBuf) -> Self {
        let secrets = Arc::new(OnceLock::from(SecretBox::derived_only("chilpa", &root).ok()));
        Self { root, secrets, open: RwLock::new(HashMap::new()), reported: Mutex::new(HashMap::new()) }
    }

    /// The box, once it is open: this waits for the keyring.
    fn secrets(&self) -> Option<&SecretBox> {
        self.secrets.wait().as_ref()
    }

    /// The box's backend for a log line, without waiting for it.
    fn backend(&self) -> &'static str {
        match self.secrets.get() {
            None => "opening",
            Some(None) => "none",
            Some(Some(secrets)) => backend_name(secrets.backend()),
        }
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
        match self.read(instance) {
            Ok(key) => {
                self.open.write().ok()?.insert(instance.to_string(), key.clone());
                if let Ok(mut reported) = self.reported.lock() {
                    reported.remove(instance);
                }
                Some(key)
            }
            Err(step) => {
                self.report(instance, step);
                None
            }
        }
    }

    /// The sealed credential, opened; otherwise the step that failed. The
    /// file is read before waiting for the box, as it always was.
    fn read(&self, instance: &str) -> Result<Credential, &'static str> {
        let file = self.file(instance).ok_or("bad-instance")?;
        let sealed = std::fs::read(&file).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "no-file",
            _ => "unreadable-file",
        })?;
        let plain = self.secrets().ok_or("no-box")?.open_sealed(&sealed).map_err(|e| e.kind())?;
        serde_json::from_slice(&plain).map_err(|_| "bad-json")
    }

    /// Says which step failed, once per step: the watcher asks again every
    /// minute.
    fn report(&self, instance: &str, step: &str) {
        let Ok(mut reported) = self.reported.lock() else { return };
        if reported.get(instance).is_some_and(|last| last == step) {
            return;
        }
        reported.insert(instance.to_string(), step.to_string());
        eprintln!("chilpa-servers: server {instance} not opened: step={step} backend={}", self.backend());
    }

    /// Seals and keeps it. `false` when it could not be kept, and then the
    /// server is not added: one that cannot sign in again after a restart is
    /// worse than none.
    pub(crate) fn put(&self, instance: &str, key: &Credential) -> bool {
        match self.write(instance, key) {
            Ok(()) => {
                if let Ok(mut open) = self.open.write() {
                    open.insert(instance.to_string(), key.clone());
                }
                true
            }
            Err(step) => {
                eprintln!("chilpa-servers: server {instance} not kept: step={step} backend={}", self.backend());
                false
            }
        }
    }

    /// Seals and writes it; otherwise the step that failed.
    fn write(&self, instance: &str, key: &Credential) -> Result<(), &'static str> {
        let file = self.file(instance).ok_or("bad-instance")?;
        let secrets = self.secrets().ok_or("no-box")?;
        let plain = serde_json::to_vec(key).map_err(|_| "serialize")?;
        let sealed = secrets.seal(&plain).map_err(|e| e.kind())?;
        let dir = file.parent().ok_or("bad-instance")?;
        std::fs::create_dir_all(dir).map_err(|_| "unwritable-folder")?;
        std::fs::write(&file, sealed).map_err(|_| "unwritable-file")
    }

    /// Forgets it, on disk and in memory.
    /// `false` when the sealed file is still on disk.
    pub(crate) fn forget(&self, instance: &str) -> bool {
        if let Ok(mut open) = self.open.write() {
            open.remove(instance);
        }
        let Some(dir) = self.file(instance).and_then(|file| file.parent().map(Path::to_path_buf)) else { return true };
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => true,
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        }
    }
}

/// Opens the box and says which backend it got, or why there is none.
fn open_box(dir: &Path) -> Option<SecretBox> {
    match SecretBox::open("chilpa", dir) {
        Ok(secrets) => {
            eprintln!("chilpa-servers: keys box open, backend={}", backend_name(secrets.backend()));
            Some(secrets)
        }
        Err(e) => {
            eprintln!("chilpa-servers: no keys box: step={} ({e})", e.kind());
            None
        }
    }
}

fn backend_name(kind: BackendKind) -> &'static str {
    match kind {
        BackendKind::Keyring => "keyring",
        BackendKind::Derived => "derived",
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
    fn what_cannot_be_opened_says_which_step_failed() {
        let root = std::env::temp_dir().join(format!("chilpa-keys-{}", mint()));
        let keys = Keys::derived(root.clone());
        let instance = mint();
        assert_eq!(keys.read(&instance).unwrap_err(), "no-file");
        assert_eq!(keys.read("not-an-instance").unwrap_err(), "bad-instance");
        std::fs::create_dir_all(root.join(&instance)).unwrap();
        std::fs::write(root.join(&instance).join("credential.sealed"), b"not an envelope").unwrap();
        assert_eq!(keys.read(&instance).unwrap_err(), "malformed");
        let sealed = SecretBox::derived_only("chilpa", &root).unwrap().seal(b"not json").unwrap();
        std::fs::write(root.join(&instance).join("credential.sealed"), sealed).unwrap();
        assert_eq!(keys.read(&instance).unwrap_err(), "bad-json");
        assert_eq!(keys.backend(), "derived");
        assert!(keys.get(&instance).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_instance_that_is_not_one_of_mine_never_becomes_a_path() {
        // Derived, so the test never reaches the system keyring.
        let keys = Keys::derived(std::env::temp_dir().join("chilpa-keys-never"));
        for bad in ["../../etc", "", "ABCDEF0123456789Z", "abc/def0123456789"] {
            assert!(keys.file(bad).is_none(), "{bad}");
        }
        assert_eq!(mint().len(), 16);
    }
}
