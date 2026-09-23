//! El core headless de Chilpa: biblioteca, player, sources y plugins.
//!
//! Nadie habla con este crate desde un frontend. La única puerta pública es
//! `chilpa-api` (ADR-001); aquí adentro vive la implementación.

pub mod playback;

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

/// Lo que el core le cuenta a la fachada cuando terminó de sonar algo.
pub struct PlayedSummary {
    pub frames: u64,
    pub quality: chilpa_models::AudioQuality,
    pub bit_perfect: bool,
}

impl Core {
    /// Reproduce un archivo local en un dispositivo ALSA directo.
    ///
    /// Es la ruta mínima de la rebanada vertical: decodificar, medir el
    /// dispositivo, negociar y entregar. El error sale ya redactado para la
    /// UI, porque la fachada no conoce los tipos de adentro.
    #[cfg(target_os = "linux")]
    pub fn play_local(&mut self, path: &std::path::Path, device: &str, bit_perfect: bool) -> Result<PlayedSummary, String> {
        use chilpa_audio::negotiate::Policy;
        use chilpa_audio::render::Outcome;
        use chilpa_audio::sink::StopSignal;

        let caps = chilpa_alsa::probe(device).map_err(|e| format!("{e:?}"))?;
        let policy = if bit_perfect { Policy::BitPerfect } else { Policy::AllowAnything };
        let played = playback::play_file(path, &caps, policy, &StopSignal::new(), |format| {
            chilpa_alsa::AlsaSink::open(device, format)
        })
        .map_err(|e| e.to_string())?;
        let opened = played.delivery.opened;
        let quality = chilpa_models::AudioQuality {
            codec: format!("{:?}", opened.sample_format).to_lowercase(),
            bit_depth: opened.sample_format.is_integer().then(|| opened.sample_format.valid_bits()),
            sample_rate_hz: opened.sample_rate_hz,
            channels: opened.channels as u8,
        };
        match played.outcome {
            Outcome::Finished { frames } | Outcome::Stopped { frames } => Ok(PlayedSummary {
                frames,
                quality,
                bit_perfect: played.delivery.is_bit_perfect(),
            }),
            Outcome::Failed { frames, error } => {
                Err(format!("el dispositivo falló tras {frames} frames: {error:?}"))
            }
        }
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
