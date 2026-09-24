//! The envelope that gets written to disk.
//!
//! ```text
//! [0]       version (u8)        — always 1 today
//! [1]       backend marker      — 0 = keyring, 1 = derived
//! [2..14]   nonce (12 bytes)    — different on every seal
//! [14..]    ciphertext + tag    — what AES-256-GCM produces
//! ```
//!
//! The marker travels inside so we can **refuse** to open something another
//! backend sealed, rather than trying and failing with a cipher error that
//! says nothing. If someone gains keyring access after having run with the
//! fallback, their old data is not silently ignored.

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
                "son {} bytes, no alcanzan ni para el encabezado",
                bytes.len()
            )));
        }
        let version = bytes[0];
        if version != ENVELOPE_VERSION {
            return Err(SecretError::Malformed(format!("versión {version} desconocida")));
        }
        Ok(Self {
            backend_marker: bytes[1],
            nonce: &bytes[2..HEADER_LEN],
            ciphertext: &bytes[HEADER_LEN..],
        })
    }

    pub fn build(backend_marker: u8, nonce: &[u8], ciphertext: &[u8]) -> Vec<u8> {
        assert_eq!(nonce.len(), NONCE_LEN, "el nonce son 12 bytes");
        let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
        out.push(ENVELOPE_VERSION);
        out.push(backend_marker);
        out.extend_from_slice(nonce);
        out.extend_from_slice(ciphertext);
        out
    }
}
