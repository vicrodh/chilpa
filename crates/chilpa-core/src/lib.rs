//! El core headless de Chilpa: biblioteca, player, sources y plugins.
//!
//! Nadie habla con este crate desde un frontend. La única puerta pública es
//! `chilpa-api` (ADR-001); aquí adentro vive la implementación.

#[cfg(target_os = "linux")]
pub mod cd_playback;
pub mod dsd_playback;
pub mod playback;
pub mod player;
pub mod settings;
pub mod queue_playback;

/// Lo que sale por el dispositivo, para quien lo quiera dibujar.
pub use chilpa_audio::tap;

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

/// Una salida disponible, vista desde la fachada.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Output {
    pub id: String,
    pub label: String,
}

/// Las salidas de la máquina, más la de diagnóstico.
///
/// La captura va al final y se llama por su nombre: sirve para probar la
/// ruta completa sin sacar sonido y para comprobar el bit-perfect contra el
/// archivo que produce.
#[cfg(target_os = "linux")]
pub fn outputs() -> Vec<Output> {
    let mut salidas: Vec<Output> = chilpa_alsa::list()
        .into_iter()
        .map(|d| Output { id: d.id.clone(), label: d.label() })
        .collect();
    salidas.push(Output {
        id: chilpa_audio::CAPTURE_ID.to_string(),
        label: format!("Captura a archivo ({})", capture_path().display()),
    });
    salidas
}

/// Abre la biblioteca del usuario, creándola si hace falta.
pub fn open_library() -> rusqlite::Result<chilpa_library::Library> {
    chilpa_library::Library::open(&chilpa_library::Library::default_path())
}

/// Escanea una carpeta hacia la biblioteca, dejando las portadas en la
/// caché.
pub fn scan_into_library(
    lib: &chilpa_library::Library,
    root: &std::path::Path,
) -> rusqlite::Result<chilpa_library::ScanReport> {
    chilpa_library::scan_root_with_cache(lib, root, Some(&artwork_cache()))
}

/// Dónde se guardan las portadas que se sacan de los archivos.
pub fn artwork_cache() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("chilpa").join("artwork")
}

/// Dónde escribe la salida de captura.
pub fn capture_path() -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push("chilpa-capture.raw");
    p
}

#[cfg(not(target_os = "linux"))]
pub fn outputs() -> Vec<Output> {
    Vec::new()
}

/// Arranca un reproductor conectado a las salidas reales de la máquina.
#[cfg(target_os = "linux")]
pub fn spawn_player() -> player::Player {
    spawn_player_with(chilpa_audio::tap::Tap::new())
}

/// Lo mismo, mirando lo que sale por ese `tap`.
///
/// El tap envuelve al dispositivo: copia lo entregado y lo pasa tal cual.
/// Quien dibuja lee de ahí, y el audio no se entera.
#[cfg(target_os = "linux")]
pub fn spawn_player_with(tap: chilpa_audio::tap::Tap) -> player::Player {
    let del_hilo = tap.clone();
    player::Player::spawn(
        Box::new(|id| {
            if id == chilpa_audio::CAPTURE_ID {
                Ok(chilpa_audio::capture::capabilities())
            } else {
                chilpa_alsa::probe(id)
            }
        }),
        Box::new(move |id, formato| {
            let abierto: Box<dyn chilpa_audio::sink::AudioSink + Send> =
                if id == chilpa_audio::CAPTURE_ID {
                    Box::new(chilpa_audio::CaptureSink::create(&capture_path(), formato)?)
                } else {
                    Box::new(chilpa_alsa::AlsaSink::open(id, formato)?)
                };
            Ok(Box::new(del_hilo.wrap(abierto)) as Box<dyn chilpa_audio::sink::AudioSink + Send>)
        }),
    )
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
