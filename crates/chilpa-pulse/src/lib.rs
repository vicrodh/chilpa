//! The PulseAudio protocol, whoever is answering it.
//!
//! On most desktops today the answer is PipeWire wearing a PulseAudio hat,
//! and that is worth saying rather than leaving somebody to believe they
//! picked a different road. `info` reports who replied.
//!
//! **This stack can never be bit-perfect, and says so.** The protocol is
//! built around a server that mixes every client together and resamples the
//! result to one rate. There is no exclusive mode to ask for and no
//! passthrough to arrange: a track at 44.1 played on a server running at 48
//! comes out resampled, and nothing here can change that. What it is good
//! for is working everywhere, which is not nothing.
//!
//! **A sink is named per stream.** The connection carries the sink it wants,
//! so nothing global moves: QBZ ran `pactl set-default-sink` and deliberately
//! never put it back, which repointed the whole desktop's sound every time
//! somebody chose an output in the player.

use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, Fidelity, OutputBackend};
use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::sink::{AudioSink, SinkError};
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;

/// The PulseAudio protocol.
pub struct Pulse;

/// What the protocol calls the formats Chilpa has.
///
/// Every one of them is a plain integer layout, which is what a bit-perfect
/// path needs. The protocol will accept them and then resample anyway, and
/// that is what `Fidelity::Never` is for.
///
/// **`S24In32` is `S32le` and not `S24_32le`.** Chilpa packs 24 bits left
/// justified, the low byte zero, which is a 32 bit sample. The protocol's
/// `S24_32le` is the value in the low 24 bits: named that, every 24 bit
/// track reached the server with its top byte dropped, which is noise. The
/// same mistake was made and fixed in the ALSA backend.
fn spec_format(f: SampleFormat) -> Option<Format> {
    Some(match f {
        SampleFormat::S16 => Format::S16le,
        SampleFormat::S24Packed => Format::S24le,
        SampleFormat::S24In32 => Format::S32le,
        SampleFormat::S32 => Format::S32le,
        SampleFormat::F32 => Format::F32le,
    })
}

/// The name inside what the server calls itself.
///
/// It answers "PulseAudio (on PipeWire 1.6.8)" when PipeWire is wearing the
/// hat, and putting that whole string after the stack's own name reads
/// "PulseAudio (through PulseAudio (on PipeWire 1.6.8))". What is worth
/// reading is the part in the parentheses; when there is none, PulseAudio is
/// answering for itself and there is nothing to add.
fn behind(server: &str) -> String {
    server
        .rsplit_once("(on ")
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .map(|inner| inner.trim().to_string())
        .unwrap_or_default()
}

impl OutputBackend for Pulse {
    fn kind(&self) -> BackendKind {
        BackendKind::Pulse
    }

    fn info(&self) -> BackendInfo {
        let Some(name) = crate::server::name() else {
            return BackendInfo::missing(
                BackendKind::Pulse,
                chilpa_i18n::t("no server is answering the PulseAudio protocol"),
            );
        };
        BackendInfo {
            kind: BackendKind::Pulse,
            present: true,
            missing: String::new(),
            // Who really replied. "PulseAudio (on PipeWire 1.6.8)" is a
            // different machine from one running PulseAudio itself, and only
            // this line can tell somebody which one they are on.
            provider: behind(&name),
            fidelity: Fidelity::Never,
            caveat: chilpa_i18n::t(
                "The PulseAudio protocol mixes every program together and resamples the result to \
                 one rate, so a track never reaches the device as it left the file. There is no \
                 setting here that changes that. It is the one that works everywhere; for samples \
                 that arrive untouched, choose the card itself or PipeWire.",
            ),
        }
    }

    /// The protocol carries anything and converts what it must, so there
    /// is nothing to refuse. That it converts is the stack's fidelity, and
    /// it says so where an output is chosen.
    fn caps(&self, _id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, SinkError> {
        Ok(chilpa_audio::capture::capabilities())
    }

    fn outputs(&self) -> Vec<BackendOutput> {
        crate::server::sinks()
            .into_iter()
            .map(|(name, description)| BackendOutput {
                label: if description.is_empty() { name.clone() } else { description },
                detail: name.clone(),
                id: name,
                // Never, and it is the protocol and not the device: the same
                // DAC is bit-perfect through the card and is not through
                // here.
                bit_perfect: false,
            })
            .collect()
    }

    fn open(
        &self,
        id: &str,
        format: StreamFormat,
        _exclusive: bool,
    ) -> Result<Box<dyn AudioSink + Send>, SinkError> {
        // There is nothing to ask for exclusively: the protocol has no such
        // idea, and pretending it did is how a player ends up claiming
        // bit-perfect over a mixer.
        let Some(encoding) = spec_format(format.sample_format) else {
            return Err(SinkError::Backend(chilpa_i18n::t(
                "the PulseAudio protocol does not carry this format",
            )));
        };
        let spec = Spec {
            format: encoding,
            rate: format.sample_rate_hz,
            channels: format.channels as u8,
        };
        if !spec.is_valid() {
            return Err(SinkError::Backend(chilpa_i18n::t_args(
                "the PulseAudio protocol does not carry {} Hz with {} channels",
                &[&format.sample_rate_hz.to_string(), &format.channels.to_string()],
            )));
        }
        // The sink travels with the connection, so nothing outside this
        // stream is touched.
        let simple = Simple::new(None, "Chilpa", Direction::Playback, Some(id), "music", &spec, None, None)
            .map_err(|e| SinkError::Backend(e.to_string().unwrap_or_default()))?;
        Ok(Box::new(PulseSink { simple, stream: format }))
    }
}

/// A stream on the server.
struct PulseSink {
    simple: Simple,
    stream: StreamFormat,
}

impl AudioSink for PulseSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        // The simple API writes all of it or fails; there is no partial
        // write to report and no underrun to recover from, because the
        // server is doing the buffering. The answer is in frames, as the
        // trait asks.
        self.simple
            .write(frames)
            .map(|_| frames.len() / self.stream.frame_bytes())
            .map_err(|e| SinkError::Backend(e.to_string().unwrap_or_default()))
    }

    fn close(&mut self) -> Result<(), SinkError> {
        // Dropped with the sink. Flushing here would wait for the server,
        // and stopping never waits for a device.
        Ok(())
    }
}

/// Asking the server what it has, through `pactl`.
mod server {
    use std::time::Duration;

    /// How long a question may take before it is given up on. QBZ asked with
    /// no limit, and the refresh it freezes is the one running while
    /// somebody is looking at the window.
    const PATIENCE: Duration = Duration::from_secs(2);

    /// The sinks, as (name, description).
    pub fn sinks() -> Vec<(String, String)> {
        let Some(text) = ask(&["list", "sinks"]) else { return Vec::new() };
        let mut out = Vec::new();
        let mut name = String::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(v) = line.strip_prefix("Name: ") {
                name = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("Description: ") {
                if !name.is_empty() {
                    out.push((std::mem::take(&mut name), v.trim().to_string()));
                }
            }
        }
        out
    }

    /// Who is answering the protocol, or nothing when nobody is.
    pub fn name() -> Option<String> {
        let text = ask(&["info"])?;
        text.lines()
            .find_map(|l| l.trim().strip_prefix("Server Name: "))
            .map(|v| v.trim().to_string())
    }

    fn ask(args: &[&str]) -> Option<String> {
        let mut child = std::process::Command::new("pactl")
            .args(args)
            // In C: `pactl` translates its own field names into whatever the
            // desktop speaks, and the field names are what gets read. Without
            // this the parser finds nothing anywhere but an English machine,
            // and the stack reports itself missing where it is plainly
            // running. Nothing read here reaches a person.
            .env("LC_ALL", "C")
            .env("LANGUAGE", "C")
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_stack_never_claims_to_be_exact() {
        // It cannot be, and a player whose whole claim is bit-perfect has to
        // be the one that says so plainly instead of leaving somebody to
        // wonder why their DAC never shows the right rate.
        let info = Pulse.info();
        if info.present {
            assert_eq!(info.fidelity, Fidelity::Never);
            assert!(!info.caveat.is_empty());
            assert!(Pulse.outputs().iter().all(|o| !o.bit_perfect));
        } else {
            assert!(!info.missing.is_empty());
        }
    }

    #[test]
    fn the_name_says_who_is_behind_it_without_saying_it_twice() {
        assert_eq!(behind("PulseAudio (on PipeWire 1.6.8)"), "PipeWire 1.6.8");
        // PulseAudio answering for itself adds nothing.
        assert_eq!(behind("pulseaudio"), "");
        assert_eq!(behind(""), "");
    }

    /// What the server reads out of one little-endian four byte word, as
    /// `pulse/sample.h` defines the four byte formats, as a fraction of full
    /// scale.
    fn server_hears(format: Format, word: [u8; 4]) -> f64 {
        let w = i32::from_le_bytes(word);
        match format {
            Format::S32le => f64::from(w) / f64::from(1u32 << 31),
            // "Signed 24 bit integer PCM in LSB of 32 bit words".
            Format::S24_32le => f64::from((w << 8) >> 8) / f64::from(1u32 << 23),
            other => panic!("{other:?} is not a four byte format"),
        }
    }

    #[test]
    fn a_24_bit_sample_reaches_the_server_as_the_same_sample() {
        // Every 24 bit file asks for S24In32, which Chilpa packs left
        // justified. Named S24_32LE, which is low justified, the server read
        // each sample with its top byte gone and the rest shifted up: noise.
        for v in [1, -1, 0x12_3456, -0x12_3456, 0x7F_FFFF, -0x80_0000] {
            let mut bytes = Vec::new();
            chilpa_audio::pack::pack(
                &chilpa_audio::pack::Samples::Int { data: &[v], bits: 24 },
                SampleFormat::S24In32,
                &mut bytes,
            )
            .unwrap();
            let format = spec_format(SampleFormat::S24In32).unwrap();
            let word: [u8; 4] = bytes.as_slice().try_into().expect("four bytes a sample");
            assert_eq!(
                server_hears(format, word),
                f64::from(v) / f64::from(1u32 << 23),
                "{v:#x} named {format:?}"
            );
        }
    }

    #[test]
    fn a_write_answers_in_frames_and_not_in_bytes() {
        // The renderer multiplies the answer by the frame size. Answered in
        // bytes, every frame was counted four to eight times, and the
        // position ran ahead of the music by as much.
        let Some((id, _)) = server::sinks().into_iter().next() else {
            eprintln!("no PulseAudio server: skipped");
            return;
        };
        let format = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let mut sink = match Pulse.open(&id, format, false) {
            Ok(sink) => sink,
            Err(e) => {
                eprintln!("the server would not open {id}: skipped ({e:?})");
                return;
            }
        };
        // Ten milliseconds of silence, mixed in with whatever is playing.
        let silence = vec![0u8; 441 * format.frame_bytes()];
        assert_eq!(sink.write(&silence).unwrap(), 441);
    }

    #[test]
    fn asking_the_server_never_hangs() {
        let _ = server::name();
        let _ = server::sinks();
    }
}
