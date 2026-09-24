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
pub mod backends;
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
    /// What a person recognises: "Fosi Audio ZH3".
    pub label: String,
    /// The second line: how it is attached and what it says it can do.
    pub detail: String,
    /// Whether samples reach it without anything in between.
    ///
    /// It is what the path is, not what was measured: Chilpa opens direct
    /// devices (`hw:`) and nothing else, so every real output here can carry
    /// a track untouched, and what it cannot do is refuse a format -- that
    /// is the wizard's answer, and it measures. The one output that is not
    /// bit-perfect is the capture, which writes what was handed to it.
    pub bit_perfect: bool,
}

/// Every output of every stack that is on this machine.
///
/// Flat, because a window that only offers one picker still has to offer
/// everything. The one that offers two -- the stack, and then its outputs --
/// asks `backends::outputs_of` instead and gets the same rows.
pub fn outputs() -> Vec<Output> {
    backends::all()
        .iter()
        .filter(|stack| stack.info().present)
        .flat_map(|stack| {
            let kind = stack.info().kind;
            backends::outputs_of(kind)
        })
        .map(|o| Output {
            id: o.id,
            label: o.label,
            detail: o.detail,
            bit_perfect: o.bit_perfect,
        })
        .collect()
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
    let Some(v) = chilpa_alsa::Volume::of(&bare(device)) else { return VolumeState::default() };
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
    match chilpa_alsa::Volume::of(&bare(device)) {
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
    match chilpa_alsa::Volume::of(&bare(device)) {
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
    measure(id)
}

/// What a stack calls an output, out of the identifier that names both.
///
/// An identifier written before stacks existed has no prefix, and reading it
/// as a bare name is right: it can only have meant the card, because that is
/// all there was.
#[cfg(target_os = "linux")]
fn bare(id: &str) -> String {
    chilpa_audio::backend::BackendOutput::split(id)
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| id.to_string())
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
        Box::new(|id| measure(id)),
        Box::new(move |id, format, exclusive| {
            let opened = open_output(id, format, exclusive)?;
            Ok(Box::new(from_thread.wrap(opened)) as Box<dyn chilpa_audio::sink::AudioSink + Send>)
        }),
    )
}

/// What an output really accepts, measured by opening it.
///
/// Only the stack that owns the identifier can answer, so the identifier is
/// resolved first. A name that belongs to no stack this build has is said so
/// plainly rather than tried against whichever one happens to be first,
/// which is how a settings file from another machine ends up playing
/// somewhere nobody chose.
#[cfg(target_os = "linux")]
fn measure(id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, chilpa_audio::sink::SinkError> {
    match chilpa_audio::backend::BackendOutput::split(id) {
        Some((chilpa_audio::backend::BackendKind::File, _)) => {
            Ok(chilpa_audio::capture::capabilities())
        }
        // A server takes whatever it is handed and makes it fit, so there
        // is nothing here that would refuse a format. What it does to the
        // samples on the way is the stack's `fidelity`, and that is said
        // where an output is chosen rather than smuggled in as a capability.
        //
        // Measuring for real would mean opening the sink, and opening one to
        // measure it takes it from whoever is using it -- which is exactly
        // what a listing must never do.
        // JACK is the one server that can be measured without taking
        // anything: the graph's rate is the graph's, it is published, and
        // every client is told it. Handing that over lets the negotiation
        // decide -- refuse with bit-perfect on, convert and say so with it
        // off -- instead of this crate deciding for it.
        Some((chilpa_audio::backend::BackendKind::Jack, _)) => chilpa_jack::capabilities()
            .ok_or_else(|| {
                chilpa_audio::sink::SinkError::Backend(chilpa_i18n::t(
                    "no JACK graph is running",
                ))
            }),
        Some((
            chilpa_audio::backend::BackendKind::PipeWire
            | chilpa_audio::backend::BackendKind::Pulse,
            _,
        )) => Ok(chilpa_audio::capture::capabilities()),
        Some((_, bare)) => chilpa_alsa::probe(bare),
        None => Err(unknown_output(id)),
    }
}

/// Opens one, through the stack its identifier names.
#[cfg(target_os = "linux")]
fn open_output(
    id: &str,
    format: chilpa_audio::format::StreamFormat,
    exclusive: bool,
) -> Result<Box<dyn chilpa_audio::sink::AudioSink + Send>, chilpa_audio::sink::SinkError> {
    let Some((stack, bare)) = backends::resolve(id) else {
        return Err(unknown_output(id));
    };
    // Whether to ask for the output alone is the user's, not this
    // function's. It used to ask always, which is right about fidelity and
    // wrong about everything else: somebody turns bit-perfect off precisely
    // so they can hear something else at the same time.
    stack.open(&bare, format, exclusive)
}

/// Whether the road past Chilpa leaves the samples alone.
///
/// A delivery only knows what Chilpa itself did. Handing samples to a server
/// that mixes and resamples leaves every transform empty, so a verdict built
/// on the delivery alone reads "bit-perfect" over a resampler -- which is
/// the exact claim this project exists to stop making, and which it made for
/// one afternoon before this existed.
///
/// An identifier that names no stack is not exact, because nothing is known
/// about it and the safe answer to "did the samples survive" is never yes.
pub fn path_is_exact(device: &str, exclusive: bool) -> bool {
    use chilpa_audio::backend::Fidelity;
    backends::resolve(device)
        .map(|(stack, _)| match stack.info().fidelity {
            Fidelity::Exact => true,
            // "It depends" depends on exactly one thing: whether the output
            // was asked for alone. A server hands the samples over when it
            // has nobody else to mix them with and resamples when it does,
            // so this is not a policy choice, it is what happened.
            Fidelity::Depends => exclusive,
            Fidelity::Never => false,
        })
        .unwrap_or(false)
}

/// The error for an identifier that names no stack this build has.
#[cfg(target_os = "linux")]
fn unknown_output(id: &str) -> chilpa_audio::sink::SinkError {
    chilpa_audio::sink::SinkError::Backend(chilpa_i18n::t_args(
        "there is no output called \"{}\" on this machine",
        &[id],
    ))
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
    player::Player::spawn(Box::new(|_| nothing_yet()), Box::new(|_, _, _| nothing_yet()))
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
                bit_perfect: played.delivery.nothing_was_converted(),
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
