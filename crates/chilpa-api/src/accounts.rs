//! La cuenta conectada de un plugin, y cómo se renueva sola.
//!
//! Lo que se guarda no es "un token": es cómo se conecta esa cuenta. Una
//! credencial fija se guarda y ya; una de OAuth trae además con qué pedir
//! otra cuando caduque, y eso es lo que hay que cuidar más (ADR-008).
//!
//! **El plugin no ve nada de esto.** Pide salir a un dominio suyo y Chilpa
//! pone la cabecera; si el acceso ya venció, lo renueva antes, aquí, y el
//! plugin ni se entera de que hubo una renovación.

use chilpa_auth::{Auth, Tokens};
use serde::{Deserialize, Serialize};

/// Lo que Chilpa guarda por cada cuenta, cifrado.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Account {
    /// Lo que el usuario pegó: una API key, un token de larga vida. Va tal
    /// cual en la cabecera, porque cada servicio la escribe a su manera.
    Token { header: String },
    /// Lo que salió de conectar con OAuth.
    OAuth { tokens: Tokens },
}

impl Account {
    /// Qué va en `Authorization`, si hay algo que poner.
    pub fn header(&self) -> Option<String> {
        match self {
            Account::Token { header } => {
                let h = header.trim();
                (!h.is_empty()).then(|| h.to_string())
            }
            Account::OAuth { tokens } => {
                (!tokens.access_token.trim().is_empty()).then(|| tokens.header())
            }
        }
    }

    /// Cierto si hay que renovar antes de usarla.
    pub fn stale(&self, now_unix: i64) -> bool {
        match self {
            Account::Token { .. } => false,
            Account::OAuth { tokens } => tokens.stale(now_unix) && tokens.renewable(),
        }
    }

    /// Cierto si caducó y no hay cómo renovarla: el usuario tiene que
    /// volver a conectar. Se dice, no se intenta en vano.
    pub fn dead(&self, now_unix: i64) -> bool {
        match self {
            Account::Token { .. } => false,
            Account::OAuth { tokens } => tokens.stale(now_unix) && !tokens.renewable(),
        }
    }

    /// Pide un acceso nuevo. Devuelve la cuenta renovada.
    pub fn renew(
        &self,
        auth: &Auth,
        now_unix: i64,
        post: chilpa_auth::Post<'_>,
    ) -> Result<Account, String> {
        let Account::OAuth { tokens } = self else {
            return Ok(self.clone());
        };
        let fresh = chilpa_auth::refresh(auth, tokens, now_unix, post).map_err(|e| e.to_string())?;
        Ok(Account::OAuth { tokens: fresh })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oauth(expires_at: Option<i64>, refresh: &str) -> Account {
        Account::OAuth {
            tokens: Tokens {
                access_token: "acceso".into(),
                refresh_token: refresh.into(),
                expires_at,
            },
        }
    }

    #[test]
    fn a_static_credential_goes_in_the_header_the_way_it_was_given() {
        // Cada servicio escribe la suya distinto: unos "Bearer x", otros
        // "Token x". Inventarle un prefijo sería romper la mitad.
        let a = Account::Token { header: "Token abc123".into() };
        assert_eq!(a.header().unwrap(), "Token abc123");
        assert!(!a.stale(9_999), "una fija no caduca sola");
        assert!(!a.dead(9_999));
    }

    #[test]
    fn an_empty_credential_is_no_credential() {
        assert_eq!(Account::Token { header: "   ".into() }.header(), None);
    }

    #[test]
    fn an_oauth_account_signs_as_bearer() {
        assert_eq!(oauth(None, "r").header().unwrap(), "Bearer acceso");
    }

    #[test]
    fn one_that_expired_and_can_be_renewed_is_stale_not_dead() {
        let a = oauth(Some(1_000), "para-renovar");
        assert!(a.stale(1_001));
        assert!(!a.dead(1_001), "hay con qué renovarla sin molestar a nadie");
    }

    #[test]
    fn one_that_expired_without_a_refresh_is_dead_and_says_so() {
        // Intentar renovar sin refresh sería una petición que se sabe que
        // va a fallar. Se dice y el usuario vuelve a conectar.
        let a = oauth(Some(1_000), "");
        assert!(!a.stale(1_001), "no hay nada que intentar");
        assert!(a.dead(1_001));
    }

    #[test]
    fn what_is_stored_reads_back_the_same() {
        for a in [
            Account::Token { header: "Token x".into() },
            oauth(Some(42), "r"),
        ] {
            let json = serde_json::to_vec(&a).unwrap();
            assert_eq!(serde_json::from_slice::<Account>(&json).unwrap(), a);
        }
    }
}
