//! PipeWire, reached through the ALSA plugin it installs.
//!
//! No second way of writing to a device: PipeWire ships a PCM that ALSA
//! knows, so the samples take the same road `chilpa-alsa` already drives.
//! What this crate knows that that one does not is which sinks exist, what
//! they are called, and how to name one.
//!
//! **A sink is named per stream, and nothing global is touched.** The plugin
//! takes arguments -- `pipewire:NODE=<name>,EXCLUSIVE=1` -- so the stream
//! goes where it was told without setting an environment variable and
//! without repointing the machine's default. QBZ did both: it wrote
//! `PIPEWIRE_NODE`, which is one variable shared by every stream in the
//! process, and it ran `pactl set-default-sink` and deliberately never put
//! it back, so choosing an output in the player moved the whole desktop's
//! sound. Neither is here.
//!
//! **`EXCLUSIVE=1` is what makes bit-perfect possible.** With it the node is
//! this stream's alone and the graph hands the samples over; without it they
//! are mixed with everything else and resampled to the graph's rate. So the
//! stack promises `Depends` and means it: it is exact when asked for
//! exclusively and not otherwise.

use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, Fidelity, OutputBackend};
use chilpa_audio::format::StreamFormat;
use chilpa_audio::sink::{AudioSink, SinkError};
use std::time::Duration;

/// How long a question to the server may take before it is given up on.
///
/// QBZ asked `pactl` with no limit, so a wedged daemon froze the refresh --
/// and the refresh runs while somebody is looking at the window. A server
/// that cannot answer in two seconds is not going to.
const PATIENCE: Duration = Duration::from_secs(2);

/// The PipeWire graph.
pub struct PipeWire;

/// One sink, as the server describes it.
struct Sink {
    /// `node.name`, which is what the plugin is given.
    name: String,
    /// What a person reads: "Fosi Audio ZH3 Analog Stereo".
    description: String,
}

/// Asks the server what sinks it has.
///
/// Through `pactl`, which is the one interface that answers whether PipeWire
/// or PulseAudio is behind it. Empty when it is not there or does not answer
/// in time, and that is not an error here: `info` is what says whether the
/// stack exists, and it asks the same question.
fn sinks() -> Vec<Sink> {
    let Some(text) = ask(&["list", "sinks"]) else { return Vec::new() };
    let mut out = Vec::new();
    let mut name = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Name: ") {
            name = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("Description: ") {
            if !name.is_empty() {
                out.push(Sink { name: std::mem::take(&mut name), description: v.trim().to_string() });
            }
        }
    }
    out
}

/// Runs `pactl` and gives back what it said, or nothing.
///
/// It is killed when it runs past `PATIENCE`. A child that is waited on
/// forever is the difference between a window that is slow and a window that
/// is stuck.
fn ask(args: &[&str]) -> Option<String> {
    let mut child = std::process::Command::new("pactl")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return None,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => return None,
        }
    }
    let output = child.wait_with_output().ok()?;
    String::from_utf8(output.stdout).ok()
}

/// Whoever is answering the protocol, out of `pactl info`.
///
/// Worth saying: on a machine where PipeWire answers for PulseAudio and for
/// JACK as well, choosing one of those is not a shorter path, and only this
/// line can tell somebody that.
fn server() -> String {
    let Some(text) = ask(&["info"]) else { return String::new() };
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("Server Name: ") {
            return v.trim().to_string();
        }
    }
    String::new()
}

impl OutputBackend for PipeWire {
    fn info(&self) -> BackendInfo {
        // What this stack needs is a server that answers and a PCM that
        // reaches it. Asking for what it actually needs, rather than looking
        // for a daemon by name, is what stops a stack being called missing
        // when something else is providing it.
        let name = server();
        if name.is_empty() {
            return BackendInfo::missing(
                BackendKind::PipeWire,
                chilpa_i18n::t("no sound server answered; install pipewire and pipewire-pulse"),
            );
        }
        if !name.to_lowercase().contains("pipewire") {
            return BackendInfo::missing(
                BackendKind::PipeWire,
                chilpa_i18n::t_args("the server answering is {}, not PipeWire", &[&name]),
            );
        }
        BackendInfo {
            kind: BackendKind::PipeWire,
            present: true,
            missing: String::new(),
            provider: String::new(),
            fidelity: Fidelity::Depends,
            caveat: chilpa_i18n::t(
                "PipeWire passes the samples through untouched only when it hands the output over \
                 to one program at a time. Chilpa asks for that whenever bit-perfect is on. With \
                 it off the graph mixes this with everything else and resamples the result to \
                 whatever rate it is running at.",
            ),
        }
    }

    fn outputs(&self) -> Vec<BackendOutput> {
        sinks()
            .into_iter()
            .map(|s| BackendOutput {
                label: if s.description.is_empty() { s.name.clone() } else { s.description },
                detail: s.name.clone(),
                id: s.name,
                // The path can be exact, and whether it is comes down to
                // asking for the output exclusively. Saying it flatly cannot
                // be would be as wrong as saying it always is.
                bit_perfect: true,
            })
            .collect()
    }

    fn open(
        &self,
        id: &str,
        format: StreamFormat,
        exclusive: bool,
    ) -> Result<Box<dyn AudioSink + Send>, SinkError> {
        // A sink that is gone has to be caught here. The plugin does not
        // fail on a name it cannot find: it quietly plays to the default
        // instead, so an unplugged DAC would come out of the laptop speakers
        // with the window still naming the DAC.
        if !sinks().iter().any(|s| s.name == id) {
            return Err(SinkError::Backend(chilpa_i18n::t_args(
                "PipeWire has no output called \"{}\" any more",
                &[id],
            )));
        }
        let pcm = if exclusive {
            format!("pipewire:NODE={id},EXCLUSIVE=1")
        } else {
            format!("pipewire:NODE={id}")
        };
        Ok(Box::new(chilpa_alsa::AlsaSink::open(&pcm, format)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_never_hangs_and_never_panics() {
        // Whatever is or is not installed, this returns.
        let _ = server();
        let _ = sinks();
    }

    #[test]
    fn a_stack_that_is_not_here_says_what_is_missing() {
        let info = PipeWire.info();
        if !info.present {
            assert!(!info.missing.is_empty());
        } else {
            assert_eq!(info.fidelity, Fidelity::Depends);
            assert!(!info.caveat.is_empty(), "a path that is not exact says why");
        }
    }

    #[test]
    fn an_output_that_is_gone_is_refused_and_not_played_somewhere_else() {
        // The trap: the plugin falls back to the default sink for a name it
        // cannot find, so without this an unplugged DAC plays out of the
        // laptop speakers while the window still names the DAC.
        let format =
            StreamFormat::new(chilpa_audio::format::SampleFormat::S16, 44_100, 2);
        let refused = PipeWire.open("no.such.sink.anywhere", format, false);
        assert!(refused.is_err(), "it has to refuse rather than play elsewhere");
    }
}
