//! Text on its way into an address.

/// A value as one component of an address: everything but letters, digits
/// and `-_.~` written as `%XX`.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// A value inside a Lucene query, with what Lucene would read as an
/// operator escaped.
pub fn query_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if matches!(c, '\\' | '"' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '~' | '*' | '?' | '!' | '+' | '-' | '&' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A MusicBrainz id: 8-4-4-4-12 hexadecimal digits, in lower case.
pub fn is_uuid(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(b),
        })
}

/// Only digits, from one to twenty of them: a Discogs release.
pub fn is_number(text: &str) -> bool {
    (1..=20).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_with_a_quote_and_a_colon_is_escaped() {
        assert_eq!(query_escape(r#"OK Computer: "OKNOTOK""#), r#"OK Computer\: \"OKNOTOK\""#);
        assert_eq!(query_escape(r"AC\DC"), r"AC\\DC");
        assert_eq!(query_escape("(What's the Story) Morning Glory?"), r"\(What's the Story\) Morning Glory\?");
        assert_eq!(query_escape("Björk"), "Björk", "letters are left alone, whatever their alphabet");
    }

    #[test]
    fn an_address_takes_anything() {
        assert_eq!(encode("a b"), "a%20b");
        assert_eq!(encode(r#"release:"A&B""#), "release%3A%22A%26B%22");
        assert_eq!(encode("Björk"), "Bj%C3%B6rk");
        assert_eq!(encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn an_id_is_one_or_it_is_not() {
        assert!(is_uuid("3d3207f9-c4c2-425c-a364-f359b4320805"));
        assert!(!is_uuid("3D3207F9-C4C2-425C-A364-F359B4320805"), "lower case: whoever calls lowers it first");
        assert!(!is_uuid("3d3207f9-c4c2-425c-a364-f359b432080"));
        assert!(!is_uuid("3d3207f9-c4c2-425c-a364-f359b4320805/../artist"));
        assert!(!is_uuid(""));
        assert!(is_number("1234567"));
        assert!(!is_number("") && !is_number("12a") && !is_number("123456789012345678901"));
    }
}
