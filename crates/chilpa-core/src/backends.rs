//! Every stack this build knows, and how an output is found inside one.
//!
//! The list is assembled here and not in a platform crate because that is
//! what changes per platform: on Linux it is ALSA and the servers on top of
//! it, on Windows it will be WASAPI, and the crate that knows about a card
//! has no business knowing what else the machine might offer.
//!
//! What crosses out of here is an identifier that names its own stack
//! (`alsa:hw:1,0`), so that a remembered output read back from the settings
//! still says which stack it belonged to. In QBZ a device that disappeared
//! left the window showing "System default" while the preference still
//! pointed at the old bare id, and nothing could tell which.

use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, OutputBackend};

// What the window needs to draw a stack picker, handed on from here.
//
// The facade is not allowed to reach into `chilpa-audio` (ADR-001), and that
// is not an obstacle to work around: the moment it could, `StreamFormat`,
// `DeviceCaps` and `SinkError` would start crossing the same border, and the
// window would end up shaped like the engine. What it needs is the answer to
// "which stacks are there and what do they promise", and that is a question
// for whoever assembles them.
pub use chilpa_audio::backend::{BackendInfo as Stack, BackendKind as StackKind, Fidelity};

/// Every stack, present or not, in the order the window offers them.
///
/// The ones that are not on this machine are in here too, carrying the
/// reason: a picker that silently drops what it could not find teaches
/// nobody anything, and the reason is usually one package away.
pub fn all() -> Vec<Box<dyn OutputBackend>> {
    let mut stacks: Vec<Box<dyn OutputBackend>> = Vec::new();
    #[cfg(target_os = "linux")]
    {
        stacks.push(Box::new(chilpa_alsa::System));
        stacks.push(Box::new(chilpa_alsa::Alsa));
        stacks.push(Box::new(chilpa_pipewire::PipeWire));
        stacks.push(Box::new(chilpa_pulse::Pulse));
        stacks.push(Box::new(chilpa_jack::Jack));
    }
    stacks.push(Box::new(chilpa_audio::capture::FileBackend::new(super::capture_path())));
    stacks
}

/// The one with that kind, if this build has it.
pub fn of(kind: BackendKind) -> Option<Box<dyn OutputBackend>> {
    all().into_iter().find(|b| b.info().kind == kind)
}

/// What each stack says about itself.
pub fn described() -> Vec<BackendInfo> {
    all().iter().map(|b| b.info()).collect()
}

/// The outputs of one stack, with identifiers that name it.
pub fn outputs_of(kind: BackendKind) -> Vec<BackendOutput> {
    let Some(stack) = of(kind) else { return Vec::new() };
    stack
        .outputs()
        .into_iter()
        .map(|o| BackendOutput { id: BackendOutput::full_id(kind, &o.id), ..o })
        .collect()
}

/// The stack an identifier names, and what that stack calls the output.
///
/// `None` when the identifier names no stack this build has, which is what a
/// settings file written by a newer Chilpa, or on another machine, looks
/// like. It is not an error and it is not silently turned into something
/// else: whoever asked decides what to do about it.
pub fn resolve(full_id: &str) -> Option<(Box<dyn OutputBackend>, String)> {
    let (kind, id) = BackendOutput::split(full_id)?;
    of(kind).map(|stack| (stack, id.to_string()))
}

/// The first output worth starting on, when nothing is remembered.
///
/// The card before the server: somebody who installed a player that says
/// "bit-perfect" on the box did not install it to be resampled. If there is
/// no card, whatever the system is using, which always works.
pub fn first_choice() -> Option<String> {
    for kind in [BackendKind::Alsa, BackendKind::System] {
        if let Some(first) = outputs_of(kind).into_iter().next() {
            return Some(first.id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stack_that_is_here_answers_for_itself() {
        for info in described() {
            if info.present {
                assert!(info.missing.is_empty(), "{:?} is here and says something is missing", info.kind);
            } else {
                assert!(!info.missing.is_empty(), "{:?} is not here and does not say why", info.kind);
            }
        }
    }

    #[test]
    fn an_identifier_goes_out_and_comes_back_to_the_stack_it_came_from() {
        for info in described() {
            for output in outputs_of(info.kind) {
                let (stack, bare) = resolve(&output.id).expect("it resolves");
                assert_eq!(stack.info().kind, info.kind, "{}", output.id);
                assert!(!bare.is_empty(), "{}", output.id);
            }
        }
    }

    #[test]
    fn an_identifier_from_a_stack_this_build_lacks_is_not_turned_into_another() {
        // What a settings file from a newer Chilpa looks like. Quietly
        // reading it as the system default is how somebody ends up
        // resampled without being told.
        assert!(resolve("oss:dsp0").is_none());
        assert!(resolve("hw:1,0").is_none(), "a bare id names no stack");
    }

    #[test]
    fn only_a_stack_that_can_be_exact_lets_a_verdict_say_so() {
        // The bug this closes, caught by playing a track: the delivery only
        // knows what Chilpa did to the samples, and handing them to a server
        // that resamples leaves every transform empty. So the window said
        // "bit-perfect" in green over PulseAudio, which mixes every program
        // together and resamples the result -- the exact claim this project
        // exists to stop making.
        for info in described() {
            for output in outputs_of(info.kind) {
                use chilpa_audio::backend::Fidelity;
                let alone = super::super::path_is_exact(&output.id, true);
                let shared = super::super::path_is_exact(&output.id, false);
                match info.fidelity {
                    // Nothing is between, however it was asked for.
                    Fidelity::Exact => assert!(alone && shared, "{}", output.id),
                    // The whole of what "it depends" depends on.
                    Fidelity::Depends => assert!(
                        alone && !shared,
                        "{} says the same either way, so it does not depend on anything",
                        output.id
                    ),
                    Fidelity::Never => assert!(!alone && !shared, "{}", output.id),
                }
                let never = info.fidelity == Fidelity::Never;
                assert!(
                    !(never && output.bit_perfect),
                    "{} is marked bit-perfect on a stack that never is",
                    output.id
                );
            }
        }
    }

    #[test]
    fn an_output_nobody_has_heard_of_is_never_called_exact() {
        // The safe answer to "did the samples survive" is never yes when
        // nothing is known about the road.
        for alone in [true, false] {
            assert!(!super::super::path_is_exact("oss:dsp0", alone));
            assert!(!super::super::path_is_exact("hw:1,0", alone));
            assert!(!super::super::path_is_exact("", alone));
        }
    }

    #[test]
    fn the_first_choice_is_a_real_output_of_a_stack_that_is_here() {
        if let Some(id) = first_choice() {
            assert!(resolve(&id).is_some(), "{id}");
        }
    }
}
