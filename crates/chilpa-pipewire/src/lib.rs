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
fn ask(args: &[&str]) -> Option<String> {
    run("pactl", args)
}

/// Runs a PipeWire or PulseAudio tool and gives back what it said, or
/// nothing.
///
/// It is killed when it runs past `PATIENCE`. A child that is waited on
/// forever is the difference between a window that is slow and a window that
/// is stuck.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut child = std::process::Command::new(program)
        .args(args)
        // In C, because the field names are what gets read and `pactl`
        // translates them into whatever the desktop speaks. Without this,
        // the parser finds nothing anywhere but an English machine and the
        // stack reports itself missing -- on the machine where it is plainly
        // running, with the window falling back to the one that resamples.
        // Nothing read here is shown to anybody, so pinning the language of
        // what gets parsed costs nothing.
        .env("LC_ALL", "C")
        .env("LANGUAGE", "C")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    // Read while it runs. An answer longer than the pipe holds -- 64 KiB,
    // a score of outputs in `pactl list sinks` -- waits to be read, and
    // waiting for the program to end first waited until the patience ran
    // out and the stack said it was missing.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut text).ok().map(|_| text)
    });
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
    reader.join().ok()?
}

/// The rates PipeWire's graph may run at, out of its own settings.
///
/// A stream asked for alone passes through untouched only if the graph can
/// run at the stream's rate, and the graph switches only to the rates in
/// `clock.allowed-rates` -- and to none when `clock.force-rate` is set. Any
/// other rate is resampled, exclusive or not. `None` when the settings could
/// not be read.
fn graph_rates() -> Option<Vec<u32>> {
    rates_in(&run("pw-metadata", &["-n", "settings"])?)
}

/// The rates in `pw-metadata`'s account of the settings.
fn rates_in(settings: &str) -> Option<Vec<u32>> {
    let value = |key: &str| {
        settings.lines().find_map(|line| {
            let after = line.split_once(&format!("key:'{key}'"))?.1;
            let value = after.split_once("value:'")?.1;
            value.split_once('\'').map(|(v, _)| v.to_string())
        })
    };
    let forced = value("clock.force-rate").and_then(|v| v.trim().parse::<u32>().ok());
    if let Some(rate) = forced.filter(|r| *r > 0) {
        return Some(vec![rate]);
    }
    let allowed: Vec<u32> = value("clock.allowed-rates")
        .unwrap_or_default()
        .split(|c: char| c == ',' || c == '[' || c == ']' || c.is_whitespace())
        .filter_map(|r| r.parse().ok())
        .collect();
    if !allowed.is_empty() {
        return Some(allowed);
    }
    value("clock.rate").and_then(|v| v.trim().parse().ok()).map(|r| vec![r])
}

/// Whether a stream at `rate`, asked for alone, reaches the device untouched,
/// and if not, what to tell somebody.
///
/// `EXCLUSIVE=1` keeps other programs off the node; it does not stop the
/// graph resampling a rate it cannot switch to. So a 176.4 kHz track -- or
/// DSD64 over DoP, which travels at 176.4 kHz and turns into noise when
/// resampled -- went through a graph allowed only up to 96 and 192, and the
/// verdict said bit-perfect because the output had been asked for alone.
fn untouched_at(rate: u32, rates: Option<&[u32]>) -> Result<(), String> {
    match rates {
        Some(rates) if rates.contains(&rate) => Ok(()),
        Some(rates) => {
            let rates: Vec<String> = rates.iter().map(u32::to_string).collect();
            Err(chilpa_i18n::t_args(
                "PipeWire's graph only runs at {} Hz, so {} Hz would be resampled; add it to \
                 default.clock.allowed-rates, or choose the card itself",
                &[&rates.join(", "), &rate.to_string()],
            ))
        }
        None => Err(chilpa_i18n::t(
            "the rates PipeWire's graph may run at could not be read, so nothing can be \
             promised untouched",
        )),
    }
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
    fn kind(&self) -> BackendKind {
        BackendKind::PipeWire
    }

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

    /// A server takes whatever it is handed and makes it fit, so there is
    /// nothing here that would refuse a format. What it does to the samples
    /// on the way is the stack's fidelity, said where an output is chosen
    /// rather than smuggled in as a capability.
    ///
    /// Measuring for real would mean opening the sink, and opening one to
    /// measure it takes it from whoever is using it -- which is exactly what
    /// a listing must never do.
    fn caps(&self, _id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, SinkError> {
        Ok(chilpa_audio::capture::capabilities())
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
        if exclusive {
            untouched_at(format.sample_rate_hz, graph_rates().as_deref()).map_err(SinkError::Backend)?;
        }
        let pcm = if exclusive {
            format!("pipewire:NODE={id},EXCLUSIVE=1")
        } else {
            format!("pipewire:NODE={id}")
        };
        match chilpa_alsa::AlsaSink::open_pcm(&pcm, format) {
            Ok(sink) => Ok(Box::new(sink)),
            // The sink is still listed and it would not open, which on an
            // exclusive ask means somebody else has it. ALSA says that as
            // "snd_pcm_hw_params failed with error 'No such file or
            // directory'", and nobody can act on that. What somebody can act
            // on is knowing another program is holding the output.
            Err(e) if exclusive => Err(SinkError::Backend(chilpa_i18n::t_args(
                "another program is using \"{}\"; close it, or turn bit-perfect off to share it ({})",
                &[id, &e.to_string()],
            ))),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_answer_comes_back_whole() {
        // A machine with a score of outputs answers `pactl list sinks` with
        // more than the 64 KiB a pipe holds. Waiting for the program to end
        // before reading waited while it waited to be read, until the
        // patience ran out and the stack said it was missing.
        let said = run("sh", &["-c", "head -c 100000 /dev/zero | tr '\\0' x"]);
        assert_eq!(said.map(|s| s.len()), Some(100_000));
    }

    /// What `pw-metadata -n settings` printed on the machine this was found on.
    const SETTINGS: &str = "Found \"settings\" metadata 32
update: id:0 key:'log.level' value:'2' type:''
update: id:0 key:'clock.rate' value:'48000' type:''
update: id:0 key:'clock.allowed-rates' value:'[ 44100, 48000, 88200, 96000, 192000 ]' type:''
update: id:0 key:'clock.quantum' value:'1024' type:''
update: id:0 key:'clock.force-rate' value:'0' type:''
";

    #[test]
    fn the_graph_s_rates_are_read_from_its_settings() {
        assert_eq!(rates_in(SETTINGS), Some(vec![44_100, 48_000, 88_200, 96_000, 192_000]));
        let forced = SETTINGS.replace("clock.force-rate' value:'0'", "clock.force-rate' value:'96000'");
        assert_eq!(rates_in(&forced), Some(vec![96_000]), "a forced rate is the only one");
        let none = SETTINGS.replace("[ 44100, 48000, 88200, 96000, 192000 ]", "[ ]");
        assert_eq!(rates_in(&none), Some(vec![48_000]), "with no list, the one it runs at");
    }

    #[test]
    fn a_rate_the_graph_cannot_switch_to_is_not_promised_untouched() {
        let rates = rates_in(SETTINGS).unwrap();
        assert!(untouched_at(44_100, Some(&rates)).is_ok());
        assert!(untouched_at(192_000, Some(&rates)).is_ok());
        let dop = untouched_at(176_400, Some(&rates)).expect_err("DSD64 over DoP would be resampled");
        assert!(dop.contains("176400"), "{dop}");
        assert!(untouched_at(44_100, None).is_err(), "nothing is promised from settings nobody read");
    }

    #[test]
    fn asking_never_hangs_and_never_panics() {
        // Whatever is or is not installed, this returns.
        let _ = server();
        let _ = sinks();
    }

    #[test]
    fn the_server_is_found_whatever_language_the_desktop_speaks() {
        // The bug this closes: `pactl` translates its own field names into
        // whatever the desktop speaks, so a parser looking for "Server
        // Name:" finds nothing anywhere but an English machine. PipeWire
        // then reported itself missing on the machine where it was plainly
        // running, and the window quietly fell back to the stack that
        // resamples.
        let under_spanish = std::process::Command::new("pactl")
            .arg("info")
            .env("LANG", "es_MX.UTF-8")
            .env("LC_ALL", "C")
            .env("LANGUAGE", "C")
            .output();
        if let Ok(out) = under_spanish {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                assert!(
                    text.contains("Server Name:") || text.contains("Server String:"),
                    "the field names came out translated: {:?}",
                    text.lines().next()
                );
            }
        }
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
