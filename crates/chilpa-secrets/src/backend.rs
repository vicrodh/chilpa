//! Which of the two paths is used, and why.
//!
//! Opening tries the system keyring first. If that works, a 32-byte master
//! key lives there and everything is encrypted with it. If it does not, for
//! whatever reason, the key is derived with HKDF from device identifiers.
//!
//! **Only Linux, macOS and Windows consult the keyring.** That is a
//! correctness requirement, not a preference: without the native feature,
//! `keyring` 3.x resolves to an in-memory `mock` **whose writes SUCCEED**.
//! That call does not fall through to the fallback; it hands back a key
//! that dies with the process, and everything sealed in one run is
//! unreadable in the next while the backend reports itself as "keyring".
//! The derived path, by contrast, comes from the same install id every
//! start.
//!
//! The backend marker travels inside every envelope, so what one sealed is
//! not silently opened by the other, and so that if someone gains keyring
//! access later, their old data is not ignored without a word.

use std::path::Path;

use hkdf::Hkdf;
// Only the keyring path mints a key; the other one derives it.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use rand::RngExt;
use sha2::Sha256;

use crate::cipher::{open_with_key, seal_with_key};
use crate::error::SecretError;
use crate::install_id;

const MASTER_KEY_LEN: usize = 32;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const KEYRING_ENTRY: &str = "master-key-v1";
const HKDF_INFO: &[u8] = b"chilpa-secrets master-key derivation v1";

const MARKER_KEYRING: u8 = 0;
const MARKER_DERIVED: u8 = 1;

/// Which path ended up active. Used to tell the user: having the system
/// keyring is not the same as not having it, and hiding that would promise
/// a security that is not there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// The master key lives in the system store. Copying the files to another
    /// machine leaves them unreadable.
    Keyring,
    /// The key is derived from `machine-id` plus the install id. Still bound
    /// to the machine, but anyone with filesystem access and the `machine-id`
    /// can rebuild it.
    Derived,
}

impl BackendKind {
    /// How it is told to the user, who has no reason to know what HKDF is.
    pub fn describe(&self) -> String {
        match self {
            BackendKind::Keyring => chilpa_i18n::t("in the system keyring"),
            BackendKind::Derived => chilpa_i18n::t(
                "encrypted and tied to this machine (there is no system keyring)",
            ),
        }
    }
}

pub struct Backend {
    kind: BackendKind,
    master_key: [u8; MASTER_KEY_LEN],
}

impl Backend {
    pub fn new(service_name: &str, storage_dir: &Path) -> Result<Self, SecretError> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        if let Ok(master_key) = try_keyring(service_name) {
            return Ok(Self { kind: BackendKind::Keyring, master_key });
        }

        let master_key = derive_key(service_name, storage_dir)?;
        Ok(Self { kind: BackendKind::Derived, master_key })
    }

    /// The derived path only, without touching the keyring.
    ///
    /// It exists for the tests. Reaching the real Keychain from a test is not
    /// stronger, it is flakier: macOS binds an entry's permission to the exact
    /// binary that created it, so the **second** `cargo test` after editing
    /// code is a new binary against an old entry, and the system blocks the
    /// process with a dialog a headless run can never answer. What is being
    /// tested — the round trip, and catching tampering — is identical on either
    /// path.
    pub fn derived_only(service_name: &str, storage_dir: &Path) -> Result<Self, SecretError> {
        let master_key = derive_key(service_name, storage_dir)?;
        Ok(Self { kind: BackendKind::Derived, master_key })
    }

    /// The keyring path with the entry given: the tests' keyring-backed box,
    /// which never reaches the system store.
    #[cfg(all(test, any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    pub(crate) fn with_entry(entry: &dyn MasterKeyEntry) -> Result<Self, SecretError> {
        Ok(Self { kind: BackendKind::Keyring, master_key: master_key_from(entry)? })
    }

    pub fn kind(&self) -> BackendKind {
        self.kind
    }

    fn marker(&self) -> u8 {
        match self.kind {
            BackendKind::Keyring => MARKER_KEYRING,
            BackendKind::Derived => MARKER_DERIVED,
        }
    }

    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
        seal_with_key(&self.master_key, self.marker(), plaintext)
    }

    pub fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, SecretError> {
        open_with_key(&self.master_key, self.marker(), sealed)
    }
}

/// Reads, or creates and stores, the master key in the system store.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn try_keyring(service_name: &str) -> Result<[u8; MASTER_KEY_LEN], SecretError> {
    use keyring::credential::CredentialPersistence;
    use keyring::Entry;

    // A build without its platform's feature exports the in-memory mock,
    // whose writes succeed and whose contents die with the process. That is
    // worse than the derived fallback, which is at least stable, and it does
    // not show because nothing fails. Any backend that does not write to
    // disk is refused: the real ones inherit `UntilDelete` and the mock
    // `EntryOnly`.
    //
    // Reach: it reads the compile-time builder, which is what `Entry::new`
    // uses here. It would not catch someone swapping it at runtime with
    // `keyring::set_default_credential_builder`; nobody does, and what this
    // guards against is a build configuration mistake.
    let persistence = keyring::default::default_credential_builder().persistence();
    if !matches!(persistence, CredentialPersistence::UntilDelete) {
        return Err(SecretError::Keyring(
            "the keyring backend does not write to disk (a mock?); it is not used".into(),
        ));
    }

    // Building the entry touches nothing: it is attributes and a label. The
    // store is first reached by `read`.
    let entry = Entry::new(service_name, KEYRING_ENTRY)
        .map_err(|e| SecretError::Keyring(format!("Entry::new: {e}")))?;
    master_key_from(&SystemEntry(entry))
}

/// The one entry the keyring path uses: the master key, kept as text.
///
/// A seam, so the tests put an in-memory store where the system one goes
/// and never reach the real keyring.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) trait MasterKeyEntry {
    /// `Ok(None)` when there is no entry yet.
    fn read(&self) -> Result<Option<String>, String>;
    fn write(&self, value: &str) -> Result<(), String>;
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
struct SystemEntry(keyring::Entry);

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl MasterKeyEntry for SystemEntry {
    fn read(&self) -> Result<Option<String>, String> {
        match self.0.get_password() {
            Ok(existing) => Ok(Some(existing)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("get_password: {e}")),
        }
    }

    fn write(&self, value: &str) -> Result<(), String> {
        self.0.set_password(value).map_err(|e| format!("set_password: {e}"))
    }
}

/// The key the entry holds, or a new one minted and stored there.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn master_key_from(
    entry: &dyn MasterKeyEntry,
) -> Result<[u8; MASTER_KEY_LEN], SecretError> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;

    match entry.read().map_err(SecretError::Keyring)? {
        Some(existing) => decode_key(&existing),
        None => {
            let mut key = [0u8; MASTER_KEY_LEN];
            rand::rng().fill(&mut key);
            entry.write(&B64.encode(key)).map_err(SecretError::Keyring)?;
            Ok(key)
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn decode_key(text: &str) -> Result<[u8; MASTER_KEY_LEN], SecretError> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;

    let bytes = B64.decode(text.trim()).map_err(|e| SecretError::Keyring(format!("base64: {e}")))?;
    if bytes.len() != MASTER_KEY_LEN {
        return Err(SecretError::Keyring(format!(
            "the entry is {} bytes and should be {MASTER_KEY_LEN}",
            bytes.len()
        )));
    }
    let mut out = [0u8; MASTER_KEY_LEN];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn derive_key(
    service_name: &str,
    storage_dir: &Path,
) -> Result<[u8; MASTER_KEY_LEN], SecretError> {
    let install_uuid = install_id::load_or_create(storage_dir).map_err(SecretError::Io)?;
    let machine = install_id::machine_id().unwrap_or_default();

    // None of the three is a secret. The strength is that all three have to
    // be on the same filesystem to rebuild the key.
    let mut ikm: Vec<u8> = Vec::with_capacity(service_name.len() + machine.len() + 64);
    ikm.extend_from_slice(service_name.as_bytes());
    ikm.push(0);
    ikm.extend_from_slice(machine.as_bytes());
    ikm.push(0);
    ikm.extend_from_slice(install_uuid.as_bytes());

    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut out = [0u8; MASTER_KEY_LEN];
    hk.expand(HKDF_INFO, &mut out).map_err(|e| SecretError::Other(format!("HKDF: {e}")))?;
    Ok(out)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    /// How long a reader is held for the others. A resolution that runs
    /// alone reads alone, waits this out and goes on.
    const HOLD: Duration = Duration::from_millis(300);

    /// A store that lets every reader in before any of them writes: a fresh
    /// keyring with the startup threads at its door at the same time.
    struct Contended {
        value: Mutex<Option<String>>,
        arrived: Mutex<usize>,
        everyone: Condvar,
        parties: usize,
    }

    impl Contended {
        fn new(parties: usize) -> Self {
            Self {
                value: Mutex::new(None),
                arrived: Mutex::new(0),
                everyone: Condvar::new(),
                parties,
            }
        }

        fn stored(&self) -> Option<String> {
            self.value.lock().unwrap().clone()
        }
    }

    impl MasterKeyEntry for Contended {
        fn read(&self) -> Result<Option<String>, String> {
            let mut arrived = self.arrived.lock().unwrap();
            *arrived += 1;
            self.everyone.notify_all();
            let (arrived, _) = self
                .everyone
                .wait_timeout_while(arrived, HOLD, |arrived| *arrived < self.parties)
                .unwrap();
            drop(arrived);
            Ok(self.stored())
        }

        fn write(&self, value: &str) -> Result<(), String> {
            *self.value.lock().unwrap() = Some(value.to_string());
            Ok(())
        }
    }

    /// A store where another process stores its own key right after ours:
    /// what it holds afterwards is not what this process wrote.
    struct Overtaken {
        value: Mutex<Option<String>>,
        other: String,
    }

    impl MasterKeyEntry for Overtaken {
        fn read(&self) -> Result<Option<String>, String> {
            Ok(self.value.lock().unwrap().clone())
        }

        fn write(&self, _value: &str) -> Result<(), String> {
            *self.value.lock().unwrap() = Some(self.other.clone());
            Ok(())
        }
    }

    #[test]
    fn boxes_opened_at_once_on_a_fresh_keyring_share_the_key_it_keeps() {
        // The first start of an install: no entry yet, and the plugins, the
        // scrobble queue and the servers each open a box on a thread of
        // their own. Whatever they seal must open on the next start, so all
        // of them have to end up with the key the keyring kept.
        let entry = Arc::new(Contended::new(4));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let entry = Arc::clone(&entry);
                std::thread::spawn(move || Backend::with_entry(&*entry).unwrap().master_key)
            })
            .collect();
        let keys: Vec<[u8; MASTER_KEY_LEN]> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        let stored = decode_key(&entry.stored().expect("a key was stored")).unwrap();
        for key in &keys {
            assert_eq!(*key, stored, "every box seals with the key the keyring keeps");
        }
    }

    #[test]
    fn the_key_in_use_is_the_one_the_keyring_holds_after_the_write() {
        let entry = Overtaken { value: Mutex::new(None), other: B64.encode([7u8; MASTER_KEY_LEN]) };
        let backend = Backend::with_entry(&entry).unwrap();
        assert_eq!(
            backend.master_key,
            [7u8; MASTER_KEY_LEN],
            "a key the keyring did not keep seals nothing worth keeping"
        );
    }
}
