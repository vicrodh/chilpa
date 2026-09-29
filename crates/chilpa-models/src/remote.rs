//! How a track that lives on a server travels through the queue.
//!
//! The queue carries strings. A file is its path, a track of a disc image
//! is `sacd:<image>#<n>`, and a track on a server is
//! `remote:<provider>/<instance>/<id>`, each part percent-encoded so that a
//! slash or a space in a server's id cannot cut the entry.
//!
//! The entry names the track and nothing else: never the address it is
//! fetched from, never a credential. Both are looked up when it plays. So a
//! saved queue carries no secret, and it survives the server changing
//! address.

use crate::source::SourceId;

pub const PREFIX: &str = "remote:";

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RemoteEntry {
    pub source: SourceId,
    /// The track's id on its server, as the server wrote it.
    pub external_id: String,
}

impl RemoteEntry {
    pub fn new(source: SourceId, external_id: impl Into<String>) -> Self {
        Self { source, external_id: external_id.into() }
    }

    /// What goes in the queue.
    pub fn to_entry(&self) -> String {
        format!(
            "{PREFIX}{}/{}/{}",
            encode(&self.source.provider),
            encode(&self.source.instance),
            encode(&self.external_id)
        )
    }

    /// Reads a queue entry back. `None` for anything that is not one of
    /// these, including a malformed one: a file path never parses.
    pub fn parse(entry: &str) -> Option<Self> {
        let rest = entry.strip_prefix(PREFIX)?;
        let mut parts = rest.splitn(3, '/');
        let provider = decode(parts.next()?)?;
        let instance = decode(parts.next()?)?;
        let external_id = decode(parts.next()?)?;
        if provider.is_empty() || instance.is_empty() || external_id.is_empty() {
            return None;
        }
        Some(Self { source: SourceId::new(provider, instance), external_id })
    }

    pub fn is_remote(entry: &str) -> bool {
        entry.starts_with(PREFIX)
    }
}

/// Keeps what can never be mistaken for a separator; every other byte of
/// the UTF-8 goes as `%XX`.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = text.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            // Only what `encode` writes: a raw slash or space means the
            // entry was not made here.
            b if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') => {
                out.push(b);
                i += 1;
            }
            _ => return None,
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_survives_the_round_trip_with_what_would_cut_it() {
        for id in ["tr-123", "al/42 with space", "0Hj5wUXsCkIBJ7Q8r2bh1d", "Canción ñ 日本", "%41", "a:b#c?d&e"] {
            let entry = RemoteEntry::new(SourceId::new("example-server", "4f1c9a0b2d3e4f50"), id);
            let written = entry.to_entry();
            assert!(!written[PREFIX.len()..].contains(' '), "{written}");
            assert_eq!(written.matches('/').count(), 2, "only the two separators: {written}");
            assert_eq!(RemoteEntry::parse(&written), Some(entry), "{written}");
        }
    }

    #[test]
    fn a_file_or_a_disc_track_is_not_a_remote_entry() {
        for entry in ["/music/a.flac", "sacd:/music/x.iso#3", "http://host/a.mp3", "cdda:/dev/sr0"] {
            assert_eq!(RemoteEntry::parse(entry), None, "{entry}");
            assert!(!RemoteEntry::is_remote(entry));
        }
    }

    #[test]
    fn a_malformed_entry_parses_to_nothing() {
        for entry in ["remote:", "remote:example", "remote:example/abc", "remote:example/abc/", "remote:example//x",
                      "remote:example/abc/a b", "remote:example/abc/%4", "remote:example/abc/%zz", "remote:example/abc/%FF"] {
            assert_eq!(RemoteEntry::parse(entry), None, "{entry}");
        }
    }

    #[test]
    fn two_instances_of_one_provider_write_two_entries() {
        let a = RemoteEntry::new(SourceId::new("example-server", "aaaa"), "same-id").to_entry();
        let b = RemoteEntry::new(SourceId::new("example-server", "bbbb"), "same-id").to_entry();
        assert_ne!(a, b);
    }
}
