//! Conectar una cuenta sin que el plugin toque el token.
//!
//! El ADR-008 lo decide así y la razón es una: **el refresh token es lo más
//! valioso que hay**. No caduca pronto y sirve para pedir accesos nuevos
//! indefinidamente. Si pasara por las manos del plugin, aunque fuera un
//! momento, todo lo demás sería decoración. Así que el plugin declara sus
//! endpoints en el manifest y Chilpa corre el flujo entero.
//!
//! # Qué flujo
//!
//! Authorization Code con PKCE y redirección a loopback, que es lo que el
//! RFC 8252 dice para una aplicación de escritorio. Sin `client_secret`:
//! un secreto dentro de un binario que se distribuye no es un secreto, y
//! PKCE es justo lo que lo reemplaza.
//!
//! # Lo que este crate no hace
//!
//! No guarda nada. Contesta [`Tokens`] y quien llame decide dónde ponerlos
//! —en Chilpa, cifrados, con `chilpa-secrets`—. Tampoco abre el navegador:
//! devuelve la URL a la que hay que mandar al usuario.

mod flow;
mod pkce;

pub use flow::{authorize_url, exchange, refresh, wait_for_code, Loopback, Post};
pub use pkce::Pkce;

use serde::{Deserialize, Serialize};

/// Cómo se conecta una cuenta en ese source. Sale del manifest del plugin.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Auth {
    /// Una credencial que el usuario pega y ya: una API key, un token de
    /// larga vida. Chilpa la guarda cifrada y la pone en cada petición.
    Token,
    /// OAuth 2.0 con código y PKCE.
    ///
    /// El nombre va escrito a mano: `rename_all` lo volvería `o_auth2`, y
    /// esto cruza el ABI. Un manifest dice `"type": "oauth2"`.
    #[serde(rename = "oauth2")]
    OAuth2 {
        /// A dónde se manda al usuario a decir que sí.
        authorize_url: String,
        /// A dónde se cambia el código por los tokens, y de donde se
        /// renuevan.
        token_url: String,
        /// La identidad de la aplicación ante ese servicio. No es un
        /// secreto: va en la URL que ve el usuario.
        client_id: String,
        /// Qué se le pide permiso de hacer.
        #[serde(default)]
        scopes: Vec<String>,
    },
}

impl Auth {
    /// Con qué servidor va a hablar el usuario, para poder decírselo antes
    /// de mandarlo. Vacío si no hay a dónde ir.
    pub fn host(&self) -> String {
        match self {
            Auth::Token => String::new(),
            Auth::OAuth2 { authorize_url, .. } => host_of(authorize_url).unwrap_or_default(),
        }
    }

    /// Qué está mal, si algo. Vacío si se puede usar.
    ///
    /// Se revisa al leer el manifest y no al conectarse: un plugin que pide
    /// mandar al usuario a `http://` no se instala, no falla a la mitad.
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

/// Lo que el servicio entrega cuando el usuario dice que sí.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Tokens {
    /// Con esto se firman las peticiones. Caduca.
    pub access_token: String,
    /// Con esto se pide uno nuevo cuando el de arriba caduca. Es el que
    /// hay que cuidar.
    #[serde(default)]
    pub refresh_token: String,
    /// Cuándo deja de servir el de arriba, en segundos unix. `None` si el
    /// servicio no lo dijo.
    #[serde(default)]
    pub expires_at: Option<i64>,
}

impl Tokens {
    /// Cierto si ya caducó, o está por caducar.
    ///
    /// Se renueva un minuto antes a propósito: entre que se revisa y llega
    /// la petición pasa tiempo, y un token que caduca en el camino se ve
    /// como un 401 que nadie sabe de dónde salió.
    pub fn stale(&self, now_unix: i64) -> bool {
        self.expires_at.is_some_and(|t| t - 60 <= now_unix)
    }

    /// Cierto si se puede renovar sin volver a molestar al usuario.
    pub fn renewable(&self) -> bool {
        !self.refresh_token.trim().is_empty()
    }

    /// Cómo va en la cabecera `Authorization`.
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

/// Cierto si esa URL se puede usar sin exponer la sesión.
///
/// `https` siempre; `http` solo a loopback, que no sale de la máquina y es
/// justo a donde el navegador regresa.
pub(crate) fn secure(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    host_of(url).is_some_and(|h| h == "127.0.0.1" || h == "localhost" || h == "[::1]")
}

/// La máquina de una URL, sin puerto ni usuario.
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
