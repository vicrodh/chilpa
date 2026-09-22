//! Tipos compartidos por todo Chilpa. Sin Qt, sin I/O y sin lógica.
//!
//! Todo lo que existe en la biblioteca lleva su procedencia: de qué source
//! salió y de qué instancia de ese source (ADR-003, ADR-004). Ningún tipo
//! nombra un servicio concreto.

use std::fmt;

/// De dónde salió algo: el tipo de origen más la instancia (la cuenta o el
/// root concreto). Dos cuentas del mismo servicio son dos sources distintas.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceId {
    /// Quién provee: `local`, el id de un plugin o el de un servidor.
    pub provider: String,
    /// Cuál de sus instancias.
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

/// Un track dentro de su source. El id externo lo define el source; Chilpa no
/// lo interpreta.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TrackRef {
    pub source: SourceId,
    pub external_id: String,
}

/// Cómo suena algo, en términos que no dependen de ningún servicio: formato,
/// profundidad de bits, frecuencia y canales (ADR-004).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AudioQuality {
    pub codec: String,
    pub bit_depth: Option<u8>,
    pub sample_rate_hz: u32,
    pub channels: u8,
}

impl AudioQuality {
    /// Cierto si la salida puede ser bit-perfect: PCM con profundidad
    /// conocida. Los formatos con pérdida nunca lo son.
    pub fn is_lossless(&self) -> bool {
        self.bit_depth.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_source_local_se_reconoce() {
        assert!(SourceId::local().is_local());
        assert!(!SourceId::new("example-source", "cuenta-1").is_local());
    }

    #[test]
    fn dos_instancias_del_mismo_provider_son_distintas() {
        let a = SourceId::new("example-source", "cuenta-1");
        let b = SourceId::new("example-source", "cuenta-2");
        assert_ne!(a, b);
        assert_eq!(a.to_string(), "example-source:cuenta-1");
    }

    #[test]
    fn la_calidad_con_perdida_no_es_lossless() {
        let flac = AudioQuality { codec: "flac".into(), bit_depth: Some(24), sample_rate_hz: 96_000, channels: 2 };
        let lossy = AudioQuality { codec: "opus".into(), bit_depth: None, sample_rate_hz: 48_000, channels: 2 };
        assert!(flac.is_lossless());
        assert!(!lossy.is_lossless());
    }
}
