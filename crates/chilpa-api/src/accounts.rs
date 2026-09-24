//! A plugin's connected account, and how it renews itself.
//!
//! What is kept is not "a token": it is how that account connects. A fixed
//! credential is kept and that is that; an OAuth one also carries what to
//! ask for another with when it expires, and that is what needs guarding
//! most (ADR-008).
//!
//! **The plugin sees none of this.** It asks to reach a domain of its own
//! and Chilpa adds the header; if the access has expired, it is renewed
//! first, here, and the plugin never finds out a renewal happened.

use chilpa_auth::{Auth, Tokens};
use serde::{Deserialize, Serialize};

/// What Chilpa keeps per account, encrypted.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Account {
    /// What the user pasted: an API key, a long lived token. It goes into
    /// the header as is, because every service writes it its own way.
    Token { header: String },
    /// What came out of connecting over OAuth.
    OAuth { tokens: Tokens },
}

impl Account {
    /// What goes in `Authorization`, if there is anything to put there.
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

    /// True if it has to be renewed before being used.
    pub fn stale(&self, now_unix: i64) -> bool {
        match self {
            Account::Token { .. } => false,
            Account::OAuth { tokens } => tokens.stale(now_unix) && tokens.renewable(),
        }
    }

    /// True if it expired and there is no way to renew it: the user has to
    /// connect again. It is said, not attempted in vain.
    pub fn dead(&self, now_unix: i64) -> bool {
        match self {
            Account::Token { .. } => false,
            Account::OAuth { tokens } => tokens.stale(now_unix) && !tokens.renewable(),
        }
    }

    /// Asks for a fresh access. Returns the renewed account.
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
        // Every service writes its own differently: some "Bearer x", others
        // "Token x". Inventing a prefix would break half of them.
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
        // Trying to renew with no refresh would be a request known to be
        // doomed. It is said and the user connects again.
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
