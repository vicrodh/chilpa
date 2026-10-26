//! I keep credentials out of diagnostics and sign each authenticated request.
use crate::text::encode;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Credential {
    #[default]
    Anonymous,
    Token(String),
    OAuth { consumer_key: String, token: String, signature: String },
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { out.write_str("Credential(..)") }
}

fn clean(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && value.bytes().all(|b| b.is_ascii_graphic() && b != b',' && b != b'"' && b != b'\\')
}

impl Credential {
    pub fn personal(token: &str) -> Option<Self> {
        let token = token.trim();
        clean(token).then(|| Self::Token(token.into()))
    }
    /// I sign the two handshake legs with the same encoding as API requests.
    pub fn sign_oauth(consumer_key:&str,token:Option<&str>,signature:&str,now_unix:u64,nonce:&str)->Option<String> {
        if !clean(consumer_key) || !clean(signature) || !clean(nonce) || token.is_some_and(|t|!clean(t)) {return None;}
        let token=token.map(|t|format!("oauth_token=\"{}\", ",encode(t))).unwrap_or_default();
        Some(format!("OAuth oauth_consumer_key=\"{}\", oauth_nonce=\"{}\", {token}oauth_signature_method=\"PLAINTEXT\", oauth_timestamp=\"{now_unix}\", oauth_signature=\"{}\"",encode(consumer_key),encode(nonce),encode(signature)))
    }
    pub fn authenticated(&self) -> bool { !matches!(self, Self::Anonymous) }
    /// The signature is the RFC 5849 PLAINTEXT value, percent encoded again
    /// as an Authorization parameter. Both secrets are encoded in its inner value.
    pub fn header(&self, now_unix: u64, nonce: &str) -> Option<String> {
        match self {
            Self::Anonymous => None,
            Self::Token(token) if clean(token) => Some(format!("Discogs token={token}")),
            Self::OAuth { consumer_key, token, signature } => Self::sign_oauth(consumer_key,Some(token),signature,now_unix,nonce),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anonymous_has_no_header() { assert_eq!(Credential::Anonymous.header(1,"nonce"),None); }
    #[test]
    fn token_header_is_discogs_token() {
        assert_eq!(Credential::personal("  marker-token  ").unwrap().header(1,"n"),Some("Discogs token=marker-token".into()));
    }
    #[test]
    fn blank_or_dirty_token_has_no_header() {
        for token in ["", " ", "a\nb", "a\rb", "a, b", "a b", "a\"b", "a\\b"] {
            assert!(Credential::personal(token).is_none(),"{token:?}");
            assert!(Credential::Token(token.into()).header(1,"n").is_none());
        }
    }
    #[test]
    fn oauth_header_has_every_field_and_encodes_the_signature_parameter() {
        let key = Credential::OAuth {consumer_key:"consumer".into(),token:"access".into(),signature:"client%25secret&token%2Fsecret".into()};
        assert_eq!(key.header(123,"fresh"),Some("OAuth oauth_consumer_key=\"consumer\", oauth_nonce=\"fresh\", oauth_token=\"access\", oauth_signature_method=\"PLAINTEXT\", oauth_timestamp=\"123\", oauth_signature=\"client%2525secret%26token%252Fsecret\"".into()));
        assert!(key.header(123,"bad\nnonce").is_none());
    }
    #[test]
    fn request_token_signing_omits_the_absent_token_and_refuses_header_injection() {
        let header=Credential::sign_oauth("consumer",None,"client%25secret&",123,"fresh").unwrap();
        assert!(!header.contains("oauth_token="));assert!(header.contains("client%2525secret%26"));
        assert!(Credential::sign_oauth("consumer",None,"bad\r\nheader",123,"fresh").is_none());
    }
    #[test]
    fn debug_never_shows_secrets() {
        for key in [Credential::Token("MARKER_TOKEN".into()), Credential::OAuth {consumer_key:"MARKER_KEY".into(),token:"MARKER_TOKEN".into(),signature:"MARKER_SIGNATURE".into()}] {
            assert_eq!(format!("{key:?}"),"Credential(..)");
        }
    }
}
