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
//!
//! **Nothing is stored in the clear, ever, on either path.**

mod backend;
mod cipher;
mod envelope;
mod error;
mod install_id;

pub use backend::BackendKind;
pub use error::SecretError;

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

    /// Opens. Fails if it was tampered with, if the other backend sealed it, or
    /// if the master key is no longer the same.
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
}
