//! Where the content comes from.

use crate::quality::AudioQuality;
use std::fmt;

/// The origin of something: who provides it, plus which instance of theirs.
///
/// Two accounts of the same provider are two different sources (ADR-003).
/// In QBZ only one instance per kind fit, and that was one of the failures
/// the audit found.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceId {
    /// `local`, a plugin's id, or a server's.
    pub provider: String,
    /// Which of its instances: an account, a server, a root.
    pub instance: String,
}

impl SourceId {
    pub fn new(provider: impl Into<String>, instance: impl Into<String>) -> Self {
        Self { provider: provider.into(), instance: instance.into() }
    }

    /// The local library: one provider with one instance.
    pub fn local() -> Self {
        Self::new("local", "default")
    }

    pub fn is_local(&self) -> bool {
        self.provider == "local"
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.instance)
    }
}

/// How reachable something is in a source. A source that is off, in
/// quarantine or without network leaves its content
/// [`Availability::Unreachable`]; it is never deleted (ADR-003, and the
/// audit's `scan.rs:1351`, where an unmounted root deleted all its tracks).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reachability {
    /// It can be played right now.
    Playable,
    /// The source is not answering; what we know about it still holds.
    Unreachable,
    /// The source answered and said it no longer has it.
    Gone,
}

/// A track seen from one particular source: whether it can be played and at
/// what quality. The same track can live in several sources at different
/// qualities, and the album view shows them all (ADR-003).
#[derive(Clone, PartialEq, Debug)]
pub struct Availability {
    pub source: SourceId,
    pub reachability: Reachability,
    /// The best quality that source offers, if it knows it.
    pub quality: Option<AudioQuality>,
}

impl Availability {
    pub fn playable(source: SourceId, quality: Option<AudioQuality>) -> Self {
        Self { source, reachability: Reachability::Playable, quality }
    }

    pub fn is_playable(&self) -> bool {
        self.reachability == Reachability::Playable
    }
}

/// Picks where to play: the user's order of preference first and,
/// among those that tie, the best quality. The ones that cannot be played do
/// not compete, but they still exist for the UI.
pub fn preferred<'a>(options: &'a [Availability], order: &[SourceId]) -> Option<&'a Availability> {
    options
        .iter()
        .filter(|a| a.is_playable())
        .min_by_key(|a| {
            let rank = order.iter().position(|s| *s == a.source).unwrap_or(order.len());
            let quality = a.quality.as_ref().map_or(0, AudioQuality::rank);
            (rank, std::cmp::Reverse(quality))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quality(bits: Option<u8>, hz: u32) -> AudioQuality {
        AudioQuality {
            codec: if bits.is_some() { "flac".into() } else { "opus".into() },
            bit_depth: bits,
            sample_rate_hz: hz,
            channels: 2,
        }
    }

    #[test]
    fn the_local_source_is_recognized() {
        assert!(SourceId::local().is_local());
        assert!(!SourceId::new("example-source", "cuenta-1").is_local());
    }

    #[test]
    fn two_instances_of_the_same_provider_are_different() {
        let a = SourceId::new("example-source", "cuenta-1");
        let b = SourceId::new("example-source", "cuenta-2");
        assert_ne!(a, b);
        assert_eq!(a.to_string(), "example-source:cuenta-1");
    }

    #[test]
    fn the_source_the_user_prefers_wins() {
        let local = SourceId::local();
        let remote = SourceId::new("example-source", "cuenta-1");
        let options = vec![
            Availability::playable(remote.clone(), Some(quality(Some(24), 192_000))),
            Availability::playable(local.clone(), Some(quality(Some(16), 44_100))),
        ];
        let chosen = preferred(&options, &[local.clone(), remote.clone()]).unwrap();
        assert_eq!(chosen.source, local, "el orden del usuario manda sobre la calidad");
    }

    #[test]
    fn among_equals_the_better_quality_wins() {
        let a = SourceId::new("example-source", "cuenta-1");
        let b = SourceId::new("otro-source", "cuenta-1");
        let options = vec![
            Availability::playable(a.clone(), Some(quality(Some(16), 44_100))),
            Availability::playable(b.clone(), Some(quality(Some(24), 96_000))),
        ];
        assert_eq!(preferred(&options, &[]).unwrap().source, b);
    }

    #[test]
    fn an_unreachable_source_is_not_chosen_but_stays_listed() {
        let local = SourceId::local();
        let options = vec![Availability {
            source: local.clone(),
            reachability: Reachability::Unreachable,
            quality: Some(quality(Some(24), 96_000)),
        }];
        assert!(preferred(&options, &[local]).is_none());
        assert_eq!(options.len(), 1, "lo inalcanzable no se borra");
    }
}
