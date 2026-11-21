//! Cast diagnostics. The crate has no sink of its own: the host installs its
//! playback trace once, and every event goes there with the media token
//! redacted.
use std::{fmt, sync::OnceLock};

pub type Sink = fn(&str, &str, fmt::Arguments<'_>);
static SINK: OnceLock<Sink> = OnceLock::new();

/// Sets the sink. The first call wins; later calls are ignored.
pub fn install(sink: Sink) { let _ = SINK.set(sink); }

pub(crate) fn event(scope: &str, event: &str, details: fmt::Arguments<'_>) {
    if let Some(sink) = SINK.get() { sink(scope, event, details); }
}

/// `/audio/<token>/<id>` becomes `/audio/***/<id>`: a media token never
/// reaches a log.
pub fn redact(text: &str) -> String {
    if !text.contains("/audio/") { return text.to_string(); }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("/audio/") {
        out.push_str(&rest[..i]);
        out.push_str("/audio/***");
        rest = &rest[i + "/audio/".len()..];
        match rest.find('/') {
            Some(slash) => { out.push('/'); rest = &rest[slash + 1..]; }
            None => rest = "",
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::redact;
    #[test]
    fn a_media_token_never_reaches_the_trace() {
        assert_eq!(redact("http://192.168.1.2:9876/audio/deadbeefcafebabe/42"), "http://192.168.1.2:9876/audio/***/42");
        assert_eq!(redact("/audio/tokentokentoken/7?x=1"), "/audio/***/7?x=1");
        assert_eq!(redact("/audio/onlytoken"), "/audio/***");
        assert_eq!(redact("<res>http://a/audio/t1/1</res><res>http://a/audio/t2/2</res>"), "<res>http://a/audio/***/1</res><res>http://a/audio/***/2</res>");
        assert_eq!(redact("/other/path"), "/other/path");
    }
}
