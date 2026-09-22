//! El core headless de Chilpa: biblioteca, player, sources y plugins.
//!
//! Nadie habla con este crate desde un frontend. La única puerta pública es
//! `chilpa-api` (ADR-001); aquí adentro vive la implementación.

use chilpa_models::SourceId;

/// Estado del proceso, por ahora vacío. Crece conforme entran los crates
/// portados.
#[derive(Default)]
pub struct Core {
    sources: Vec<SourceId>,
}

impl Core {
    pub fn new() -> Self {
        Self { sources: vec![SourceId::local()] }
    }

    /// Los sources registrados, en el orden en que se agregaron.
    pub fn sources(&self) -> &[SourceId] {
        &self.sources
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_core_arranca_con_la_biblioteca_local() {
        let core = Core::new();
        assert_eq!(core.sources(), &[SourceId::local()]);
    }
}
