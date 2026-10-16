//! AES-256-GCM, which is what both backends use.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngExt;

use crate::envelope::{Sealed, NONCE_LEN};
use crate::error::SecretError;

pub(crate) fn seal_with_key(
    key: &[u8; 32],
    backend_marker: u8,
    plaintext: &[u8],
) -> Result<Vec<u8>, SecretError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext =
        cipher.encrypt(nonce, plaintext).map_err(|e| SecretError::Cipher(e.to_string()))?;

    Ok(Sealed::build(backend_marker, &nonce_bytes, &ciphertext))
}

pub(crate) fn open_with_key(
    key: &[u8; 32],
    expected_marker: u8,
    sealed: &[u8],
) -> Result<Vec<u8>, SecretError> {
    let parsed = Sealed::parse(sealed)?;
    if parsed.backend_marker != expected_marker {
        return Err(SecretError::BackendMismatch);
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(parsed.nonce);
    cipher.decrypt(nonce, parsed.ciphertext).map_err(|e| SecretError::Cipher(e.to_string()))
}
