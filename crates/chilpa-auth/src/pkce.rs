//! El reto de PKCE (RFC 7636).
//!
//! Sin esto, quien lograra interceptar el código de autorización —otra app
//! en la misma máquina escuchando el loopback, un log del navegador— podría
//! cambiarlo por tokens él mismo. Con esto, el código solo sirve para quien
//! tenga el verificador, que nunca salió de este proceso.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use rand::RngExt;
use sha2::{Digest, Sha256};

/// Un verificador y su reto.
#[derive(Clone, Debug)]
pub struct Pkce {
    verifier: String,
}

impl Pkce {
    /// Uno nuevo, con 32 bytes de aleatoriedad.
    ///
    /// El RFC pide entre 43 y 128 caracteres; 32 bytes en base64url dan 43,
    /// que es el mínimo y ya son 256 bits.
    pub fn new() -> Self {
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        Self { verifier: B64.encode(bytes) }
    }

    /// Lo que se manda al cambiar el código por los tokens. No sale de aquí
    /// hasta ese momento.
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// Lo que va en la URL a la que se manda al usuario.
    pub fn challenge(&self) -> String {
        B64.encode(Sha256::digest(self.verifier.as_bytes()))
    }

    /// Cómo se llama el método, tal cual lo espera el servicio.
    pub fn method() -> &'static str {
        "S256"
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// Un valor aleatorio para el `state`, que es lo que amarra la respuesta del
/// navegador con la petición que la originó.
pub(crate) fn nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    B64.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verifier_is_long_enough_for_the_rfc() {
        let p = Pkce::new();
        assert!((43..=128).contains(&p.verifier().len()), "{}", p.verifier().len());
    }

    #[test]
    fn two_of_them_are_not_the_same() {
        assert_ne!(Pkce::new().verifier(), Pkce::new().verifier());
    }

    #[test]
    fn the_challenge_is_the_sha256_of_the_verifier_in_base64url() {
        // El ejemplo del RFC 7636, apéndice B.
        let p = Pkce { verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into() };
        assert_eq!(p.challenge(), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn the_challenge_never_reveals_the_verifier() {
        let p = Pkce::new();
        assert_ne!(p.challenge(), p.verifier());
        assert!(!p.challenge().contains(p.verifier()));
    }

    #[test]
    fn nothing_in_the_url_carries_padding_that_would_need_escaping() {
        let p = Pkce::new();
        assert!(!p.challenge().contains('='), "base64url sin relleno");
        assert!(!p.verifier().contains('+') && !p.verifier().contains('/'));
    }
}
