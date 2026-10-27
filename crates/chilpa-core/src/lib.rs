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
pub mod output_evidence;
pub mod settings;
pub mod wizard;
pub mod queue_playback;
pub mod remote;
mod preload;
pub mod playback_events;
pub mod sacd_tracks;
pub mod sacd_xml;
pub mod seek_waveform;
pub mod loudness;
/// The HiFi Wizard: its configuration is PipeWire's, so it is Linux's.
#[cfg(target_os = "linux")]
pub mod dac_setup;
pub mod test_signal;

/// What goes out the device, for whoever wants to draw it.
pub use chilpa_audio::{spectrum, tap, visualizer};

/// Adds a facade event to the playback lifecycle trace.
pub fn trace_playback(scope: &str, event: &str, details: std::fmt::Arguments<'_>) {
    chilpa_audio::diagnostics::trace(scope, event, details);
}

/// A request made on a plugin's behalf. The caller supplies the credential,
/// never the plugin (ADR-007).
pub use chilpa_stream::{network, send_request};
pub use chilpa_stream::{send_worker_request, worker_url_allowed, WorkerNetworkError};
pub use chilpa_stream::download;
pub use chilpa_stream::{fetch, fetch_oauth, permits_authorization, Fetch, FetchError, Fetched};
pub use chilpa_stream::{call_server, Call, RangedRequest};

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
        .flat_map(|stack| backends::outputs_of(stack.kind()))
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
    let cache = artwork_cache();
    chilpa_library::scan::scan_root_with(lib, root, Some(&cache), &|_| {}, &sacd_tracks::DiscImages)
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

/// The volume control of that output, if it has one.
///
/// Asked of the stack the identifier names, like everything else. It used to
/// go straight to ALSA, which meant a Mac's own knob was invisible however
/// well CoreAudio reported it.
pub fn volume_of(device: &str) -> VolumeState {
    let Some((stack, bare)) = backends::resolve(device) else { return VolumeState::default() };
    let Some(knob) = stack.volume(&bare) else { return VolumeState::default() };
    VolumeState {
        available: true,
        percent: knob.percent,
        muted: knob.muted,
        can_mute: knob.can_mute,
        element: knob.name,
    }
}

#[cfg(target_os = "linux")]
pub use chilpa_alsa::volume::ControlChoice as AlsaHardwareVolumeChoice;

#[cfg(target_os = "linux")]
pub fn alsa_hardware_volume_choices(device: &str) -> Vec<AlsaHardwareVolumeChoice> {
    device.strip_prefix("alsa:").map(chilpa_alsa::volume::choices).unwrap_or_default()
}

#[cfg(target_os = "linux")]
pub fn select_alsa_hardware_volume(device: &str, choice: &str) -> Result<(), String> {
    let bare = device.strip_prefix("alsa:").ok_or("Choose a direct ALSA output first.")?;
    chilpa_alsa::volume::select(bare, choice)
}

#[cfg(target_os = "linux")]
pub fn activate_alsa_hardware_volume(device: &str) -> Result<(), String> {
    let bare = device.strip_prefix("alsa:").ok_or("Choose a direct ALSA output first.")?;
    chilpa_alsa::volume::activate(bare)
}

/// Moves it.
pub fn set_volume(device: &str, percent: u8) -> Result<(), String> {
    let Some((stack, bare)) = backends::resolve(device) else {
        return Err(chilpa_i18n::t("this output has no volume control of its own"));
    };
    stack.set_volume(&bare, percent)
}

/// Mutes or unmutes it.
pub fn set_muted(device: &str, muted: bool) -> Result<(), String> {
    let Some((stack, bare)) = backends::resolve(device) else {
        return Err(chilpa_i18n::t("this output has no volume control of its own"));
    };
    stack.set_muted(&bare, muted)
}

/// Measures an output device.
///
/// It is the same thing the player uses to negotiate; the wizard calls it
/// separately so it can say what is possible before playing anything.
#[cfg(target_os = "linux")]
pub fn probe_device(id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, chilpa_audio::sink::SinkError> {
    measure(id)
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
pub fn spawn_player() -> player::Player {
    spawn_player_with(chilpa_audio::tap::Tap::new())
}

/// The same, watching what goes out through that `tap`.
///
/// The tap wraps the device: it copies what is delivered and passes it
/// through. Whoever draws reads from there, and the audio never finds out.
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

/// What an output really accepts.
///
/// One line, because the question belongs to the stack and every one of them
/// answers it differently: a card by being opened, a server by taking
/// whatever it is handed, a JACK graph by publishing its one rate, a Mac by
/// saying what its wire carries. This used to be a match over all of them
/// here, with a cfg for the platform each needed.
fn measure(id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, chilpa_audio::sink::SinkError> {
    let _lease = output_evidence::lease::playback()?;
    let Some((stack, bare)) = backends::resolve(id) else {
        return Err(unknown_output(id));
    };
    stack.caps(&bare)
}

/// Opens one, through the stack its identifier names.
fn open_output(
    id: &str,
    format: chilpa_audio::format::StreamFormat,
    exclusive: bool,
) -> Result<Box<dyn chilpa_audio::sink::AudioSink + Send>, chilpa_audio::sink::SinkError> {
    let lease = output_evidence::lease::playback()?;
    let Some((stack, bare)) = backends::resolve(id) else {
        return Err(unknown_output(id));
    };
    // Whether to ask for the output alone is the user's, not this
    // function's. It used to ask always, which is right about fidelity and
    // wrong about everything else: somebody turns bit-perfect off precisely
    // so they can hear something else at the same time.
    stack.open(&bare, format, exclusive).map(|sink|output_evidence::lease::wrap(sink,lease))
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

/// Whether the output, as it stands now that a track opened, carries what
/// the delivery opened untouched: the half of the verdict that only the
/// device can answer, read after the fact. See
/// `OutputBackend::carries_untouched`.
pub fn wire_keeps(device: &str, delivery: &chilpa_audio::negotiate::Delivery) -> bool {
    backends::resolve(device).is_some_and(|(stack, bare)| {
        stack.carries_untouched(&bare, delivery.opened.sample_rate_hz, delivery.bits_carried())
    })
}

/// The error for an identifier that names no stack this build has.
fn unknown_output(id: &str) -> chilpa_audio::sink::SinkError {
    chilpa_audio::sink::SinkError::Backend(chilpa_i18n::t_args(
        "there is no output called \"{}\" on this machine",
        &[id],
    ))
}

/// What the core tells the facade when something has finished playing.
pub struct PlayedSummary {
    pub frames: u64,
    pub quality: chilpa_models::AudioQuality,
    pub bit_perfect: bool,
}

impl Core {
    /// Plays one file and waits for it: decode, measure the output,
    /// negotiate and deliver. The error comes out already written for the
    /// window, because the facade does not know the types in here.
    ///
    /// Through the catalogue like everything else. It spoke straight to ALSA
    /// before, which is why it existed only on Linux and why every other
    /// platform had a stub that refused. It now works wherever a stack does,
    /// which is the point of having stacks.
    pub fn play_local(&mut self, path: &std::path::Path, device: &str, _legacy_bit_perfect: bool) -> Result<PlayedSummary, String> {
        use chilpa_audio::negotiate::Policy;
        use chilpa_audio::render::Outcome;
        use chilpa_audio::sink::StopSignal;

        let caps = measure(device).map_err(|e| e.to_string())?;
        let policy = Policy::AllowAnything;
        let played = playback::play_file(path, &caps, policy, &StopSignal::new(), |format| {
            open_output(device, format, false)
        })
        .map_err(|e| e.to_string())?;
        let opened = played.delivery.opened;
        let quality = chilpa_models::AudioQuality {
            codec: format!("{:?}", opened.sample_format).to_lowercase(),
            bit_depth: played.delivery.bits_carried(),
            sample_rate_hz: opened.sample_rate_hz,
            channels: opened.channels as u8,
        };
        match played.outcome {
            Outcome::Finished { frames } | Outcome::Stopped { frames } => Ok(PlayedSummary {
                frames,
                quality,
                // Both halves, as the player says it. The delivery knows what
                // Chilpa did and nothing about the road after it, so on its
                // own this called a JACK graph or a shared server exact.
                bit_perfect: played.delivery.nothing_was_converted()
                    && path_is_exact(device, false),
            }),
            Outcome::Failed { frames, error } => {
                Err(chilpa_i18n::t_args(
                    "the device failed after {} frames: {}",
                    &[&frames.to_string(), &error.to_string()],
                ))
            }
            Outcome::Unreadable { reason, .. } => Err(reason),
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
pub use chilpa_audio::dsp;
