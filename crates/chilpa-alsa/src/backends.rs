//! The two stacks ALSA itself answers for: the card, and whatever the
//! system has decided.
//!
//! They share every line of code that touches the hardware and differ in
//! exactly one thing, which is the whole point of having both: one names a
//! card and the other names `default`, and `default` is wherever the machine
//! is currently sending sound. On a desktop that is usually a sound server,
//! so the samples pass through a mixer on their way out. It always works and
//! it promises nothing.

use crate::{devices, AlsaSink};
use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, Fidelity, OutputBackend};
use chilpa_audio::format::StreamFormat;
use chilpa_audio::sink::{AudioSink, SinkError};

/// The card, addressed directly.
pub struct Alsa;

/// Whatever the system is using.
pub struct System;

impl OutputBackend for Alsa {
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
        let mut outputs: Vec<BackendOutput> = devices::list()
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
            .collect();
        // What somebody plugged in on purpose, above what is soldered to the
        // board.
        outputs.sort_by_key(|o| !o.detail.starts_with("USB"));
        outputs
    }

    fn open(&self, id: &str, format: StreamFormat, _exclusive: bool)
        -> Result<Box<dyn AudioSink + Send>, SinkError> {
        Ok(Box::new(AlsaSink::open(id, format)?))
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
    fn info(&self) -> BackendInfo {
        BackendInfo {
            kind: BackendKind::System,
            present: true,
            missing: String::new(),
            // Whoever `default` turns out to be. On a desktop this usually
            // names the sound server, which is the useful part: somebody
            // picking "System" should see that they picked PipeWire.
            provider: what_default_is(),
            fidelity: Fidelity::Depends,
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

    fn open(&self, id: &str, format: StreamFormat, _exclusive: bool)
        -> Result<Box<dyn AudioSink + Send>, SinkError> {
        Ok(Box::new(AlsaSink::open(id, format)?))
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
        let info = System.info();
        assert!(info.present);
        assert_ne!(info.fidelity, Fidelity::Exact);
        assert!(!info.caveat.is_empty(), "a path that is not exact says why");
        assert!(!System.outputs()[0].bit_perfect);
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
