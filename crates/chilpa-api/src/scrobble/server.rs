//! What the services somebody runs themselves share: reading the address
//! they typed, and keeping an account of several parts as one credential.

/// Where a server is, from what was typed: trimmed, with `https://` when
/// no scheme was written, and without trailing slashes. `None` for nothing,
/// or for what cannot be an address: a space in it, another scheme.
pub(super) fn origin(typed: &str) -> Option<String> {
    let typed = typed.trim().trim_end_matches('/');
    if typed.is_empty() || typed.contains(char::is_whitespace) {
        return None;
    }
    let with_scheme = if typed.starts_with("https://") || typed.starts_with("http://") {
        typed.to_string()
    } else if typed.contains("://") {
        return None;
    } else {
        format!("https://{typed}")
    };
    let parsed = url::Url::parse(&with_scheme).ok()?;
    parsed.host_str().filter(|host| !host.is_empty())?;
    Some(with_scheme)
}

/// An account's parts as the one text that is sealed: a line each. No part
/// holds a line break: an address, a key and a session never do.
pub(super) fn pack(parts: &[&str]) -> String {
    parts.join("\n")
}

pub(super) fn unpack(account: &str) -> Vec<&str> {
    account.split('\n').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_gets_a_scheme_and_loses_its_last_slash() {
        assert_eq!(origin("example.org").as_deref(), Some("https://example.org"));
        assert_eq!(origin("http://example.org:8080//").as_deref(), Some("http://example.org:8080"));
        assert_eq!(origin("https://example.org/fm").as_deref(), Some("https://example.org/fm"));
        assert_eq!(origin("  "), None);
        assert_eq!(origin("ftp://example.org"), None);
        assert_eq!(origin("not an address"), None);
    }

    #[test]
    fn an_account_comes_back_as_the_parts_it_was_made_of() {
        assert_eq!(unpack(&pack(&["https://example.org", "key", "session"])), ["https://example.org", "key", "session"]);
    }
}
