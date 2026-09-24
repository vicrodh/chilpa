//! Chilpa's headless core: library, player, sources and plugins.
//!
//! No frontend talks to this crate. The only public door is `chilpa-api`
//! (ADR-001); the implementation lives in here.

pub mod analysis_job;
// A CD drive is read through `chilpa-cd`, which is ioctls and therefore
// Linux. The module follows its dependency; everything else in this crate
// does not, and a `cfg` here on anything that has no reason for one is what
// ADR-010 calls a lie with a date on it.
#[cfg(target_os = "linux")]
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
        label: chilpa_i18n::t_args("Capture to a file ({})", &[&capture_path().display().to_string()]),
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
    chilpa_dirs::cache().join("artwork")
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

/// What a device's own volume control is doing.
///
/// `None` means the device has no control of its own, which is the common
/// case on a USB DAC with a knob. Chilpa does not make one up: a volume
/// applied by multiplying the samples is not a volume, it is a conversion,
/// and it is the end of bit-perfect.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct VolumeState {
    /// True if the device has a control at all.
    pub available: bool,
    /// Where it sits, from 0 to 100.
    pub percent: u8,
    /// True if it is muted.
    pub muted: bool,
    /// True if it can be muted at all.
    pub can_mute: bool,
    /// Which control it is, as the card names it: `Master`, `PCM`... Worth
    /// showing when the knob does not do what someone expected.
    pub element: String,
}

/// The volume control of that device, if it has one.
#[cfg(target_os = "linux")]
pub fn volume_of(device: &str) -> VolumeState {
    let Some(v) = chilpa_alsa::Volume::of(device) else { return VolumeState::default() };
    let level = v.read().unwrap_or_default();
    VolumeState {
        available: true,
        percent: level.percent,
        muted: level.muted,
        can_mute: v.can_mute(),
        element: v.element().to_string(),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn volume_of(_device: &str) -> VolumeState {
    VolumeState::default()
}

/// Moves that device's own volume control.
#[cfg(target_os = "linux")]
pub fn set_volume(device: &str, percent: u8) -> Result<(), String> {
    match chilpa_alsa::Volume::of(device) {
        Some(v) => v.set(percent),
        None => Err(chilpa_i18n::t("this output has no volume control of its own")),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_volume(_device: &str, _percent: u8) -> Result<(), String> {
    Err(chilpa_i18n::t("this output has no volume control of its own"))
}

/// Mutes or unmutes that device.
#[cfg(target_os = "linux")]
pub fn set_muted(device: &str, muted: bool) -> Result<(), String> {
    match chilpa_alsa::Volume::of(device) {
        Some(v) => v.mute(muted),
        None => Err(chilpa_i18n::t("this output has no volume control of its own")),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_muted(_device: &str, _muted: bool) -> Result<(), String> {
    Err(chilpa_i18n::t("this output has no volume control of its own"))
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

/// The same where there is no backend yet.
///
/// ADR-010 deuda M2: Windows, macOS and Android have no output backend, so
/// there is nothing to measure. It says so and the facade goes on
/// compiling; what it must never do is not exist, because then the shell
/// stops building and the platform quietly leaves the workspace.
#[cfg(not(target_os = "linux"))]
pub fn probe_device(
    _id: &str,
) -> Result<chilpa_audio::negotiate::DeviceCaps, chilpa_audio::sink::SinkError> {
    Err(chilpa_audio::sink::SinkError::Backend(chilpa_i18n::t(
        "there is no audio output on this platform yet",
    )))
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

/// The same where there is no backend yet (ADR-010 deuda M2).
///
/// A real player over a device that refuses to open: the queue, the signals
/// and the events all work, and what fails is the one thing that actually is
/// missing. That is what makes the rest of the engine testable on a platform
/// whose audio nobody has written.
#[cfg(not(target_os = "linux"))]
pub fn spawn_player_with(tap: chilpa_audio::tap::Tap) -> player::Player {
    let _ = tap;
    fn nothing_yet<T>() -> Result<T, chilpa_audio::sink::SinkError> {
        Err(chilpa_audio::sink::SinkError::Backend(chilpa_i18n::t(
            "there is no audio output on this platform yet",
        )))
    }
    player::Player::spawn(Box::new(|_| nothing_yet()), Box::new(|_, _| nothing_yet()))
}

/// The same where there is no backend yet (ADR-010 deuda M2).
#[cfg(not(target_os = "linux"))]
pub fn spawn_player() -> player::Player {
    spawn_player_with(chilpa_audio::tap::Tap::new())
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
    /// The same where there is no backend yet (ADR-010 deuda M2).
    #[cfg(not(target_os = "linux"))]
    pub fn play_local(
        &mut self,
        _path: &std::path::Path,
        _device: &str,
        _bit_perfect: bool,
    ) -> Result<PlayedSummary, String> {
        Err(chilpa_i18n::t("there is no audio output on this platform yet"))
    }

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
                Err(chilpa_i18n::t_args(
                    "the device failed after {} frames: {}",
                    &[&frames.to_string(), &format!("{error:?}")],
                ))
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
