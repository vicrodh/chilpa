//! De dónde sale el contenido.

use crate::quality::AudioQuality;
use std::fmt;

/// El origen de algo: quién lo provee más de qué instancia suya.
///
/// Dos cuentas del mismo proveedor son dos sources distintas (ADR-003). En
/// QBZ solo cabía una instancia por tipo, y esa fue una de las fallas que
/// encontró la auditoría.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceId {
    /// `local`, el id de un plugin o el de un servidor.
    pub provider: String,
    /// Cuál de sus instancias: una cuenta, un servidor, un root.
    pub instance: String,
}

impl SourceId {
    pub fn new(provider: impl Into<String>, instance: impl Into<String>) -> Self {
        Self { provider: provider.into(), instance: instance.into() }
    }

    /// La biblioteca local: un solo provider con una sola instancia.
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

/// Qué tan accesible está algo en un source. Un source apagado, en cuarentena
/// o sin red deja su contenido [`Availability::Unreachable`]; nunca se borra
/// (ADR-003, y el `scan.rs:1351` de la auditoría, donde un root desmontado
/// borraba todos sus tracks).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reachability {
    /// Se puede reproducir ahora.
    Playable,
    /// El source no responde; lo que sabemos de él sigue siendo válido.
    Unreachable,
    /// El source respondió y dijo que ya no lo tiene.
    Gone,
}

/// Un track visto desde un source concreto: si se puede reproducir y con qué
/// calidad. Un mismo track puede estar en varios sources con calidades
/// distintas, y la vista de álbum las muestra todas (ADR-003).
#[derive(Clone, PartialEq, Debug)]
pub struct Availability {
    pub source: SourceId,
    pub reachability: Reachability,
    /// La mejor calidad que ofrece ese source, si la conoce.
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

/// Escoge dónde reproducir: primero el orden de preferencia del usuario y,
/// entre los que empatan, la mejor calidad. Los que no se pueden reproducir
/// no compiten, pero siguen existiendo para la UI.
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
