//! The PKCE challenge (RFC 7636).
//!
//! Without this, anyone who intercepted the authorization code — another app
//! on the same machine listening on loopback, a browser log — could exchange
//! it for tokens themselves. With it, the code is only good to whoever holds
//! the verifier, which never left this process.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use rand::RngExt;
use sha2::{Digest, Sha256};

/// A verifier and its challenge.
#[derive(Clone, Debug)]
pub struct Pkce {
    verifier: String,
}

impl Pkce {
    /// A fresh one, with 32 bytes of randomness.
    ///
    /// The RFC asks for 43 to 128 characters; 32 bytes in base64url give 43,
    /// which is the minimum and is already 256 bits.
    pub fn new() -> Self {
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        Self { verifier: B64.encode(bytes) }
    }

    /// What is sent when exchanging the code for tokens. It does not leave here
    /// until that moment.
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// What goes in the URL the user is sent to.
    pub fn challenge(&self) -> String {
        B64.encode(Sha256::digest(self.verifier.as_bytes()))
    }

    /// What the method is called, exactly as the service expects it.
    pub fn method() -> &'static str {
        "S256"
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// A random value for the `state`, which is what ties the browser's reply
/// to the request that caused it.
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
        // The example from RFC 7636, appendix B.
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
        assert!(!p.challenge().contains('='), "base64url without padding");
        assert!(!p.verifier().contains('+') && !p.verifier().contains('/'));
    }
}
