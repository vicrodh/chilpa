//! Koito: a listening tracker somebody runs on their own server, reached
//! through the ListenBrainz API it answers under `/apis/listenbrainz`.
//!
//! The account is the server's address and an API key made in its
//! settings. Both are kept together, sealed, as what is sent with: the
//! address is part of the account, not a preference that can drift from
//! the key it was checked with.

use super::queue::Service;
use super::{listenbrainz, server, Failure, Kind, Listen};

/// The path the ListenBrainz API lives under on a Koito server.
const SUFFIX: &str = "/apis/listenbrainz";

/// Listens in one import: Koito takes a thousand, like the API it copies.
const BATCH: usize = 100;

pub(super) const SERVICE: Service = Service { label: "Koito", batch: BATCH, begin: None, check, submit };

/// The API of the server at `address`, as typed: with or without its
/// scheme, a trailing slash, or the API's own path pasted along.
fn api(address: &str) -> Option<String> {
    let origin = server::origin(address)?;
    let origin = origin.strip_suffix("/1").unwrap_or(&origin);
    let origin = origin.strip_suffix(SUFFIX).unwrap_or(origin);
    Some(format!("{origin}{SUFFIX}/1"))
}

/// The address and the key as typed, a line each: the user the key
/// belongs to, and the API with the key to send with.
fn check(typed: &str) -> Result<(String, String), Failure> {
    let (address, key) = typed.split_once('\n').unwrap_or(("", typed));
    let api = api(address).ok_or_else(|| Failure::Refused(chilpa_i18n::t("Write the server's address first, like https://koito.example.com.")))?;
    let key = key.trim();
    let user = listenbrainz::validate(&api, key)?;
    Ok((user, server::pack(&[&api, key])))
}

fn submit(account: &str, kind: Kind, listens: &[Listen]) -> Result<(), Failure> {
    match server::unpack(account).as_slice() {
        [api, key] => listenbrainz::submit_to(api, key, kind, listens),
        // An account nobody can read is one to connect again.
        _ => Err(Failure::Token),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_reaches_the_api_however_it_was_typed() {
        let api_of = |typed| api(typed).unwrap();
        assert_eq!(api_of("https://koito.example.com"), "https://koito.example.com/apis/listenbrainz/1");
        assert_eq!(api_of(" koito.example.com/ "), "https://koito.example.com/apis/listenbrainz/1");
        assert_eq!(api_of("http://192.168.1.4:4110"), "http://192.168.1.4:4110/apis/listenbrainz/1");
        assert_eq!(api_of("https://example.com/koito/apis/listenbrainz/1/"), "https://example.com/koito/apis/listenbrainz/1");
        assert_eq!(api(""), None);
    }

    #[test]
    fn a_key_with_no_address_is_refused_with_the_reason() {
        assert!(matches!(check("\nkey"), Err(Failure::Refused(why)) if !why.is_empty()));
        assert!(matches!(check("key"), Err(Failure::Refused(_))));
    }

    #[test]
    fn an_account_that_cannot_be_read_asks_to_connect_again() {
        assert_eq!(submit("only-one-line", Kind::Single, &[Listen::default()]), Err(Failure::Token));
    }
}
