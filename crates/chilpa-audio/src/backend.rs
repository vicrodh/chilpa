//! Which audio stack the samples go through, and what it can promise.
//!
//! A machine usually offers several: the sound server everything else uses,
//! the hardware underneath it, and one or two more that a server is wearing
//! as a hat. They are not interchangeable. One hands the samples to the DAC
//! as they left the file; another mixes them with a notification sound and
//! resamples the result to whatever rate the graph happens to run at. Both
//! play music. Only one of them plays *the* music.
//!
//! So the backend is a choice, and it is the choice that decides everything
//! after it: which outputs exist, what they are called, and whether
//! bit-perfect is even on the table. That is why it comes first in the
//! window and why it lives here rather than inside one platform's crate.
//!
//! Three things are kept apart on purpose:
//!
//! 1. **Listing** never opens anything. A listing that opens devices takes
//!    one away from whoever is using it, and this one runs while music is
//!    playing.
//! 2. **Opening** is the only thing that touches the hardware.
//! 3. **Declaring** is what the backend says about itself before either:
//!    whether it is even there, who is really answering for it, and what it
//!    can promise about the samples.

use crate::format::StreamFormat;
use crate::sink::{AudioSink, SinkError};

/// The stacks Chilpa knows how to hand samples to.
///
/// Not every one of them exists on every machine, and `BackendInfo::present`
/// is what says so. The list is the same everywhere so that a preference
/// written on one machine still reads on another.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BackendKind {
    /// Whatever the system has decided is the output. The one that always
    /// works and never promises anything.
    System,
    /// The card, addressed directly. Nothing between the samples and it.
    Alsa,
    /// The PipeWire graph.
    PipeWire,
    /// The PulseAudio protocol, whoever is answering it.
    Pulse,
    /// The JACK graph, whoever is answering it.
    Jack,
    /// A Mac's own audio.
    CoreAudio,
    /// A Windows endpoint, reached through its own session API.
    Wasapi,
    /// A file on disk. It makes no sound, and it is the only output whose
    /// result can be compared byte for byte against the source.
    File,
}

impl BackendKind {
    /// Every one, in the order the window offers them: the safe one first,
    /// then the one that can be exact, then the servers.
    pub const ALL: &'static [BackendKind] = &[
        BackendKind::System,
        BackendKind::Alsa,
        BackendKind::PipeWire,
        BackendKind::Pulse,
        BackendKind::Jack,
        BackendKind::CoreAudio,
        BackendKind::Wasapi,
        BackendKind::File,
    ];

    /// What it is called in a settings file. It is written to disk, so it is
    /// not translated and it does not change.
    pub fn key(&self) -> &'static str {
        match self {
            BackendKind::System => "system",
            BackendKind::Alsa => "alsa",
            BackendKind::PipeWire => "pipewire",
            BackendKind::Pulse => "pulse",
            BackendKind::Jack => "jack",
            BackendKind::CoreAudio => "coreaudio",
            BackendKind::Wasapi => "wasapi",
            BackendKind::File => "file",
        }
    }

    /// The one that key names, if it names one.
    pub fn from_key(key: &str) -> Option<BackendKind> {
        BackendKind::ALL.iter().copied().find(|k| k.key() == key)
    }

    /// What it is called in the window.
    ///
    /// Most of these are the same word in every language, because they are
    /// the names projects gave themselves: ALSA is ALSA in Spanish. Two are
    /// not. `System` and `File` are ordinary English words that happen to
    /// sit in the same list, and leaving them out of the catalog because
    /// their neighbours belong there is how the picker ended up half
    /// translated.
    pub fn name(&self) -> String {
        match self {
            BackendKind::System => chilpa_i18n::t("System"),
            BackendKind::Alsa => "ALSA".to_string(),
            BackendKind::PipeWire => "PipeWire".to_string(),
            BackendKind::Pulse => "PulseAudio".to_string(),
            BackendKind::Jack => "JACK".to_string(),
            BackendKind::CoreAudio => "CoreAudio".to_string(),
            BackendKind::Wasapi => "WASAPI".to_string(),
            BackendKind::File => chilpa_i18n::t("File"),
        }
    }
}

/// What a backend can promise about the samples that go through it.
///
/// Three answers and not two, because "it depends" is the true answer for a
/// sound server and rounding it either way is a lie. A server that is set up
/// for passthrough is exact; the same server with a different configuration
/// resamples, and neither Chilpa nor the user can tell from here which one
/// this machine has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fidelity {
    /// The samples arrive as they left. Nothing is between.
    Exact,
    /// They can, if the server is set up for it. Chilpa cannot tell from
    /// here, so it says so instead of guessing.
    Depends,
    /// They cannot, and `BackendInfo::caveat` says why and what to use
    /// instead.
    Never,
}

/// What a backend is, before anything has been opened.
///
/// All of it is read or asked for, never assumed. The one that cost a
/// mistake is `present`: a stack can be there without its usual daemon --
/// on the machine this was written on there is no `jackd` and JACK works,
/// because PipeWire provides the JACK client library. So `present` is
/// answered by asking for what the backend actually needs, and `provider`
/// says who turned out to be answering.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BackendInfo {
    pub kind: BackendKind,
    /// Whether it can be used on this machine right now.
    pub present: bool,
    /// When it cannot: what is missing, in words somebody can act on.
    pub missing: String,
    /// Who is really answering, when it is not the stack itself: on a
    /// machine where PipeWire wears the JACK hat, choosing JACK does not
    /// get a shorter path, and saying nothing would let somebody believe it
    /// did.
    pub provider: String,
    pub fidelity: Fidelity,
    /// When the fidelity is not `Exact`: why, and what to use instead.
    pub caveat: String,
}

impl BackendInfo {
    /// One that is not on this machine, with the reason.
    pub fn missing(kind: BackendKind, why: String) -> Self {
        BackendInfo {
            kind,
            present: false,
            missing: why,
            provider: String::new(),
            fidelity: Fidelity::Never,
            caveat: String::new(),
        }
    }

    /// How it reads in the picker: "JACK", or "JACK (through PipeWire)".
    pub fn label(&self) -> String {
        if self.provider.is_empty() {
            self.kind.name().to_string()
        } else {
            format!("{} ({})", self.kind.name(), through(&self.provider))
        }
    }
}

/// "through PipeWire", translated where the window will read it.
fn through(provider: &str) -> String {
    chilpa_i18n::t_args("through {}", &[provider])
}

/// One output a backend offers.
///
/// The identifier means whatever the backend it came from means by it, and
/// nobody else reads it. What everyone else reads is the label, and it is
/// the card's name and not the driver's: every USB DAC in the world calls
/// its device "USB Audio", so a list built on driver names shows a DAC, a
/// dock and a webcam as three lines that read the same.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BackendOutput {
    pub id: String,
    /// What a person recognises: "Fosi Audio ZH3".
    pub label: String,
    /// The line under it: how it is attached, what it says it can do.
    pub detail: String,
    /// Whether samples reach this one untouched. It is about the path and
    /// not about a measurement: what a device refuses to accept is a
    /// different question, and answering it means opening the device.
    pub bit_perfect: bool,
}

impl BackendOutput {
    /// The identifier as everything outside a backend uses it: the stack's
    /// key, a colon, and whatever that stack calls the output.
    ///
    /// One string that identifies an output completely. It is what gets
    /// written to the settings, and it is why a remembered output that is no
    /// longer there can still say which stack it belonged to instead of
    /// quietly reading as "system".
    pub fn full_id(kind: BackendKind, id: &str) -> String {
        format!("{}:{}", kind.key(), id)
    }

    /// The stack and the bare identifier inside one of those.
    pub fn split(full: &str) -> Option<(BackendKind, &str)> {
        let (key, rest) = full.split_once(':')?;
        BackendKind::from_key(key).map(|kind| (kind, rest))
    }
}

/// What an output's own volume control is doing.
///
/// Its own, and that word is load-bearing: a volume applied by multiplying
/// the samples is not a volume, it is a conversion, and it is the end of
/// bit-perfect. A stack that cannot reach a real knob says so and Chilpa
/// leaves the knob to the person.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Knob {
    /// Where it sits, from 0 to 100.
    pub percent: u8,
    pub muted: bool,
    /// Whether it can be muted at all.
    pub can_mute: bool,
    /// Which control it is, as the device names it. Worth showing when the
    /// knob does not do what somebody expected.
    pub name: String,
}

/// A stack Chilpa can hand samples to.
pub trait OutputBackend: Send + Sync {
    /// Which stack it is: a constant, and all anybody needs to tell stacks
    /// apart. `info` is not that. Finding out whether a server is there can
    /// mean connecting to it, and looking a stack up by asking every one for
    /// its `info` opened and closed a JACK client on the way to the file
    /// stack, on whatever thread was looking -- the player's, once, forever.
    fn kind(&self) -> BackendKind;

    /// What it is, without opening anything.
    fn info(&self) -> BackendInfo;

    /// What it offers. Never opens a device.
    ///
    /// Empty is a legitimate answer and is not an error: a backend that is
    /// there but has nothing plugged into it has nothing to list.
    fn outputs(&self) -> Vec<BackendOutput>;

    /// What that output accepts.
    ///
    /// The one question whose answer differs most between stacks. A card can
    /// be asked by opening it; a server takes whatever it is handed and
    /// makes it fit, so there is nothing to refuse; a JACK graph has exactly
    /// one rate and publishes it. Each says its own, and `DeviceCaps::probed`
    /// is where the difference between measured and assumed is recorded --
    /// guessed caps cannot promise bit-perfect.
    fn caps(&self, id: &str) -> Result<crate::negotiate::DeviceCaps, SinkError>;

    /// That output's own volume control, if it has one.
    ///
    /// `None` is the common answer and not a failure: a USB DAC with a knob
    /// on the front usually exposes nothing, and the right thing then is to
    /// say so rather than to invent a volume by scaling the samples.
    fn volume(&self, _id: &str) -> Option<Knob> {
        None
    }

    /// Moves it. The default is the honest one for a stack with no control:
    /// there is nothing to move.
    fn set_volume(&self, _id: &str, _percent: u8) -> Result<(), String> {
        Err(chilpa_i18n::t("this output has no volume control of its own"))
    }

    /// Mutes it, or takes it off mute.
    fn set_muted(&self, _id: &str, _muted: bool) -> Result<(), String> {
        Err(chilpa_i18n::t("this output has no volume control of its own"))
    }

    /// Something short that changes when the set of outputs changes.
    ///
    /// The window needs to notice a DAC being plugged in without asking
    /// `outputs()` sixty times a minute, and `outputs()` is not always cheap:
    /// a sound server is asked over a socket, with a timeout, and calling
    /// that on a timer would stall the very window it is meant to keep
    /// current. So a stack that can answer cheaply answers here, and the
    /// caller only does the real work when this string moves.
    ///
    /// **An empty answer means "do not poll me", not "nothing changed".** A
    /// stack that cannot tell without doing the expensive thing says nothing
    /// and is left alone. On Linux that costs nothing: sound servers hand out
    /// what the cards offer, so the cards changing is the event, and ALSA can
    /// see that in one small file.
    ///
    /// This is polling, and polling is not the right answer. Every one of
    /// these platforms can say when a device arrives -- udev on Linux, a
    /// property listener on CoreAudio, `IMMNotificationClient` on Windows --
    /// and each of those is a thread with a callback in it. This is the cheap
    /// half of that, written down as the cheap half.
    fn generation(&self) -> String {
        String::new()
    }

    /// Opens one. The only call here that touches the hardware.
    ///
    /// `Send` because playing happens on its own thread: whoever opens the
    /// device is not who writes to it. A sink that cannot move there cannot
    /// be played through, so the trait asks for it rather than leaving it to
    /// be discovered at the call site.
    /// `exclusive` asks for the output to itself, which is what makes a
    /// sound server able to pass samples through untouched instead of
    /// mixing them. A stack that has nothing like it ignores it; one that
    /// does is the difference between `Depends` being kept and not.
    fn open(&self, id: &str, format: StreamFormat, exclusive: bool)
        -> Result<Box<dyn AudioSink + Send>, SinkError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_survives_a_round_trip_and_an_unknown_one_is_not_invented() {
        for kind in BackendKind::ALL {
            assert_eq!(BackendKind::from_key(kind.key()), Some(*kind), "{}", kind.key());
        }
        assert_eq!(BackendKind::from_key("oss"), None);
        assert_eq!(BackendKind::from_key(""), None);
    }

    #[test]
    fn an_identifier_says_which_stack_it_came_from() {
        // A bare "hw:1,0" does not: read back from a settings file it could
        // belong to any of them, and the one it actually belonged to is
        // exactly what somebody needs to be told when it is gone.
        let full = BackendOutput::full_id(BackendKind::Alsa, "hw:1,0");
        assert_eq!(full, "alsa:hw:1,0");
        assert_eq!(BackendOutput::split(&full), Some((BackendKind::Alsa, "hw:1,0")));
        // The rest is handed back whole, colons and all.
        assert_eq!(
            BackendOutput::split("pulse:alsa_output.usb-Fosi:0"),
            Some((BackendKind::Pulse, "alsa_output.usb-Fosi:0"))
        );
        assert_eq!(BackendOutput::split("hw:1,0"), None, "no stack, no answer");
        assert_eq!(BackendOutput::split("oss:dsp"), None);
    }

    #[test]
    fn the_keys_are_not_the_names() {
        // The key goes to disk and the name goes on screen. Tying them
        // together is how renaming something in the window silently
        // invalidates everybody's settings file.
        assert_eq!(BackendKind::Pulse.key(), "pulse");
        assert_eq!(BackendKind::Pulse.name(), "PulseAudio");
    }

    #[test]
    fn a_backend_says_who_is_really_answering() {
        let plain = BackendInfo {
            kind: BackendKind::Jack,
            present: true,
            missing: String::new(),
            provider: String::new(),
            fidelity: Fidelity::Never,
            caveat: String::new(),
        };
        assert_eq!(plain.label(), "JACK");

        let worn = BackendInfo { provider: "PipeWire".into(), ..plain };
        assert!(worn.label().starts_with("JACK ("), "{}", worn.label());
        assert!(worn.label().contains("PipeWire"), "{}", worn.label());
    }

    #[test]
    fn one_that_is_not_there_carries_its_reason_and_promises_nothing() {
        let absent = BackendInfo::missing(BackendKind::Jack, "no JACK client library".into());
        assert!(!absent.present);
        assert_eq!(absent.fidelity, Fidelity::Never);
        assert!(!absent.missing.is_empty(), "it has to say what is missing");
    }
}
