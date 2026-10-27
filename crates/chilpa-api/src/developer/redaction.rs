//! I redact before a line enters memory, disk, the window or an export.

/// Only explicitly typed library version facts may bypass address detection.
pub(super) fn version(text: &str) -> String {
    if text.len() <= 32 && text.split('.').count() <= 4
        && text.split('.').all(|part| !part.is_empty() && part.len() <= 8 && part.bytes().all(|byte| byte.is_ascii_digit())) {
        text.into()
    } else {
        clean(text)
    }
}

pub(super) fn clean(text: &str) -> String {
    let text: String = text.chars().filter(|character| !character.is_control() || character.is_whitespace()).collect();
    let lower = text.to_ascii_lowercase();
    if ["password", "passwd", "token", "bearer", "authorization", "cookie", "secret", "credential",
        "api_key", "api-key", "apikey", "access_key", "session=", "session_id", "sessionid", "private key", "username", "user_name",
        "signature", "client_id", "client-id"].iter().any(|key| lower.contains(key)) {
        return "[redacted sensitive log entry]".into();
    }
    text.split_whitespace().map(|word| {
        let value = word.trim_start_matches(['\'', '"', '(', '[']);
        if value.contains("://") || value.starts_with("www.") { "[url]" }
        else if value.contains('/') || value.contains(":\\") { "[path]" }
        else if value.contains('@') { "[identity]" }
        else if value.trim_matches(['"', '\'', ',', ';', '[', ']']).parse::<std::net::IpAddr>().is_ok() { "[address]" }
        else if value.len() >= 32 && value.chars().all(|character| character.is_ascii_alphanumeric() || "_-=+.".contains(character)) { "[identifier]" }
        else { word }
    }).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_versions_are_an_explicit_bounded_exception() {
        assert_eq!(version("1.2.15.3"), "1.2.15.3");
        assert_eq!(version(""), "");
        assert_eq!(clean("endpoint 192.168.1.2"), "endpoint [address]");
        assert_eq!(clean("1.2.15.3"), "[address]");
        for value in ["password=fixture", "/home/fixture/1.2.15.3", "https://fixture.invalid/1.2.15.3", "1.2.15.3 person@fixture.invalid"] {
            assert!(!version(value).contains("fixture"));
        }
        assert_eq!(version(&"1".repeat(33)), "[identifier]");
    }
    #[test]
    fn private_fields_and_locations_never_reach_the_sink() {
        for input in ["Authorization: Bearer fixture", "password=fixture", "{\"api_key\":\"fixture\"}", "token fixture", "cookie fixture"] {
            assert!(!clean(input).contains("fixture"));
        }
        assert_eq!(clean("read /home/person/music.flac from https://example.invalid/a?key=fixture"), "read [path] from [url]");
        assert_eq!(clean("user person@example.invalid C:\\Users\\person\\Music"), "user [identity] [path]");
        assert_eq!(clean("audio: opened 96000 Hz"), "audio: opened 96000 Hz");
        for input in ["https://user:fixture@example.invalid/path?key=fixture", "API-Key: fixture", "Authorization:\nBearer fixture", "to\0ken=fixture"] {
            assert!(!clean(input).contains("fixture"));
        }
        assert_eq!(clean("{\"file\":\"/home/person/private.flac\"}"), "[path]");
        assert_eq!(clean("café 日本語"), "café 日本語");
        assert_eq!(clean("host 192.168.1.2"), "host [address]");
    }
}
