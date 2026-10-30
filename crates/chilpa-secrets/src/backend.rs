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
//! The backend marker travels inside every envelope. A box on the keyring
//! opens what the derived path sealed with the derived key, which this
//! install can always rebuild: a start that found no keyring costs nothing on
//! the next one. A box without the keyring refuses what the keyring sealed,
//! by name, because it has no keyring key to open it with.
//!
//! The keyring is asked once per process and per service name
//! (`MasterKeys`): the boxes that open at startup, each on its own thread,
//! share one resolution instead of racing each other to mint the first key.
//! When that resolution refuses the keyring, it says why on stderr, once.

use std::path::{Path, PathBuf};

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
    /// What the derived key is rebuilt from, for an envelope the derived
    /// path sealed while this box is on the keyring.
    service_name: String,
    storage_dir: PathBuf,
}

impl Backend {
    pub fn new(service_name: &str, storage_dir: &Path) -> Result<Self, SecretError> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        if let Ok(master_key) = try_keyring(service_name) {
            return Ok(Self::with_key(BackendKind::Keyring, master_key, service_name, storage_dir));
        }

        let master_key = derive_key(service_name, storage_dir)?;
        Ok(Self::with_key(BackendKind::Derived, master_key, service_name, storage_dir))
    }

    fn with_key(
        kind: BackendKind,
        master_key: [u8; MASTER_KEY_LEN],
        service_name: &str,
        storage_dir: &Path,
    ) -> Self {
        Self {
            kind,
            master_key,
            service_name: service_name.to_string(),
            storage_dir: storage_dir.to_path_buf(),
        }
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
        Ok(Self::with_key(BackendKind::Derived, master_key, service_name, storage_dir))
    }

    /// The keyring path with the entry given: the tests' keyring-backed box,
    /// which never reaches the system store. The resolution is the shared
    /// one, so each test names a service of its own.
    #[cfg(all(test, any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    pub(crate) fn with_entry(
        service_name: &str,
        storage_dir: &Path,
        entry: &dyn MasterKeyEntry,
    ) -> Result<Self, SecretError> {
        let master_key = SHARED.resolve(service_name, || master_key_from(entry))?;
        Ok(Self::with_key(BackendKind::Keyring, master_key, service_name, storage_dir))
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

    /// Opens with the primary key. On the keyring, an envelope the derived
    /// path sealed opens with the derived key: a start without the keyring
    /// sealed it, and losing it would ask the user for a sign-in again. The
    /// install id is read, never created; without it nothing here was ever
    /// derived, and the envelope is refused as another backend's.
    pub fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, SecretError> {
        match open_with_key(&self.master_key, self.marker(), sealed) {
            Err(SecretError::BackendMismatch) if self.kind == BackendKind::Keyring => {
                let key = existing_derived_key(&self.service_name, &self.storage_dir)
                    .ok_or(SecretError::BackendMismatch)?;
                open_with_key(&key, MARKER_DERIVED, sealed)
            }
            opened => opened,
        }
    }
}

/// Reads, or creates and stores, the master key in the system store.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn try_keyring(service_name: &str) -> Result<[u8; MASTER_KEY_LEN], SecretError> {
    use keyring::credential::CredentialPersistence;
    use keyring::Entry;

    SHARED.resolve(service_name, || {
        // A build without its platform's feature exports the in-memory mock,
        // whose writes succeed and whose contents die with the process. That
        // is worse than the derived fallback, which is at least stable, and
        // it does not show because nothing fails. Any backend that does not
        // write to disk is refused: the real ones inherit `UntilDelete` and
        // the mock `EntryOnly`.
        //
        // Reach: it reads the compile-time builder, which is what
        // `Entry::new` uses here. It would not catch someone swapping it at
        // runtime with `keyring::set_default_credential_builder`; nobody
        // does, and what this guards against is a build configuration
        // mistake.
        let persistence = keyring::default::default_credential_builder().persistence();
        if !matches!(persistence, CredentialPersistence::UntilDelete) {
            return Err(Refusal::new("the store keeps nothing on disk (a mock?)", ""));
        }

        // Building the entry touches nothing: it is attributes and a label.
        // The store is first reached by `read`.
        let entry = Entry::new(service_name, KEYRING_ENTRY)
            .map_err(|e| Refusal::new("the entry could not be named", e))?;
        master_key_from(&SystemEntry(entry))
    })
}

/// One master key per service name for the whole process.
///
/// Several boxes open at startup, each on a thread of its own. On a
/// keyring with no entry yet, each of them would read nothing, mint a key
/// and store it; the last write would win, and the others would seal with
/// a key the keyring never kept. Here the first resolution runs alone and
/// every box after it takes the same outcome, the failure included: a
/// process either has the keyring or it does not.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) struct MasterKeys {
    resolved: std::sync::Mutex<Vec<(String, Result<[u8; MASTER_KEY_LEN], Refusal>)>>,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
static SHARED: MasterKeys = MasterKeys::new();

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl MasterKeys {
    pub(crate) const fn new() -> Self {
        Self { resolved: std::sync::Mutex::new(Vec::new()) }
    }

    /// The key for `service_name`: resolved once, under a lock the whole
    /// way through, and shared after that. A refusal is said when it
    /// happens, so a log tells which path every later box of the process
    /// is on and why.
    pub(crate) fn resolve(
        &self,
        service_name: &str,
        resolve: impl FnOnce() -> Result<[u8; MASTER_KEY_LEN], Refusal>,
    ) -> Result<[u8; MASTER_KEY_LEN], SecretError> {
        let mut resolved = self.resolved.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, outcome)) = resolved.iter().find(|(name, _)| name == service_name) {
            return outcome.clone().map_err(Refusal::into_error);
        }
        let outcome = resolve();
        if let Err(refusal) = &outcome {
            refusal.say(service_name);
        }
        resolved.push((service_name.to_string(), outcome.clone()));
        outcome.map_err(Refusal::into_error)
    }
}

/// Why the keyring is not used.
///
/// `what` is in words the session log keeps: its redaction blanks any line
/// that says "secret", "password" or "credential", and the stores name
/// themselves and their errors after those. `detail` is the store's own
/// message, for a terminal.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[derive(Clone, Debug)]
pub(crate) struct Refusal {
    what: String,
    detail: String,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl Refusal {
    fn new(what: impl Into<String>, detail: impl std::fmt::Display) -> Self {
        Self { what: what.into(), detail: detail.to_string() }
    }

    /// What the store answered to `step`, sorted into a few plain words.
    fn from_store(step: &str, e: keyring::Error) -> Self {
        let what = match &e {
            keyring::Error::NoStorageAccess(_) => "no access to the store (locked or refused)".to_string(),
            keyring::Error::PlatformFailure(_) => "the store failed or is not running".to_string(),
            keyring::Error::Ambiguous(items) => format!("{} entries match", items.len()),
            keyring::Error::BadEncoding(_) => "the entry is not text".to_string(),
            _ => "the store refused the entry".to_string(),
        };
        Self::new(format!("{step}: {what}"), e)
    }

    /// Two lines, once per service name and process.
    fn say(&self, service_name: &str) {
        eprintln!(
            "keyring: not used for {service_name} by this process, which derives its key from this machine: {}",
            self.what
        );
        if !self.detail.is_empty() {
            eprintln!("keyring: the store said: {}", self.detail);
        }
    }

    fn into_error(self) -> SecretError {
        if self.detail.is_empty() {
            SecretError::Keyring(self.what)
        } else {
            SecretError::Keyring(format!("{}: {}", self.what, self.detail))
        }
    }
}

/// The one entry the keyring path uses: the master key, kept as text.
///
/// A seam, so the tests put an in-memory store where the system one goes
/// and never reach the real keyring.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) trait MasterKeyEntry {
    /// `Ok(None)` when there is no entry yet.
    fn read(&self) -> Result<Option<String>, Refusal>;
    fn write(&self, value: &str) -> Result<(), Refusal>;
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
struct SystemEntry(keyring::Entry);

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl MasterKeyEntry for SystemEntry {
    fn read(&self) -> Result<Option<String>, Refusal> {
        match self.0.get_password() {
            Ok(existing) => Ok(Some(existing)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Refusal::from_store("reading the entry", e)),
        }
    }

    fn write(&self, value: &str) -> Result<(), Refusal> {
        self.0.set_password(value).map_err(|e| Refusal::from_store("writing the entry", e))
    }
}

/// The key the entry holds, or a new one minted and stored there.
///
/// After a mint the entry is read back and what it holds is the key: a
/// second process can store its own in between, and a key the keyring did
/// not keep seals nothing that opens on the next start.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn master_key_from(entry: &dyn MasterKeyEntry) -> Result<[u8; MASTER_KEY_LEN], Refusal> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;

    if let Some(existing) = entry.read()? {
        return decode_key(&existing);
    }
    let mut minted = [0u8; MASTER_KEY_LEN];
    rand::rng().fill(&mut minted);
    entry.write(&B64.encode(minted))?;
    match entry.read()? {
        Some(stored) => decode_key(&stored),
        None => Err(Refusal::new("the entry did not read back after it was written", "")),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn decode_key(text: &str) -> Result<[u8; MASTER_KEY_LEN], Refusal> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;

    let bytes = B64.decode(text.trim()).map_err(|e| Refusal::new("the entry is not a key", e))?;
    if bytes.len() != MASTER_KEY_LEN {
        let size = format!("it is {} bytes and should be {MASTER_KEY_LEN}", bytes.len());
        return Err(Refusal::new("the entry is not a key", size));
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
    derive_from(service_name, &install_uuid)
}

/// The derived key of an install whose id already exists; `None` when the
/// derived path never ran here. Nothing is written.
fn existing_derived_key(service_name: &str, storage_dir: &Path) -> Option<[u8; MASTER_KEY_LEN]> {
    derive_from(service_name, &install_id::load(storage_dir)?).ok()
}

fn derive_from(
    service_name: &str,
    install_uuid: &uuid::Uuid,
) -> Result<[u8; MASTER_KEY_LEN], SecretError> {
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

    /// A box on the keyring, on an install where the derived path never ran.
    fn keyring_box(service_name: &str, entry: &dyn MasterKeyEntry) -> Backend {
        let install = tempfile::tempdir().unwrap();
        Backend::with_entry(service_name, install.path(), entry).unwrap()
    }

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
        fn read(&self) -> Result<Option<String>, Refusal> {
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

        fn write(&self, value: &str) -> Result<(), Refusal> {
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
        fn read(&self) -> Result<Option<String>, Refusal> {
            Ok(self.value.lock().unwrap().clone())
        }

        fn write(&self, _value: &str) -> Result<(), Refusal> {
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
                std::thread::spawn(move || {
                    keyring_box("test-fresh-start", &*entry).master_key
                })
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
        let backend = keyring_box("test-overtaken", &entry);
        assert_eq!(
            backend.master_key,
            [7u8; MASTER_KEY_LEN],
            "a key the keyring did not keep seals nothing worth keeping"
        );
    }

    /// A store that counts how often it is asked.
    struct Counting {
        value: Mutex<Option<String>>,
        reads: Mutex<usize>,
        writes: Mutex<usize>,
    }

    impl MasterKeyEntry for Counting {
        fn read(&self) -> Result<Option<String>, Refusal> {
            *self.reads.lock().unwrap() += 1;
            Ok(self.value.lock().unwrap().clone())
        }

        fn write(&self, value: &str) -> Result<(), Refusal> {
            *self.writes.lock().unwrap() += 1;
            *self.value.lock().unwrap() = Some(value.to_string());
            Ok(())
        }
    }

    #[test]
    fn a_second_box_in_the_same_process_takes_the_first_key_without_asking_again() {
        let entry = fresh();
        let first = keyring_box("test-once-per-process", &entry);
        let second = keyring_box("test-once-per-process", &entry);
        assert_eq!(first.master_key, second.master_key);
        assert_eq!(*entry.writes.lock().unwrap(), 1, "one mint");
        assert_eq!(*entry.reads.lock().unwrap(), 2, "one look, one read back, and nothing more");
    }

    #[test]
    fn an_entry_that_exists_is_used_as_it_is() {
        let entry = Counting {
            value: Mutex::new(Some(B64.encode([9u8; MASTER_KEY_LEN]))),
            reads: Mutex::new(0),
            writes: Mutex::new(0),
        };
        let backend = keyring_box("test-existing-entry", &entry);
        assert_eq!(backend.master_key, [9u8; MASTER_KEY_LEN]);
        assert_eq!(*entry.writes.lock().unwrap(), 0, "nothing to mint");
    }

    fn fresh() -> Counting {
        Counting { value: Mutex::new(None), reads: Mutex::new(0), writes: Mutex::new(0) }
    }

    #[test]
    fn a_keyring_box_opens_what_the_derived_path_sealed_on_this_install() {
        // One start found no keyring and sealed with the derived key; the
        // next one has the keyring again and must not ask for the sign-in.
        let install = tempfile::tempdir().unwrap();
        let derived = Backend::derived_only("test-flip", install.path()).unwrap();
        let sealed = derived.seal(b"token").unwrap();
        let entry = fresh();
        let keyring = Backend::with_entry("test-flip", install.path(), &entry).unwrap();
        assert_eq!(keyring.open(&sealed).unwrap(), b"token");
        let resealed = keyring.seal(b"token").unwrap();
        assert_eq!(resealed[1], MARKER_KEYRING, "new seals stay on the keyring");
        assert!(matches!(derived.open(&resealed).unwrap_err(), SecretError::BackendMismatch));
    }

    #[test]
    fn what_the_derived_path_sealed_on_another_install_does_not_open_on_the_keyring() {
        let one = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let sealed = Backend::derived_only("test-flip-elsewhere", one.path()).unwrap().seal(b"token").unwrap();
        let entry = fresh();
        let keyring = Backend::with_entry("test-flip-elsewhere", other.path(), &entry).unwrap();
        assert!(
            matches!(keyring.open(&sealed).unwrap_err(), SecretError::BackendMismatch),
            "no install id here, so nothing was ever derived here"
        );
        assert!(!other.path().join("install-id").exists(), "opening writes no install id");
        Backend::derived_only("test-flip-elsewhere", other.path()).unwrap();
        assert!(
            matches!(keyring.open(&sealed).unwrap_err(), SecretError::Cipher(_)),
            "another install id derives another key"
        );
    }

    /// A locked store: every read is refused.
    struct Locked {
        reads: Mutex<usize>,
    }

    impl MasterKeyEntry for Locked {
        fn read(&self) -> Result<Option<String>, Refusal> {
            *self.reads.lock().unwrap() += 1;
            Err(Refusal::new("reading the entry: no access to the store (locked or refused)", "object locked"))
        }

        fn write(&self, _value: &str) -> Result<(), Refusal> {
            unreachable!("nothing is minted after a refused read")
        }
    }

    #[test]
    fn a_refused_keyring_is_asked_once_and_every_box_takes_the_refusal() {
        let entry = Locked { reads: Mutex::new(0) };
        for _ in 0..2 {
            let refused = SHARED.resolve("test-locked", || master_key_from(&entry)).unwrap_err();
            assert!(
                matches!(&refused, SecretError::Keyring(why) if why.contains("no access to the store")),
                "{refused}"
            );
        }
        assert_eq!(*entry.reads.lock().unwrap(), 1, "the second box asks the store nothing");
    }

    #[test]
    fn the_first_line_of_a_refusal_survives_the_log_redaction() {
        // The session log blanks a line that has any of these words, and the
        // stores' own messages often do; the line that says what failed must
        // not.
        let blanked = ["secret", "password", "credential", "token"];
        let answers = vec![
            keyring::Error::NoStorageAccess("Secret Service: object locked".into()),
            keyring::Error::PlatformFailure("org.freedesktop.secrets was not provided".into()),
            keyring::Error::BadEncoding(vec![0xff]),
            keyring::Error::Invalid("service".into(), "a password-like name".into()),
        ];
        for answer in answers {
            let refusal = Refusal::from_store("reading the entry", answer);
            let what = refusal.what.to_lowercase();
            assert!(blanked.iter().all(|word| !what.contains(word)), "{what}");
            assert!(!refusal.detail.is_empty(), "the store's own message is kept for a terminal");
        }
    }
}
