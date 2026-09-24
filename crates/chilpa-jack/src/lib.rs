//! The JACK graph, whoever is answering for it.
//!
//! On this machine there is no `jackd` and JACK works, because PipeWire
//! provides the client library. That is worth saying rather than leaving
//! somebody to believe they picked a shorter road, and `info` says who
//! replied.
//!
//! **This stack can never be bit-perfect, and the reason is what JACK is.**
//! A graph runs at one sample rate and every client runs at it. A client
//! does not choose; it is told, at the moment it connects. So a 44.1 track
//! on a graph running at 48 is resampled before it is anybody's decision,
//! and no setting here changes that. What JACK is for is routing audio
//! between programs, which is a different thing from delivering a file to a
//! DAC untouched.
//!
//! **The graph calls, it is not written to.** Every other stack here takes a
//! blocking write; JACK hands out a buffer on its own thread and expects it
//! filled before it returns. A ring between the two is what makes it look
//! like the others, and the ring is the only place the two threads meet.

use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, Fidelity, OutputBackend};
use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::sink::{AudioSink, SinkError};
use jack::PortSpec as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The JACK graph.
pub struct Jack;

/// What the client is called in the graph.
const CLIENT: &str = "Chilpa";

/// Asks the graph who it is, without joining it for longer than the answer
/// takes.
///
/// Opening a client is the only way to find out anything about JACK: there
/// is no file to read and no command that answers without one. It is closed
/// immediately, and it is the one place here that connects without being
/// asked to play.
fn hello() -> Option<(jack::Client, jack::ClientStatus)> {
    jack::Client::new(CLIENT, jack::ClientOptions::NO_START_SERVER).ok()
}

/// What the graph accepts, which is one rate and nothing else.
///
/// Really measured: it is asked of the running graph, not assumed. Saying it
/// here rather than refusing inside `open` is what lets the machinery that
/// already exists do its job -- with bit-perfect on, a track at another rate
/// is refused and says why; with it off, it is converted to the graph's rate
/// and the window says converted. Neither of those is a decision this crate
/// has any business making on its own.
pub fn capabilities() -> Option<chilpa_audio::negotiate::DeviceCaps> {
    let (client, _) = hello()?;
    let rate = client.sample_rate() as u32;
    drop(client);
    Some(chilpa_audio::negotiate::DeviceCaps {
        sample_rates: vec![rate],
        // The graph deals in f32 and the conversion into it costs nothing;
        // what costs is the rate, and that is above.
        formats: vec![
            SampleFormat::S16,
            SampleFormat::S24Packed,
            SampleFormat::S24In32,
            SampleFormat::S32,
            SampleFormat::F32,
        ],
        channels: vec![1, 2, 4, 6, 8],
        probed: true,
    })
}

impl OutputBackend for Jack {
    fn info(&self) -> BackendInfo {
        let Some((client, _)) = hello() else {
            return BackendInfo::missing(
                BackendKind::Jack,
                chilpa_i18n::t(
                    "no JACK graph is running; start one, or install pipewire-jack to have \
                     PipeWire answer for it",
                ),
            );
        };
        let rate = client.sample_rate();
        // Nothing is claimed about who is answering. On most machines it
        // is PipeWire's replacement library rather than jack2, and it would
        // be worth saying -- but the only signals available are the shapes
        // of port names, and a guess dressed as a fact is worse than saying
        // nothing. What actually matters is in the caveat, and that is read
        // from the graph itself: the rate it runs at.
        let provider = String::new();
        drop(client);
        BackendInfo {
            kind: BackendKind::Jack,
            present: true,
            missing: String::new(),
            provider,
            fidelity: Fidelity::Never,
            caveat: chilpa_i18n::t_args(
                "A JACK graph runs at one rate and every program in it runs at that rate -- this \
                 one is at {} Hz. A track recorded at any other rate is resampled before Chilpa \
                 has any say, and no setting here changes it. JACK is for routing audio between \
                 programs; for a file delivered to a DAC untouched, choose the card itself.",
                &[&rate.to_string()],
            ),
        }
    }

    /// The one server that can be measured without taking anything: the
    /// graph's rate is published and every client is told it.
    fn caps(&self, _id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, SinkError> {
        capabilities().ok_or_else(|| {
            SinkError::Backend(chilpa_i18n::t("no JACK graph is running"))
        })
    }

    fn outputs(&self) -> Vec<BackendOutput> {
        let Some((client, _)) = hello() else { return Vec::new() };
        // The graph's physical inputs: the ports that are an actual way out
        // of the machine. Everything else is another program, and sending
        // music into one of those is not choosing an output.
        let ports = client.ports(
            None,
            Some(jack::AudioIn::default().jack_port_type()),
            jack::PortFlags::IS_INPUT | jack::PortFlags::IS_PHYSICAL,
        );
        let rate = client.sample_rate();
        drop(client);

        // Ports come one per channel ("...:playback_1"), and what somebody
        // picks is a device, not a channel. They are grouped back by the
        // name in front of the colon.
        let mut devices: Vec<String> = Vec::new();
        for port in ports {
            let device = port.split_once(':').map(|(d, _)| d.to_string()).unwrap_or(port);
            if !devices.contains(&device) {
                devices.push(device);
            }
        }
        devices
            .into_iter()
            .map(|device| BackendOutput {
                detail: chilpa_i18n::t_args("the graph runs at {} Hz", &[&rate.to_string()]),
                label: device.clone(),
                id: device,
                // Never, and it is the graph and not the device: the same
                // DAC is exact through the card and is not through here.
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
        // There is nothing to ask for exclusively: a graph is shared by
        // definition, and pretending otherwise is how a player ends up
        // claiming bit-perfect over a resampler.
        JackSink::open(id, format).map(|s| Box::new(s) as Box<dyn AudioSink + Send>)
    }
}

/// What the writing side and the graph's thread share.
///
/// A plain buffer behind a lock rather than a lock-free ring: the graph's
/// callback must not block, and it does not -- it takes the lock or gives
/// up and plays silence for that period, which is one underrun and not a
/// stall. A ring that is never contended is not worth the code until this
/// one is measured and found wanting.
struct Shared {
    /// Samples waiting to go out, already interleaved as f32, which is what
    /// JACK deals in.
    waiting: Mutex<std::collections::VecDeque<f32>>,
    /// Set when the graph stopped calling: the server went away.
    lost: AtomicBool,
}

/// A stream into the graph.
pub struct JackSink {
    stream: StreamFormat,
    shared: Arc<Shared>,
    /// Kept so the client stays in the graph; dropping it leaves.
    _active: Option<jack::AsyncClient<Watch, Handler>>,
    /// How many samples may wait before the writer is asked to slow down.
    /// A quarter of a second, which is long enough to ride out a scheduling
    /// hiccup and short enough that stopping is not heard as a tail.
    ceiling: usize,
}

/// What the graph says when it stops.
///
/// Without this a server that goes away leaves the writer filling a buffer
/// nobody reads: every write succeeds, the position advances, and the track
/// plays out in silence. That is the shape of failure the sink's own
/// documentation was written against.
struct Watch {
    shared: Arc<Shared>,
}

impl jack::NotificationHandler for Watch {
    unsafe fn shutdown(&mut self, _: jack::ClientStatus, _: &str) {
        self.shared.lost.store(true, Ordering::Relaxed);
    }
}

/// What the graph calls.
struct Handler {
    shared: Arc<Shared>,
    ports: Vec<jack::Port<jack::AudioOut>>,
}

impl jack::ProcessHandler for Handler {
    fn process(&mut self, _: &jack::Client, scope: &jack::ProcessScope) -> jack::Control {
        let frames = scope.n_frames() as usize;
        // Taken once for the whole period. Failing to take it is an
        // underrun, not a reason to wait: the graph is on a deadline.
        let mut waiting = match self.shared.waiting.try_lock() {
            Ok(w) => w,
            Err(_) => {
                for port in &mut self.ports {
                    port.as_mut_slice(scope).fill(0.0);
                }
                return jack::Control::Continue;
            }
        };
        // Interleaved on the way in, one port per channel on the way out.
        // Running dry gives silence for the rest of the period, which is a
        // gap somebody hears once, not a stall the graph never recovers
        // from.
        for frame in 0..frames {
            for port in self.ports.iter_mut() {
                let sample = waiting.pop_front().unwrap_or(0.0);
                port.as_mut_slice(scope)[frame] = sample;
            }
        }
        jack::Control::Continue
    }
}

impl JackSink {
    fn open(device: &str, stream: StreamFormat) -> Result<Self, SinkError> {
        let (client, _) = jack::Client::new(CLIENT, jack::ClientOptions::NO_START_SERVER)
            .map_err(|e| SinkError::Backend(e.to_string()))?;
        // The graph's rate is the graph's, not ours, and by here it has
        // already been agreed: `capabilities` publishes the one rate, so
        // whoever negotiated either matched it or converted to it on
        // purpose. This is the last check that nothing slipped past.
        if client.sample_rate() as u32 != stream.sample_rate_hz {
            return Err(SinkError::Backend(chilpa_i18n::t_args(
                "the JACK graph runs at {} Hz and this track is {} Hz; JACK cannot play it \
                 untouched",
                &[&client.sample_rate().to_string(), &stream.sample_rate_hz.to_string()],
            )));
        }
        let mut ports = Vec::new();
        for channel in 0..stream.channels {
            let port = client
                .register_port(&format!("out_{}", channel + 1), jack::AudioOut::default())
                .map_err(|e| SinkError::Backend(e.to_string()))?;
            ports.push(port);
        }
        let shared = Arc::new(Shared {
            waiting: Mutex::new(std::collections::VecDeque::new()),
            lost: AtomicBool::new(false),
        });
        let handler = Handler { shared: Arc::clone(&shared), ports };
        let active = client
            .activate_async(Watch { shared: Arc::clone(&shared) }, handler)
            .map_err(|e| SinkError::Backend(e.to_string()))?;

        // Connected to what was asked for, by name. A stream that is not
        // connected plays into nothing and sounds exactly like a broken
        // cable, so a connection that fails is a failure and not a warning.
        let wanted: Vec<String> = active
            .as_client()
            .ports(Some(&format!("^{device}:")), Some(jack::AudioIn::default().jack_port_type()),
                   jack::PortFlags::IS_INPUT);
        for (channel, target) in wanted.iter().take(stream.channels as usize).enumerate() {
            active
                .as_client()
                .connect_ports_by_name(&format!("{CLIENT}:out_{}", channel + 1), target)
                .map_err(|e| SinkError::Backend(e.to_string()))?;
        }

        Ok(JackSink {
            stream,
            shared,
            _active: Some(active),
            ceiling: (stream.sample_rate_hz as usize / 4) * stream.channels as usize,
        })
    }
}

impl AudioSink for JackSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        if self.shared.lost.load(Ordering::Relaxed) {
            return Err(SinkError::DeviceLost);
        }
        let samples = to_f32(frames, self.stream.sample_format);
        let mut waiting = self.shared.waiting.lock().map_err(|_| SinkError::Closed)?;
        if waiting.len() >= self.ceiling {
            // Full. Nothing was taken and the caller comes back, which is
            // what the sink's contract says a partial write means.
            return Ok(0);
        }
        waiting.extend(samples);
        Ok(frames.len())
    }

    fn close(&mut self) -> Result<(), SinkError> {
        // Leaving the graph is dropping the client, and it is not waited on:
        // stopping never waits for a device.
        self._active = None;
        Ok(())
    }
}

/// The samples as JACK wants them.
///
/// Converting here is not a loss of fidelity and would not be worth
/// mentioning except that it looks like one: JACK's own currency is f32, so
/// every program in a graph is already there. It is what the graph's rate
/// does to the track that costs, and that is said before anything opens.
fn to_f32(bytes: &[u8], format: SampleFormat) -> Vec<f32> {
    match format {
        SampleFormat::S16 => bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32_768.0)
            .collect(),
        SampleFormat::S24Packed => bytes
            .chunks_exact(3)
            .map(|c| {
                let v = i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8;
                v as f32 / 8_388_608.0
            })
            .collect(),
        SampleFormat::S24In32 | SampleFormat::S32 => bytes
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2_147_483_648.0)
            .collect(),
        SampleFormat::F32 => bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_stack_never_claims_to_be_exact() {
        let info = Jack.info();
        if info.present {
            assert_eq!(info.fidelity, Fidelity::Never);
            assert!(!info.caveat.is_empty());
            assert!(Jack.outputs().iter().all(|o| !o.bit_perfect));
        } else {
            assert!(!info.missing.is_empty());
        }
    }

    #[test]
    fn a_track_at_another_rate_is_refused_rather_than_resampled() {
        // The graph decides the rate and a client is told it. Playing
        // anyway means resampling, and doing that quietly is what every
        // other player does here.
        if Jack.info().present {
            let far_off = StreamFormat::new(SampleFormat::S16, 1, 2);
            assert!(Jack.open("nothing", far_off, false).is_err());
        }
    }

    #[test]
    fn the_samples_reach_jacks_own_currency_unharmed() {
        // Full scale, silence and the bottom, through each width.
        let s16: Vec<u8> = [0i16, 32_767, -32_768].iter().flat_map(|v| v.to_le_bytes()).collect();
        let out = to_f32(&s16, SampleFormat::S16);
        assert_eq!(out[0], 0.0);
        assert!((out[1] - 0.999_97).abs() < 0.001, "{}", out[1]);
        assert_eq!(out[2], -1.0);

        let s32: Vec<u8> = [0i32, i32::MAX, i32::MIN].iter().flat_map(|v| v.to_le_bytes()).collect();
        let out = to_f32(&s32, SampleFormat::S32);
        assert_eq!(out[0], 0.0);
        assert!((out[1] - 1.0).abs() < 0.001);
        assert_eq!(out[2], -1.0);

        let f32s: Vec<u8> = [0.0f32, 0.5, -1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(to_f32(&f32s, SampleFormat::F32), vec![0.0, 0.5, -1.0]);
    }
}
