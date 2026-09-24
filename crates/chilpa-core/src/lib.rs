//! Chilpa's headless core: library, player, sources and plugins.
//!
//! No frontend talks to this crate. The only public door is `chilpa-api`
//! (ADR-001); the implementation lives in here.

#[cfg(target_os = "linux")]
pub mod analysis_job;
pub mod cd_playback;
pub mod dsd_playback;
pub mod playback;
pub mod player;
pub mod settings;
pub mod wizard;
pub mod queue_playback;

/// What goes out the device, for whoever wants to draw it.
pub use chilpa_audio::{spectrum, tap};

/// A request made on a plugin's behalf. The caller supplies the credential,
/// never the plugin (ADR-007).
pub use chilpa_stream::send_request;

use chilpa_models::SourceId;

/// The process's state, empty for now. It grows as the ported crates
/// arrive.
#[derive(Default)]
pub struct Core {
    sources: Vec<SourceId>,
}

impl Core {
    pub fn new() -> Self {
        Self { sources: vec![SourceId::local()] }
    }

    /// The registered sources, in the order they were added.
    pub fn sources(&self) -> &[SourceId] {
        &self.sources
    }
}

/// An available output, as the facade sees it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Output {
    pub id: String,
    pub label: String,
}

/// The machine's outputs, plus the diagnostic one.
///
/// The capture goes last and is named plainly: it is for exercising the
/// whole path without making a sound, and for checking bit-perfect against
/// the file it produces.
#[cfg(target_os = "linux")]
pub fn outputs() -> Vec<Output> {
    let mut outputs: Vec<Output> = chilpa_alsa::list()
        .into_iter()
        .map(|d| Output { id: d.id.clone(), label: d.label() })
        .collect();
    outputs.push(Output {
        id: chilpa_audio::CAPTURE_ID.to_string(),
        label: format!("Captura a archivo ({})", capture_path().display()),
    });
    outputs
}

/// Opens the user's library, creating it if needed.
pub fn open_library() -> rusqlite::Result<chilpa_library::Library> {
    chilpa_library::Library::open(&chilpa_library::Library::default_path())
}

/// Scans a folder into the library, leaving the covers in the cache.
pub fn scan_into_library(
    lib: &chilpa_library::Library,
    root: &std::path::Path,
) -> rusqlite::Result<chilpa_library::ScanReport> {
    chilpa_library::scan_root_with_cache(lib, root, Some(&artwork_cache()))
}

/// Where the covers pulled out of files are kept.
pub fn artwork_cache() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("chilpa").join("artwork")
}

/// Where the capture output writes.
pub fn capture_path() -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push("chilpa-capture.raw");
    p
}

#[cfg(not(target_os = "linux"))]
pub fn outputs() -> Vec<Output> {
    Vec::new()
}

/// Measures an output device.
///
/// It is the same thing the player uses to negotiate; the wizard calls it
/// separately so it can say what is possible before playing anything.
#[cfg(target_os = "linux")]
pub fn probe_device(id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, chilpa_audio::sink::SinkError> {
    if id == chilpa_audio::CAPTURE_ID {
        Ok(chilpa_audio::capture::capabilities())
    } else {
        chilpa_alsa::probe(id)
    }
}

/// Starts a player wired to the machine's real outputs.
#[cfg(target_os = "linux")]
pub fn spawn_player() -> player::Player {
    spawn_player_with(chilpa_audio::tap::Tap::new())
}

/// The same, watching what goes out through that `tap`.
///
/// The tap wraps the device: it copies what is delivered and passes it
/// through. Whoever draws reads from there, and the audio never finds out.
#[cfg(target_os = "linux")]
pub fn spawn_player_with(tap: chilpa_audio::tap::Tap) -> player::Player {
    let from_thread = tap.clone();
    player::Player::spawn(
        Box::new(|id| {
            if id == chilpa_audio::CAPTURE_ID {
                Ok(chilpa_audio::capture::capabilities())
            } else {
                chilpa_alsa::probe(id)
            }
        }),
        Box::new(move |id, format| {
            let opened: Box<dyn chilpa_audio::sink::AudioSink + Send> =
                if id == chilpa_audio::CAPTURE_ID {
                    Box::new(chilpa_audio::CaptureSink::create(&capture_path(), format)?)
                } else {
                    Box::new(chilpa_alsa::AlsaSink::open(id, format)?)
                };
            Ok(Box::new(from_thread.wrap(opened)) as Box<dyn chilpa_audio::sink::AudioSink + Send>)
        }),
    )
}

/// What the core tells the facade when something has finished playing.
pub struct PlayedSummary {
    pub frames: u64,
    pub quality: chilpa_models::AudioQuality,
    pub bit_perfect: bool,
}

impl Core {
    /// Plays a local file on a direct ALSA device.
    ///
    /// It is the vertical slice's minimum path: decode, measure the
    /// device, negotiate and deliver. The error comes out already written for
    /// the UI, because the facade does not know the types in here.
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
    fn the_core_starts_with_the_local_library() {
        let core = Core::new();
        assert_eq!(core.sources(), &[SourceId::local()]);
    }
}
