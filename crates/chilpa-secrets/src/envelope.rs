//! The envelope that gets written to disk.
//!
//! ```text
//! [0]       version (u8)        — always 1 today
//! [1]       backend marker      — 0 = keyring, 1 = derived
//! [2..14]   nonce (12 bytes)    — different on every seal
//! [14..]    ciphertext + tag    — what AES-256-GCM produces
//! ```
//!
//! The marker travels inside so the box knows which key sealed it. Without
//! the keyring, what the keyring sealed is **refused** by name rather than
//! failing with a cipher error that says nothing. On the keyring, what the
//! fallback sealed is opened with the derived key, so gaining keyring access
//! after a run with the fallback does not lose that run's data.

use crate::error::SecretError;

pub(crate) const ENVELOPE_VERSION: u8 = 1;
pub(crate) const NONCE_LEN: usize = 12;
pub(crate) const HEADER_LEN: usize = 2 + NONCE_LEN;

/// A sealed secret, already split into its parts.
#[derive(Debug)]
pub struct Sealed<'a> {
    pub backend_marker: u8,
    pub nonce: &'a [u8],
    pub ciphertext: &'a [u8],
}

impl<'a> Sealed<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, SecretError> {
        // The GCM tag is 16 bytes: less than that is not a short envelope, it
        // is something else.
        if bytes.len() < HEADER_LEN + 16 {
            return Err(SecretError::Malformed(format!(
                "it is {} bytes, not even enough for the header",
                bytes.len()
            )));
        }
        let version = bytes[0];
        if version != ENVELOPE_VERSION {
            return Err(SecretError::Malformed(format!("unknown version {version}")));
        }
        Ok(Self {
            backend_marker: bytes[1],
            nonce: &bytes[2..HEADER_LEN],
            ciphertext: &bytes[HEADER_LEN..],
        })
    }

    pub fn build(backend_marker: u8, nonce: &[u8], ciphertext: &[u8]) -> Vec<u8> {
        assert_eq!(nonce.len(), NONCE_LEN, "the nonce is 12 bytes");
        let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
        out.push(ENVELOPE_VERSION);
        out.push(backend_marker);
        out.extend_from_slice(nonce);
        out.extend_from_slice(ciphertext);
        out
    }
}
