//! Connecting an account without the plugin ever touching the token.
//!
//! ADR-008 decides it this way for one reason: **the refresh token is the
//! most valuable thing there is**. It does not expire soon and it can mint
//! new access indefinitely. If it passed through the plugin's hands, even
//! for a moment, everything else would be decoration. So the plugin
//! declares its endpoints in the manifest and Chilpa runs the whole flow.
//!
//! # Which flow
//!
//! Authorization Code with PKCE and a loopback redirect, which is what RFC
//! 8252 says for a desktop application. No `client_secret`: a secret inside
//! a binary that gets distributed is not a secret, and PKCE is exactly what
//! replaces it.
//!
//! # What this crate does not do
//!
//! It stores nothing. It answers [`Tokens`] and the caller decides where to
//! put them — in Chilpa, sealed, with `chilpa-secrets`. It does not open a
//! browser either: it returns the URL the user has to be sent to.

mod flow;
mod pkce;

pub use flow::{authorize_url, exchange, refresh, wait_for_code, Loopback, Post};
pub use pkce::Pkce;

use serde::{Deserialize, Serialize};

/// How an account connects on that source. Comes from the plugin's manifest.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Auth {
    /// A credential the user pastes and that is it: an API key, a long-lived
    /// token. Chilpa keeps it sealed and puts it on every request.
    Token,
    /// OAuth 2.0 with a code and PKCE.
    ///
    /// The name is written by hand: `rename_all` would turn it into `o_auth2`,
    /// and this crosses the ABI. A manifest says `"type": "oauth2"`.
    #[serde(rename = "oauth2")]
    OAuth2 {
        /// Where the user is sent to say yes.
        authorize_url: String,
        /// Where the code is exchanged for the tokens, and where they are
        /// renewed from.
        token_url: String,
        /// The application's identity to that service. Not a secret: it goes
        /// in the URL the user sees.
        client_id: String,
        /// What permission is asked for.
        #[serde(default)]
        scopes: Vec<String>,
    },
}

impl Auth {
    /// Which server the user is going to talk to, so they can be told before
    /// being sent. Empty when there is nowhere to go.
    pub fn host(&self) -> String {
        match self {
            Auth::Token => String::new(),
            Auth::OAuth2 { authorize_url, .. } => host_of(authorize_url).unwrap_or_default(),
        }
    }

    /// What is wrong, if anything. Empty when it can be used.
    ///
    /// Checked when the manifest is read and not when connecting: a plugin that
    /// would send the user to `http://` does not install, it does not fail
    /// halfway.
    pub fn problem(&self) -> Option<String> {
        let Auth::OAuth2 { authorize_url, token_url, client_id, .. } = self else {
            return None;
        };
        if client_id.trim().is_empty() {
            return Some("el client_id viene vacío".into());
        }
        for (what, url) in [("authorize_url", authorize_url), ("token_url", token_url)] {
            if !secure(url) {
                return Some(format!(
                    "{what} tiene que ser https (o loopback): mandar a alguien a dar \
                     permiso por http deja su sesión en el camino"
                ));
            }
        }
        None
    }
}

/// What the service hands over once the user says yes.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Tokens {
    /// Requests are signed with this. It expires.
    pub access_token: String,
    /// A new one is asked for with this when the one above expires. This is
    /// the one to look after.
    #[serde(default)]
    pub refresh_token: String,
    /// When the one above stops working, in unix seconds. `None` if the
    /// service did not say.
    #[serde(default)]
    pub expires_at: Option<i64>,
}

impl Tokens {
    /// True when it has expired, or is about to.
    ///
    /// It is renewed a minute early on purpose: time passes between the check
    /// and the request landing, and a token that expires on the way looks like
    /// a 401 nobody can place.
    pub fn stale(&self, now_unix: i64) -> bool {
        self.expires_at.is_some_and(|t| t - 60 <= now_unix)
    }

    /// True when it can be renewed without bothering the user again.
    pub fn renewable(&self) -> bool {
        !self.refresh_token.trim().is_empty()
    }

    /// How it goes in the `Authorization` header.
    pub fn header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("el servicio contestó {0}: {1}")]
    Refused(u16, String),
    #[error("el servicio contestó algo que no se entiende: {0}")]
    Unreadable(String),
    #[error("no se pudo hablar con el servicio: {0}")]
    Network(String),
    #[error("la respuesta no trae access_token")]
    NoToken,
    #[error("el navegador contestó con otro estado: alguien más metió mano")]
    WrongState,
    #[error("el usuario no terminó de dar permiso")]
    NoCode(String),
    #[error("disco: {0}")]
    Io(#[from] std::io::Error),
}

/// True when that URL can be used without exposing the session.
///
/// `https` always; `http` only to loopback, which never leaves the machine
/// and is exactly where the browser comes back to.
pub(crate) fn secure(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    host_of(url).is_some_and(|h| h == "127.0.0.1" || h == "localhost" || h == "[::1]")
}

/// A URL's host, without port or user.
pub(crate) fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let rest = rest.split(['/', '?', '#']).next()?;
    let rest = rest.rsplit_once('@').map(|(_, h)| h).unwrap_or(rest);
    let host = match rest.strip_prefix('[') {
        Some(v6) => format!("[{}]", v6.split(']').next()?),
        None => rest.split(':').next()?.to_string(),
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oauth(authorize: &str, token: &str) -> Auth {
        Auth::OAuth2 {
            authorize_url: authorize.into(),
            token_url: token.into(),
            client_id: "chilpa".into(),
            scopes: vec!["read".into()],
        }
    }

    #[test]
    fn a_service_reached_over_plain_http_is_refused_before_installing() {
        let a = oauth("http://ejemplo.com/authorize", "https://ejemplo.com/token");
        assert!(a.problem().unwrap().contains("https"));
    }

    #[test]
    fn loopback_over_http_is_fine_because_it_never_leaves_the_machine() {
        let a = oauth("http://127.0.0.1:9000/authorize", "http://localhost:9000/token");
        assert_eq!(a.problem(), None);
    }

    #[test]
    fn an_empty_client_id_is_refused() {
        let a = Auth::OAuth2 {
            authorize_url: "https://a.b/authorize".into(),
            token_url: "https://a.b/token".into(),
            client_id: "  ".into(),
            scopes: vec![],
        };
        assert!(a.problem().unwrap().contains("client_id"));
    }

    #[test]
    fn a_manifest_spells_it_the_way_a_person_would() {
        let a: Auth = serde_json::from_str(
            r#"{"type":"oauth2","authorize_url":"https://a.b/a",
                "token_url":"https://a.b/t","client_id":"x"}"#,
        )
        .expect("se lee como lo escribiría alguien");
        assert!(matches!(a, Auth::OAuth2 { .. }));
        assert!(serde_json::to_string(&a).unwrap().contains(r#""type":"oauth2""#));

        let t: Auth = serde_json::from_str(r#"{"type":"token"}"#).unwrap();
        assert_eq!(t, Auth::Token);
    }

    #[test]
    fn a_static_token_needs_no_endpoints() {
        assert_eq!(Auth::Token.problem(), None);
        assert_eq!(Auth::Token.host(), "");
    }

    #[test]
    fn the_user_can_be_told_where_they_are_going() {
        assert_eq!(oauth("https://cuentas.ejemplo.com/authorize", "https://a.b/t").host(),
                   "cuentas.ejemplo.com");
    }

    #[test]
    fn a_token_is_renewed_before_it_expires_not_after() {
        let t = Tokens { access_token: "a".into(), refresh_token: "r".into(), expires_at: Some(1_000) };
        assert!(!t.stale(900), "todavía sirve");
        assert!(t.stale(941), "le queda menos de un minuto: se renueva ya");
        assert!(t.stale(1_001));
        assert!(t.renewable());

        let undated = Tokens { access_token: "a".into(), ..Default::default() };
        assert!(!undated.stale(9_999), "sin fecha no se adivina que caducó");
        assert!(!undated.renewable(), "y sin refresh no hay cómo renovarlo");
    }
}
