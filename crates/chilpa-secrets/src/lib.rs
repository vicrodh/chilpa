//! Where a secret is kept so it does not sit there in the clear.
//!
//! Chilpa uses it for plugin credentials (ADR-008). A plaintext token is
//! readable by any process running as the user, and that cannot be
//! sustained in a program that promises a plugin never sees your secrets.
//!
//! # Two paths, one API
//!
//! 1. **The system keyring** (preferred): an AES-256 master key lives inside
//!    the system store — libsecret on Linux, Keychain on macOS, DPAPI on
//!    Windows — and everything else is encrypted with it.
//! 2. **Derived from the machine** (when there is no keyring): the key comes
//!    from HKDF-SHA256 over `machine-id` plus an install UUID. Weaker,
//!    because anyone with filesystem access can rebuild it, but it works on
//!    a headless box and **is still bound to that machine**.
//!
//! Both produce the same envelope, so the caller does not have to know which
//! was used. What it can know, and tell the user, is
//! [`SecretBox::backend`].
//!
//! # When the path changes between starts
//!
//! A start can find the keyring missing or locked and the next one find it
//! back. A box on the keyring therefore also opens what the derived path
//! sealed on this install, with the derived key, which the install can
//! always rebuild; what it seals from then on goes to the keyring. The other
//! way does not work: a box without the keyring has no keyring key, and what
//! the keyring sealed waits for a start that has it.
//!
//! # What it protects against
//!
//! - Copying the directory to another machine. The keyring entry is not
//!   there, or the `machine-id` differs, and what was sealed does not open.
//! - Looking at the files. A token is not pulled out of the blob by reading.
//!
//! # What it does not
//!
//! - Someone with a shell on the same machine who can run Chilpa: they can
//!   always ask Chilpa to open it, and that is right, it is the same model
//!   as any local encryption.
//! - Recompiling Chilpa to copy what it decrypts. That is an attack on the
//!   source, not on the cipher.
//! - Someone who can read this install's id and the `machine-id` rebuilds
//!   the derived key, so they can also write an envelope that a box on the
//!   keyring accepts as the derived path's. That is the same reach as the
//!   shell above; what the keyring sealed stays out of it.
//!
//! **Nothing is stored in the clear, ever, on either path.**

mod backend;
mod cipher;
mod envelope;
mod error;
mod install_id;
mod store;

pub use backend::BackendKind;
pub use error::SecretError;
pub use store::{PrivateStore, StoreError};

use std::path::Path;

/// Seals and opens small secrets.
pub struct SecretBox {
    backend: backend::Backend,
}

impl SecretBox {
    /// Opens the box. `service_name` is what the keyring entry is called;
    /// `storage_dir` is where the install id lives.
    pub fn open(service_name: &str, storage_dir: &Path) -> Result<Self, SecretError> {
        Ok(Self { backend: backend::Backend::new(service_name, storage_dir)? })
    }

    /// The derived path only, without touching the keyring. For tests.
    pub fn derived_only(service_name: &str, storage_dir: &Path) -> Result<Self, SecretError> {
        Ok(Self { backend: backend::Backend::derived_only(service_name, storage_dir)? })
    }

    /// Which path ended up active, so it can be said.
    pub fn backend(&self) -> BackendKind {
        self.backend.kind()
    }

    /// Seals. What comes out can be written to disk as is.
    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
        self.backend.seal(plaintext)
    }

    /// Opens. Fails if it was tampered with, if the keyring sealed it and
    /// this box is not on the keyring, or if the master key is no longer the
    /// same. A box on the keyring also opens what the derived path sealed on
    /// this install.
    pub fn open_sealed(&self, sealed: &[u8]) -> Result<Vec<u8>, SecretError> {
        self.backend.open(sealed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_in(dir: &Path) -> SecretBox {
        SecretBox::derived_only("chilpa-test", dir).unwrap()
    }

    #[test]
    fn what_goes_in_comes_back_out() {
        let dir = tempfile::tempdir().unwrap();
        let b = box_in(dir.path());
        let sealed = b.seal(b"a token nobody should read").unwrap();
        assert_ne!(sealed, b"a token nobody should read", "it was not left in the clear");
        assert_eq!(b.open_sealed(&sealed).unwrap(), b"a token nobody should read");
    }

    #[test]
    fn the_same_secret_twice_does_not_look_the_same() {
        // One nonce per seal: otherwise two accounts with the same token would
        // look identical on disk, and that already says something.
        let dir = tempfile::tempdir().unwrap();
        let b = box_in(dir.path());
        assert_ne!(b.seal(b"same").unwrap(), b.seal(b"same").unwrap());
    }

    #[test]
    fn a_byte_changed_is_a_secret_refused() {
        let dir = tempfile::tempdir().unwrap();
        let b = box_in(dir.path());
        let mut sealed = b.seal(b"token").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert!(b.open_sealed(&sealed).is_err(), "GCM authenticates, it does not only encrypt");
    }

    #[test]
    fn what_another_install_sealed_does_not_open_here() {
        // This is the underlying promise: copying the files elsewhere is not
        // enough, because over there the install id is another one.
        let one = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let sealed = box_in(one.path()).seal(b"token").unwrap();
        assert!(box_in(other.path()).open_sealed(&sealed).is_err());
    }

    #[test]
    fn a_sealed_blob_that_is_too_short_says_so_instead_of_crashing() {
        let dir = tempfile::tempdir().unwrap();
        let b = box_in(dir.path());
        assert!(matches!(b.open_sealed(b"").unwrap_err(), SecretError::Malformed(_)));
        assert!(matches!(b.open_sealed(b"\x01\x01short").unwrap_err(), SecretError::Malformed(_)));
    }

    #[test]
    fn what_one_backend_sealed_the_other_refuses_by_name() {
        // Without the marker this would fail with a cipher error that says
        // nothing. With it we can say what happened.
        let dir = tempfile::tempdir().unwrap();
        let mut sealed = box_in(dir.path()).seal(b"token").unwrap();
        sealed[1] = 0; // as if the keyring had sealed it
        assert!(matches!(
            box_in(dir.path()).open_sealed(&sealed).unwrap_err(),
            SecretError::BackendMismatch
        ));
    }

    #[test]
    fn what_any_box_sealed_in_a_race_opens_in_a_box_reopened_afterwards() {
        // The first start: several boxes open at once over a fresh directory
        // and each seals with the key it got. The next start reads the id
        // from the disk, so every one of those keys has to come from it.
        const THREADS: usize = 8;
        for _ in 0..50 {
            let dir = tempfile::tempdir().unwrap();
            let gate = std::sync::Barrier::new(THREADS);
            let sealed: Vec<Vec<u8>> = std::thread::scope(|scope| {
                let racers: Vec<_> = (0..THREADS)
                    .map(|_| {
                        scope.spawn(|| {
                            gate.wait();
                            SecretBox::derived_only("chilpa-test", dir.path()).unwrap().seal(b"token").unwrap()
                        })
                    })
                    .collect();
                racers.into_iter().map(|racer| racer.join().unwrap()).collect()
            });
            let reopened = box_in(dir.path());
            for blob in &sealed {
                assert_eq!(reopened.open_sealed(blob).unwrap(), b"token");
            }
        }
    }

    #[test]
    fn an_id_that_was_already_there_keeps_producing_the_same_key() {
        // An existing install keeps its file as it is, so what it sealed
        // before this code existed goes on opening.
        let one = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let id = "5f0f6f5e-3c55-4b1a-9d3e-0a1b2c3d4e5f";
        std::fs::write(one.path().join("install-id"), id).unwrap();
        std::fs::write(other.path().join("install-id"), id).unwrap();
        let sealed = box_in(one.path()).seal(b"token").unwrap();
        assert_eq!(box_in(other.path()).open_sealed(&sealed).unwrap(), b"token");
        assert_eq!(std::fs::read_to_string(other.path().join("install-id")).unwrap(), id);
    }
}
