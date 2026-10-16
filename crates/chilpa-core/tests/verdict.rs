//! What the window is allowed to call bit-perfect.
//!
//! The project's invariant is not "Chilpa did not convert anything". It is
//! that what left the file reached the device unchanged, and Chilpa is only
//! the first half of that road. A stack that hands the samples to a mixer
//! leaves every transform empty, so a verdict built on the delivery alone
//! says bit-perfect over a resampler.
//!
//! This is asserted here and not beside each stack because it is the
//! consequence that matters, and the consequence is only visible where the
//! two halves meet. The test that did live beside the stack asserted that
//! `System` did not say `Exact` -- true, it said `Depends` -- and stayed green
//! while the verdict came out bit-perfect over PipeWire's mixer. `Depends`
//! means exact when the output is asked for alone, and the switch asks for it
//! alone on every track, so the label passed and the answer lied.

use chilpa_core::backends::{self, Fidelity, StackKind};
use chilpa_core::path_is_exact;

/// Every output of every stack this build has, by full identifier.
fn all_outputs() -> Vec<(StackKind, String)> {
    let mut out = Vec::new();
    for info in backends::described() {
        if !info.present {
            continue;
        }
        for output in backends::outputs_of(info.kind) {
            out.push((info.kind, output.id));
        }
    }
    out
}

#[test]
fn a_stack_that_cannot_ask_for_an_output_alone_is_never_exact() {
    // The rule, stated once: `Depends` is a promise to take the output
    // exclusively when asked, and a stack that ignores the flag cannot make
    // it. `System` opens whatever `default` resolves to and has nothing to
    // take exclusively, so it says `Never`.
    for info in backends::described() {
        if info.kind == StackKind::System {
            assert_eq!(
                info.fidelity,
                Fidelity::Never,
                "the system stack cannot take an output alone, so it cannot depend on it",
            );
        }
    }
}

#[test]
fn the_verdict_over_the_system_stack_is_no_with_the_switch_on() {
    // The switch being on is exactly the case that used to lie: it turns
    // into `exclusive = true`, and `Depends` plus exclusive reads as exact.
    let outputs = backends::outputs_of(StackKind::System);
    if outputs.is_empty() {
        return;
    }
    for output in outputs {
        assert!(
            !path_is_exact(&output.id, true),
            "{} went through a mixer and the window would have said bit-perfect",
            output.id,
        );
        assert!(!path_is_exact(&output.id, false), "{} is not exact either way", output.id);
    }
}

#[test]
fn what_each_stack_promises_matches_what_its_outputs_are_marked_as() {
    // A row in the picker marked bit-perfect under a stack that can never be
    // exact is the same lie one screen earlier.
    for (kind, id) in all_outputs() {
        let Some(info) = backends::described().into_iter().find(|i| i.kind == kind) else {
            continue;
        };
        let marked = backends::outputs_of(kind)
            .into_iter()
            .find(|o| o.id == id)
            .map(|o| o.bit_perfect)
            .unwrap_or(false);
        if info.fidelity == Fidelity::Never {
            assert!(
                !marked,
                "{id} is marked bit-perfect under a stack that says it never is",
            );
        }
        // And the other direction: a row that is marked has to be reachable
        // as exact by asking for it alone.
        if marked {
            assert!(
                path_is_exact(&id, true),
                "{id} is marked bit-perfect but the verdict would say no",
            );
        }
    }
}

#[test]
fn an_output_nobody_recognises_is_never_called_exact() {
    // A settings file written by a newer Chilpa, or on another machine. The
    // safe answer to "did the samples survive" is never yes.
    assert!(!path_is_exact("oss:dsp0", true));
    assert!(!path_is_exact("hw:1,0", true), "a bare id names no stack");
    assert!(!path_is_exact("", true));
}
