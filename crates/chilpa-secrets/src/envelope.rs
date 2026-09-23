//! El sobre que se guarda en disco.
//!
//! ```text
//! [0]       versión (u8)        — hoy siempre 1
//! [1]       marcador de backend — 0 = keyring, 1 = derivado
//! [2..14]   nonce (12 bytes)    — distinto en cada cifrado
//! [14..]    ciphertext + tag    — lo que sale de AES-256-GCM
//! ```
//!
//! El marcador va adentro para poder **negarse** a descifrar algo que hizo
//! otro backend, en lugar de intentarlo y fallar con un error de cripto que
//! no dice nada. Si alguien consigue acceso al keyring después de haber
//! corrido con el respaldo, sus datos viejos no se ignoran en silencio.

use crate::error::SecretError;

pub(crate) const ENVELOPE_VERSION: u8 = 1;
pub(crate) const NONCE_LEN: usize = 12;
pub(crate) const HEADER_LEN: usize = 2 + NONCE_LEN;

/// Un secreto cifrado, ya separado en sus partes.
#[derive(Debug)]
pub struct Sealed<'a> {
    pub backend_marker: u8,
    pub nonce: &'a [u8],
    pub ciphertext: &'a [u8],
}

impl<'a> Sealed<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, SecretError> {
        // El tag de GCM son 16 bytes: menos que eso no es un sobre corto,
        // es otra cosa.
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
