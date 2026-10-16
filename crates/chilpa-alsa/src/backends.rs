//! The stacks ALSA itself answers for: the card, the card through the
//! plugin that converts, and whatever the system has decided.
//!
//! They share every line of code that touches the hardware and differ in
//! what they name: `hw:` is the card and nothing else, `plughw:` is the card
//! with ALSA converting whatever it will not take, and `default` is wherever
//! the machine is currently sending sound. On a desktop that is usually a
//! sound server, so the samples pass through a mixer on their way out. It
//! always works and it promises nothing.

use crate::{devices, AlsaSink};
use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, Fidelity, Knob, OutputBackend};
use chilpa_audio::format::StreamFormat;
use chilpa_audio::sink::{AudioSink, SinkError};

/// The card, addressed directly.
pub struct Alsa;

/// The card through ALSA's `plughw` plugin.
///
/// The same card as [`Alsa`], with the plugin that converts what the card
/// will not take: a rate, a sample format, a channel count. That is what it
/// is for -- a DAC that only takes 32 bit words plays a 16 bit file with
/// nobody packing it -- and it is also why it can promise nothing: what it
/// does to the samples depends on what the card refuses, and it does it
/// without saying.
pub struct AlsaPlug;

/// The card through ALSA's own PCM layer: `sysdefault:CARD=N`.
///
/// What ALSA's configuration makes of the card -- on most cards a plug over
/// dmix -- so other programs that speak ALSA can play at the same time, and
/// what the card will not take is converted. The card itself, where `System`
/// is wherever the machine sends sound. It is QBZ's "pcm" plugin, and like
/// plughw it promises nothing about the samples.
pub struct AlsaPcm;

/// Whatever the system is using.
pub struct System;

/// The card's own mixer, when it has one. The same for every stack here:
/// they all end at a card.
fn card_knob(id: &str) -> Option<Knob> {
    let control = crate::Volume::of(id)?;
    let level = control.read().ok()?;
    Some(Knob { percent: level.percent, muted: level.muted, can_mute: control.can_mute(), name: control.element().to_string() })
}

fn no_knob() -> String {
    chilpa_i18n::t("this output has no volume control of its own")
}

impl OutputBackend for Alsa {
    fn kind(&self) -> BackendKind {
        BackendKind::Alsa
    }

    fn info(&self) -> BackendInfo {
        // What this backend needs is a card. Not a daemon, not a library
        // that might be a stand-in for another: a card, and they are listed
        // in a file.
        let cards = devices::list();
        if cards.is_empty() {
            return BackendInfo::missing(
                BackendKind::Alsa,
                chilpa_i18n::t("no sound card is listed in /proc/asound"),
            );
        }
        BackendInfo {
            kind: BackendKind::Alsa,
            present: true,
            missing: String::new(),
            provider: String::new(),
            fidelity: Fidelity::Exact,
            caveat: String::new(),
        }
    }

    fn outputs(&self) -> Vec<BackendOutput> {
        let mut found = devices::list();
        // What somebody plugged in on purpose, above what is soldered to the
        // board. Sorted on the bus and not on the line the window shows: that
        // line is in the reader's language now, so `starts_with("USB")` would
        // have held only while the window spoke English.
        found.sort_by_key(|d| d.bus != devices::Bus::Usb);
        found
            .into_iter()
            .map(|d| BackendOutput {
                id: d.id.clone(),
                label: d.label(),
                detail: d.detail(),
                // Every output this backend offers is a `hw:` device, which
                // is what "direct" means. It does not follow that any format
                // will play: what the device refuses is a different question
                // and the wizard answers it by measuring.
                bit_perfect: true,
            })
            .collect()
    }

    /// Every card and every device on it, which is what changes when a DAC is
    /// plugged in or taken out.
    ///
    /// `/proc/asound/pcm` is one line per device -- 350 bytes on this machine
    /// -- and the kernel rewrites it the moment a card appears or goes. Read
    /// once a second it costs nothing, and it is the one watch that covers
    /// every stack on this platform: PipeWire, PulseAudio and JACK hand out
    /// what the cards offer, so a card arriving is their event too.
    fn generation(&self) -> String {
        std::fs::read_to_string("/proc/asound/pcm").unwrap_or_default()
    }

    /// Measured by opening it, which is the only thing that can promise
    /// bit-perfect.
    fn caps(&self, id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, SinkError> {
        crate::probe(id)
    }

    fn volume(&self, id: &str) -> Option<Knob> {
        card_knob(id)
    }

    fn set_volume(&self, id: &str, percent: u8) -> Result<(), String> {
        crate::Volume::of(id).ok_or_else(no_knob)?.set(percent)
    }

    fn set_muted(&self, id: &str, muted: bool) -> Result<(), String> {
        crate::Volume::of(id).ok_or_else(no_knob)?.mute(muted)
    }

    fn open(&self, id: &str, format: StreamFormat, _exclusive: bool)
        -> Result<Box<dyn AudioSink + Send>, SinkError> {
        Ok(Box::new(AlsaSink::open(id, format)?))
    }
}

impl OutputBackend for AlsaPlug {
    fn kind(&self) -> BackendKind {
        BackendKind::AlsaPlug
    }

    fn info(&self) -> BackendInfo {
        // Here when the cards are, missing for the same reason they are.
        let mut info = BackendInfo { kind: BackendKind::AlsaPlug, ..Alsa.info() };
        if info.present {
            info.fidelity = Fidelity::Never;
            info.caveat = chilpa_i18n::t(
                "plughw converts whatever the card does not take -- the rate, the sample format, \
                 the channels -- without saying so. It plays what the card alone would refuse, and \
                 for the same reason it cannot promise the samples arrive as they left.",
            );
        }
        info
    }

    fn outputs(&self) -> Vec<BackendOutput> {
        Alsa.outputs()
            .into_iter()
            .map(|o| BackendOutput { id: format!("plug{}", o.id), bit_perfect: false, ..o })
            .collect()
    }

    fn generation(&self) -> String {
        Alsa.generation()
    }

    /// Anything, because making it fit is the plugin's whole job. Measuring
    /// the card would list exactly what it exists to get around.
    fn caps(&self, _id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, SinkError> {
        Ok(chilpa_audio::capture::capabilities())
    }

    fn volume(&self, id: &str) -> Option<Knob> {
        card_knob(id)
    }

    fn set_volume(&self, id: &str, percent: u8) -> Result<(), String> {
        crate::Volume::of(id).ok_or_else(no_knob)?.set(percent)
    }

    fn set_muted(&self, id: &str, muted: bool) -> Result<(), String> {
        crate::Volume::of(id).ok_or_else(no_knob)?.mute(muted)
    }

    fn open(&self, id: &str, format: StreamFormat, _exclusive: bool)
        -> Result<Box<dyn AudioSink + Send>, SinkError> {
        // Named on purpose: the guard that only lets `hw:` through is what
        // keeps the card stack from falling back to this one.
        Ok(Box::new(AlsaSink::open_pcm(id, format)?))
    }
}

impl OutputBackend for AlsaPcm {
    fn kind(&self) -> BackendKind {
        BackendKind::AlsaPcm
    }

    fn info(&self) -> BackendInfo {
        let mut info = BackendInfo { kind: BackendKind::AlsaPcm, ..Alsa.info() };
        if info.present {
            info.fidelity = Fidelity::Never;
            info.caveat = chilpa_i18n::t(
                "pcm is the card as ALSA's own configuration sets it up: usually mixed with dmix, so \
                 other programs can play at the same time, and converted to whatever rate the mix \
                 runs at. It shares the card, and for the same reason it cannot promise the samples \
                 arrive as they left.",
            );
        }
        info
    }

    /// One per card: `sysdefault` names a card's first device, which is
    /// where a DAC plays.
    fn outputs(&self) -> Vec<BackendOutput> {
        Alsa.outputs()
            .into_iter()
            .filter_map(|o| {
                let card = o.id.strip_prefix("hw:")?.strip_suffix(",0")?.to_string();
                Some(BackendOutput { id: format!("sysdefault:CARD={card}"), bit_perfect: false, ..o })
            })
            .collect()
    }

    fn generation(&self) -> String {
        Alsa.generation()
    }

    /// Anything: ALSA's configuration makes it fit.
    fn caps(&self, _id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, SinkError> {
        Ok(chilpa_audio::capture::capabilities())
    }

    fn volume(&self, id: &str) -> Option<Knob> {
        card_knob(id)
    }

    fn set_volume(&self, id: &str, percent: u8) -> Result<(), String> {
        crate::Volume::of(id).ok_or_else(no_knob)?.set(percent)
    }

    fn set_muted(&self, id: &str, muted: bool) -> Result<(), String> {
        crate::Volume::of(id).ok_or_else(no_knob)?.mute(muted)
    }

    fn open(&self, id: &str, format: StreamFormat, _exclusive: bool)
        -> Result<Box<dyn AudioSink + Send>, SinkError> {
        Ok(Box::new(AlsaSink::open_pcm(id, format)?))
    }
}

/// What `default` resolves to, as ALSA describes it.
///
/// It is worth saying out loud, because "System" tells nobody anything and
/// what it resolves to is exactly what decides whether the samples survive.
fn what_default_is() -> String {
    // The PCM's own description, first line, which is what `aplay -L`
    // prints: "Default ALSA Output (currently PipeWire Media Server)".
    for hint in alsa::device_name::HintIter::new_str(None, "pcm").into_iter().flatten() {
        if hint.name.as_deref() == Some("default") {
            if let Some(description) = hint.desc {
                if let Some(line) = description.lines().next() {
                    return whoever(line.trim());
                }
            }
        }
    }
    String::new()
}

/// The name inside ALSA's description of `default`.
///
/// ALSA writes "Default ALSA Output (currently PipeWire Media Server)", and
/// what is worth reading is the part in the parentheses: the wrapper says
/// nothing and the name inside it is the answer.
fn whoever(description: &str) -> String {
    let inner = description
        .rsplit_once("(currently ")
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .unwrap_or(description);
    inner.trim().to_string()
}

impl OutputBackend for System {
    fn kind(&self) -> BackendKind {
        BackendKind::System
    }

    fn info(&self) -> BackendInfo {
        BackendInfo {
            kind: BackendKind::System,
            present: true,
            missing: String::new(),
            // Whoever `default` turns out to be. On a desktop this usually
            // names the sound server, which is the useful part: somebody
            // picking "System" should see that they picked PipeWire.
            provider: what_default_is(),
            // Never, and not `Depends`. `Depends` means "exact when the
            // output is asked for alone", and this stack cannot ask: it
            // opens whatever `default` resolves to and ignores the exclusive
            // flag entirely, because there is nothing in `default` to take
            // exclusively. Saying `Depends` made the verdict read
            // bit-perfect with the switch on while the samples went into
            // PipeWire's mixer -- the same lie this project already had to
            // fix once over PulseAudio, from the same cause: a stack that
            // says what it wishes were true instead of what it does.
            fidelity: Fidelity::Never,
            caveat: chilpa_i18n::t(
                "This is wherever the machine is already sending sound, so what happens to the \
                 samples is whatever that is set up to do -- usually mixing them with everything \
                 else and resampling the result. It always works. For samples that arrive as they \
                 left, choose the card itself.",
            ),
        }
    }

    fn outputs(&self) -> Vec<BackendOutput> {
        vec![BackendOutput {
            id: "default".to_string(),
            label: chilpa_i18n::t("Wherever the system is sending sound"),
            detail: what_default_is(),
            bit_perfect: false,
        }]
    }

    /// Whatever is behind `default` takes what it is handed and makes it
    /// fit, so there is nothing here that would refuse a format.
    ///
    /// **It must not be probed.** `probe` opens the device through the guard
    /// that only lets `hw:` through, and this stack's one output is called
    /// `default`, so probing it did not measure anything -- it returned the
    /// guard's error. Playing measures before it opens, so the stack whose
    /// whole job is to always work could not play a single track: the failure
    /// came out of the measurement, before anything was opened, as
    /// "default is not a direct device".
    ///
    /// Opening it to measure would be wrong even if it worked, for the reason
    /// the PipeWire stack gives: a listing that opens devices takes one away
    /// from whoever is using it.
    fn caps(&self, _id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, SinkError> {
        Ok(chilpa_audio::capture::capabilities())
    }

    fn volume(&self, id: &str) -> Option<Knob> {
        card_knob(id)
    }

    fn set_volume(&self, id: &str, percent: u8) -> Result<(), String> {
        crate::Volume::of(id).ok_or_else(no_knob)?.set(percent)
    }

    fn set_muted(&self, id: &str, muted: bool) -> Result<(), String> {
        crate::Volume::of(id).ok_or_else(no_knob)?.mute(muted)
    }

    fn open(&self, id: &str, format: StreamFormat, _exclusive: bool)
        -> Result<Box<dyn AudioSink + Send>, SinkError> {
        // `open_pcm` and not `open`: this stack names `default` on purpose,
        // and the guard that refuses everything but `hw:` is there to stop
        // the card stack from falling back to something that converts, not
        // to stop this one from naming what it means.
        Ok(Box::new(AlsaSink::open_pcm(id, format)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_default_resolves_to_is_read_out_of_alsas_wrapper() {
        assert_eq!(
            whoever("Default ALSA Output (currently PipeWire Media Server)"),
            "PipeWire Media Server"
        );
        assert_eq!(whoever("Playback/recording through the PulseAudio sound server"),
                   "Playback/recording through the PulseAudio sound server");
        assert_eq!(whoever(""), "");
    }

    #[test]
    fn the_card_backend_promises_exactly_what_it_can_keep() {
        let info = Alsa.info();
        if info.present {
            assert_eq!(info.fidelity, Fidelity::Exact);
            assert!(info.caveat.is_empty(), "an exact path has nothing to warn about");
        } else {
            assert!(!info.missing.is_empty(), "it has to say what is missing");
        }
    }

    #[test]
    fn the_system_backend_never_claims_to_be_exact() {
        // It is the one that always works, and that is the whole of what it
        // promises. Saying more would be the lie the wizard exists to undo.
        //
        // `Never` and not merely "not Exact". This asserted `!= Exact` and
        // passed while the stack said `Depends`, which reads as exact the
        // moment the output is asked for alone -- and the switch asks for it
        // alone on every track. The verdict came out bit-perfect over
        // PipeWire's mixer with the test green, because the test checked the
        // label and the label was not what the verdict was built from.
        let info = System.info();
        assert!(info.present);
        assert_eq!(
            info.fidelity,
            Fidelity::Never,
            "this stack cannot ask for an output alone: `open` has no use for the flag",
        );
        assert!(!info.caveat.is_empty(), "a path that is not exact says why");
        assert!(!System.outputs()[0].bit_perfect);
    }

    #[test]
    fn the_system_backend_measures_without_opening_anything() {
        // Its one output is called `default`, and `probe` goes through the
        // guard that only opens `hw:`. Measuring it therefore did not
        // measure: it handed back the guard's refusal. Playing measures
        // before it opens, so the stack whose whole job is to always work
        // could not play one track, and the error a person saw was
        // "default is not a direct device".
        let caps = System.caps("default").expect("measuring must not fail");
        assert!(!caps.sample_rates.is_empty());
        assert!(!caps.formats.is_empty());
    }

    #[test]
    fn the_plugin_names_the_same_cards_and_promises_nothing() {
        let (cards, plugged) = (Alsa.outputs(), AlsaPlug.outputs());
        assert_eq!(cards.len(), plugged.len());
        for (card, plug) in cards.iter().zip(&plugged) {
            assert_eq!(plug.id, format!("plug{}", card.id));
            assert!(!plug.bit_perfect);
        }
        let info = AlsaPlug.info();
        if info.present {
            assert_eq!(info.fidelity, Fidelity::Never);
            assert!(!info.caveat.is_empty(), "a path that is not exact says why");
        }
    }

    #[test]
    fn pcm_names_each_card_once_and_promises_nothing() {
        let firsts = Alsa.outputs().iter().filter(|o| o.id.ends_with(",0")).count();
        let pcm = AlsaPcm.outputs();
        assert_eq!(pcm.len(), firsts);
        for o in &pcm {
            assert!(o.id.starts_with("sysdefault:CARD="), "{}", o.id);
            assert!(!o.bit_perfect);
        }
        if AlsaPcm.info().present {
            assert_eq!(AlsaPcm.info().fidelity, Fidelity::Never);
        }
    }

    #[test]
    fn every_card_output_is_a_direct_device() {
        for output in Alsa.outputs() {
            assert!(output.id.starts_with("hw:"), "{} is not direct", output.id);
            assert!(output.bit_perfect);
            assert!(!output.label.is_empty());
        }
    }
}
