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
        }
    }

    /// The one that key names, if it names one.
    pub fn from_key(key: &str) -> Option<BackendKind> {
        BackendKind::ALL.iter().copied().find(|k| k.key() == key)
    }

    /// What it is called in the window. A stack's name is its name in every
    /// language, so this is not a msgid.
    pub fn name(&self) -> &'static str {
        match self {
            BackendKind::System => "System",
            BackendKind::Alsa => "ALSA",
            BackendKind::PipeWire => "PipeWire",
            BackendKind::Pulse => "PulseAudio",
            BackendKind::Jack => "JACK",
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

/// A stack Chilpa can hand samples to.
pub trait OutputBackend: Send + Sync {
    /// What it is, without opening anything.
    fn info(&self) -> BackendInfo;

    /// What it offers. Never opens a device.
    ///
    /// Empty is a legitimate answer and is not an error: a backend that is
    /// there but has nothing plugged into it has nothing to list.
    fn outputs(&self) -> Vec<BackendOutput>;

    /// Opens one. The only call here that touches the hardware.
    fn open(&self, id: &str, format: StreamFormat) -> Result<Box<dyn AudioSink>, SinkError>;
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
